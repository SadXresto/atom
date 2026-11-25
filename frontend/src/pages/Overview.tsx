import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  Activity,
  ArrowRight,
  Boxes,
  Cpu,
  Database,
  Download,
  Gauge,
  HardDrive,
  Layers,
  MemoryStick,
  Power,
  PowerOff,
  RefreshCw,
  Zap,
} from "lucide-react";
import { api } from "../lib/api";
import { bytes, duration, percent, relativeTime } from "../lib/format";
import { useApi } from "../lib/hooks";
import type { ModelRecord, Overview as OverviewData } from "../lib/types";
import { BackendGate } from "../components/Layout";
import { JobStatusBadge } from "../components/JobConsole";
import { ModelSelect } from "../components/ModelSelect";
import { Badge, Button, EmptyState, KV, Panel, PageHeader, Progress, Stat, toast, cx } from "../components/ui";

export default function Overview() {
  const navigate = useNavigate();
  const { data, error, reload } = useApi<OverviewData>("/overview", 5000);
  const { data: modelsData, reload: reloadModels } = useApi<{ models: ModelRecord[] }>("/models", 0);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [releaseBusy, setReleaseBusy] = useState(false);

  const models = modelsData?.models ?? [];
  const runtime = data?.runtime;
  const system = data?.system;
  const last = data?.last_inference;

  useEffect(() => {
    if (!selected && models.length) setSelected(models[0].id);
  }, [models, selected]);

  const load = async () => {
    if (!selected) return;
    setBusy(true);
    try {
      const outcome = await api<{ status: { model_label: string }; warnings: string[] }>("/runtime/load", {
        method: "POST",
        body: JSON.stringify({ model_id: selected }),
      });
      toast(`Loaded ${outcome.status.model_label}`, "ok");
      outcome.warnings.forEach((warning) => toast(warning, "info"));
      void reload();
      void reloadModels();
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(false);
    }
  };

  const unload = async () => {
    setBusy(true);
    try {
      await api("/runtime/unload", { method: "POST" });
      toast("Model unloaded; memory reclaimed by the OS", "ok");
      void reload();
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(false);
    }
  };

  const release = async () => {
    setReleaseBusy(true);
    try {
      const result = await api<{ released: string }>("/runtime/release", { method: "POST" });
      toast(result.released, "ok");
      void reload();
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setReleaseBusy(false);
    }
  };

  const usedRatio = system ? system.used_memory / Math.max(1, system.total_memory) : 0;

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="Local model operations"
        title="Overview"
        description="Import GGUF models, tune how they run, permanently compress them into separate files, and compare real measured performance."
        actions={
          <>
            <Button icon={<RefreshCw size={13} />} onClick={() => void reload()} size="sm">
              Refresh
            </Button>
            <Button variant="primary" icon={<Download size={13} />} onClick={() => navigate("/library?import=1")}>
              Import model
            </Button>
          </>
        }
      />

      {error && (
        <div className="mb-4 rounded-xl border border-bad/30 bg-bad/[0.07] px-3.5 py-2.5 text-[12.5px] text-[#ffc0c0]">
          {error.message}
        </div>
      )}
      <BackendGate backend={data?.backend} />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat
          label="Active model"
          value={runtime?.loaded ? runtime.model_label ?? "loaded" : "none"}
          sub={
            runtime?.loaded ? (
              <span className="flex items-center gap-1.5">
                <span className="h-1.5 w-1.5 rounded-full bg-ok" /> loaded in {duration(runtime.load_ms)}
              </span>
            ) : (
              "No model held by llama-server"
            )
          }
          tone={runtime?.loaded ? "accent" : "default"}
          icon={<Cpu size={13} />}
          mono={false}
        />
        <Stat
          label="System memory"
          value={system ? `${percent(usedRatio * 100, 0)}` : "—"}
          sub={system ? `${bytes(system.used_memory)} of ${bytes(system.total_memory)} used` : undefined}
          icon={<MemoryStick size={13} />}
          tone={usedRatio > 0.9 ? "bad" : usedRatio > 0.75 ? "warn" : "default"}
        />
        <Stat
          label="Model process RSS"
          value={system?.model_process_rss ? bytes(system.model_process_rss) : "—"}
          sub={
            system?.model_process_pid
              ? `pid ${system.model_process_pid} · resident set`
              : "No llama-server process"
          }
          icon={<Database size={13} />}
        />
        <Stat
          label="Last throughput"
          value={last ? `${last.tokens_per_second.toFixed(1)} tok/s` : "—"}
          sub={last ? `${last.completion_tokens} tokens · TTFT ${last.ttft_ms} ms` : "Run a generation to measure"}
          icon={<Gauge size={13} />}
          tone={last ? "ok" : "default"}
        />
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-[1.35fr_1fr]">
        <Panel
          title="Runtime"
          subtitle="Live optimization only changes how the model runs. The GGUF file on disk is never modified."
          icon={<Zap size={14} />}
          actions={
            <Badge tone={runtime?.loaded ? "ok" : "neutral"}>
              {runtime?.loaded ? (runtime.alive ? "serving" : "process exited") : "idle"}
            </Badge>
          }
        >
          {runtime?.loaded ? (
            <div className="space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <div className="text-[13.5px] font-medium text-ink">{runtime.model_label}</div>
                  <div className="mt-0.5 font-mono text-[11px] text-muted">{runtime.base_url}</div>
                </div>
                <div className="flex items-center gap-2">
                  <Button size="sm" onClick={() => navigate("/inference")} icon={<ArrowRight size={12} />}>
                    Open workspace
                  </Button>
                  <Button size="sm" variant="danger" icon={<PowerOff size={12} />} onClick={unload} busy={busy}>
                    Unload
                  </Button>
                </div>
              </div>
              <div className="grid gap-x-6 sm:grid-cols-2">
                <KV label="Context" value={runtime.options ? `${runtime.options.context_length.toLocaleString()} tokens` : "—"} mono />
                <KV label="Threads" value={runtime.options?.threads ?? "—"} mono />
                <KV label="KV cache" value={runtime.options ? `${runtime.options.kv_cache_type_k}/${runtime.options.kv_cache_type_v}` : "—"} mono />
                <KV label="Mmap" value={runtime.options?.use_mmap ? "enabled" : "disabled"} />
                <KV label="Load time" value={duration(runtime.load_ms)} mono />
                <KV label="Backend build" value={runtime.backend_version ?? "not reported"} mono />
              </div>
              <div className="rounded-lg border border-line bg-black/25 px-3 py-2 text-[11.5px] leading-relaxed text-muted">
                Tuning applied at spawn: {runtime.options ? describeTuning(runtime.options) : "—"}
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <p className="text-[12.5px] leading-relaxed text-muted">
                {models.length
                  ? "Choose a library model to start a llama-server process with measured load time and memory."
                  : "The library is empty. Import a GGUF file to begin."}
              </p>
              {models.length > 0 ? (
                <div className="flex flex-wrap items-center gap-2">
                  <div className="min-w-[240px] flex-1">
                    <ModelSelect models={models} value={selected} onChange={setSelected} />
                  </div>
                  <Button variant="primary" onClick={load} busy={busy} icon={<Power size={13} />}>
                    Load
                  </Button>
                </div>
              ) : (
                <Button variant="primary" onClick={() => navigate("/library?import=1")} icon={<Download size={13} />}>
                  Import a model
                </Button>
              )}
              <div className="flex items-center gap-2 text-[11.5px] text-muted">
                <PowerOff size={12} />
                <span>Unloading reaps the child process so the OS reclaims its memory.</span>
              </div>
            </div>
          )}
        </Panel>

        <Panel title="System" subtitle="Measured by ATOM every 1.5 s." icon={<Activity size={14} />}>
          {system ? (
            <div className="space-y-3">
              <div>
                <div className="mb-1.5 flex items-baseline justify-between text-[11.5px]">
                  <span className="text-muted">RAM in use</span>
                  <span className="mono-num text-ink-dim">
                    {bytes(system.used_memory)} / {bytes(system.total_memory)}
                  </span>
                </div>
                <Progress value={usedRatio} tone={usedRatio > 0.9 ? "bad" : usedRatio > 0.75 ? "warn" : "accent"} />
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div className="rounded-lg border border-line bg-black/20 px-2.5 py-2">
                  <div className="label">CPU</div>
                  <div className="mono-num mt-1 text-[16px] text-ink">{system.system_cpu_percent.toFixed(1)}%</div>
                </div>
                <div className="rounded-lg border border-line bg-black/20 px-2.5 py-2">
                  <div className="label">ATOM process</div>
                  <div className="mono-num mt-1 text-[16px] text-ink">{bytes(system.process_rss)}</div>
                </div>
              </div>
              <KV label="Available" value={bytes(system.available_memory)} mono />
              <KV label="Load average" value={system.load_average.map((v) => v.toFixed(2)).join("  ")} mono />
              <Button size="sm" variant="ghost" onClick={() => navigate("/monitor")} icon={<Activity size={12} />}>
                Open system monitor
              </Button>
            </div>
          ) : (
            <EmptyState compact title="Waiting for the first sample" />
          )}
        </Panel>
      </div>

      <div className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <QuickAction
          icon={<Layers size={15} />}
          title="Compression Studio"
          body="Create a separate, permanently quantized model."
          onClick={() => navigate("/compression")}
        />
        <QuickAction
          icon={<Zap size={15} />}
          title="Tune runtime"
          body="Context, threads, mmap and KV cache precision."
          onClick={() => navigate("/inference")}
        />
        <QuickAction
          icon={<Gauge size={15} />}
          title="Comparison Lab"
          body="Run identical prompts against two models."
          onClick={() => navigate("/comparison")}
        />
        <QuickAction
          icon={<Boxes size={15} />}
          title="Model Library"
          body="Inspect tensors, licenses and checksums."
          onClick={() => navigate("/library")}
        />
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-[1.35fr_1fr]">
        <Panel
          title="Recent activity"
          subtitle="Background jobs, with real progress as reported by the tools."
          icon={<Activity size={14} />}
          actions={
            <Button size="sm" onClick={() => navigate("/history")} icon={<ArrowRight size={12} />}>
              History
            </Button>
          }
        >
          {!data?.recent_jobs.length ? (
            <EmptyState
              compact
              icon={<HardDrive size={16} />}
              title="No jobs yet"
              description="Imports, quantization runs and benchmark comparisons appear here."
            />
          ) : (
            <div className="divide-y divide-line">
              {data.recent_jobs.map((job) => (
                <div key={job.id} className="flex items-center gap-3 py-2.5">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[12.5px] text-ink-dim">{job.title}</div>
                    <div className="mt-0.5 flex items-center gap-2 text-[11px] text-muted">
                      <span>{job.stage}</span>
                      <span>·</span>
                      <span>{relativeTime(job.started_at ?? job.created_at)}</span>
                    </div>
                  </div>
                  <div className="w-24 shrink-0">
                    <Progress
                      value={job.progress}
                      height={4}
                      tone={job.status === "failed" ? "bad" : job.status === "succeeded" ? "ok" : "accent"}
                    />
                  </div>
                  <JobStatusBadge status={job.status} />
                </div>
              ))}
            </div>
          )}
        </Panel>

        <div className="space-y-4">
          <Panel title="Library footprint" icon={<Database size={14} />}>
            <div className="space-y-3">
              <div className="flex items-end justify-between">
                <div>
                  <div className="label">Total on disk</div>
                  <div className="mono-num mt-1 text-[20px] text-ink">{bytes(data?.disk.library_bytes ?? 0)}</div>
                </div>
                <div className="text-right">
                  <div className="label">Compressed</div>
                  <div className="mono-num mt-1 text-[14px] text-accent-2">{bytes(data?.disk.compressed_bytes ?? 0)}</div>
                </div>
              </div>
              <div className="grid grid-cols-3 gap-2 text-center">
                <MiniStat label="models" value={data?.counts.models ?? 0} />
                <MiniStat label="original" value={data?.counts.originals ?? 0} />
                <MiniStat label="compressed" value={data?.counts.compressed ?? 0} />
              </div>
              <Button size="sm" icon={<RefreshCw size={12} />} onClick={() => void reload()} className="w-full">
                Re-check library
              </Button>
            </div>
          </Panel>

          <Panel title="Toolchain" icon={<Cpu size={14} />}>
            <div className="space-y-2 text-[12px]">
              <ToolRow name="llama-server" path={data?.backend.server?.path} version={data?.backend.server?.version} />
              <ToolRow name="llama-quantize" path={data?.backend.quantize?.path} version={data?.backend.quantize?.version} />
            </div>
            <div className="mt-3 flex items-center justify-between gap-2">
              <span className="text-[11px] text-muted">
                {data?.backend.inference_ready && data?.backend.compression_ready
                  ? "All required binaries found."
                  : "Set explicit paths in Settings if ATOM cannot find them on PATH."}
              </span>
              <Button size="sm" onClick={() => navigate("/settings")}>
                Settings
              </Button>
            </div>
          </Panel>
        </div>
      </div>

      {runtime?.loaded && (
        <div className="mt-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-line bg-white/[0.015] px-4 py-3">
          <div className="text-[12px] text-muted">
            Release buffered samples and counters? This unloads the model and clears monitoring buffers; model files are untouched.
          </div>
          <Button size="sm" onClick={release} busy={releaseBusy} icon={<PowerOff size={12} />}>
            Release resources
          </Button>
        </div>
      )}
    </div>
  );
}

function describeTuning(options: {
  context_length: number;
  threads: number;
  batch_size: number;
  use_mmap: boolean;
  use_mlock: boolean;
  flash_attention: boolean;
  kv_cache_type_k: string;
  kv_cache_type_v: string;
  gpu_layers: number;
}) {
  const parts = [
    `ctx ${options.context_length}`,
    `${options.threads} threads`,
    `batch ${options.batch_size}`,
    options.use_mmap ? "mmap" : "no-mmap",
    `kv ${options.kv_cache_type_k}/${options.kv_cache_type_v}`,
  ];
  if (options.use_mlock) parts.push("mlock");
  if (options.flash_attention) parts.push("flash-attn");
  if (options.gpu_layers) parts.push(`${options.gpu_layers} GPU layers`);
  return parts.join(" · ");
}

function QuickAction({
  icon,
  title,
  body,
  onClick,
}: {
  icon: React.ReactNode;
  title: string;
  body: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cx(
        "panel-flat group flex items-start gap-3 px-3.5 py-3 text-left transition-colors",
        "hover:border-line-strong hover:bg-white/[0.03]",
      )}
    >
      <span className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-line bg-white/[0.03] text-accent">
        {icon}
      </span>
      <span className="min-w-0">
        <span className="block text-[12.5px] font-medium text-ink-dim group-hover:text-ink">{title}</span>
        <span className="mt-0.5 block text-[11.5px] leading-snug text-muted">{body}</span>
      </span>
    </button>
  );
}

function MiniStat({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-lg border border-line bg-black/20 px-2 py-1.5">
      <div className="mono-num text-[15px] text-ink">{value}</div>
      <div className="text-[10.5px] uppercase tracking-wide text-muted">{label}</div>
    </div>
  );
}

function ToolRow({ name, path, version }: { name: string; path?: string | null; version?: string | null }) {
  return (
    <div className="flex items-start justify-between gap-3 border-b border-line/60 py-1.5 last:border-0">
      <div className="min-w-0">
        <div className="text-[12px] text-ink-dim">{name}</div>
        <div className="truncate font-mono text-[10.5px] text-muted">{path ?? "not found"}</div>
      </div>
      <Badge tone={path ? "ok" : "warn"}>{path ? "found" : "missing"}</Badge>
    </div>
  );
}
