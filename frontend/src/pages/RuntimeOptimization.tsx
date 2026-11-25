import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowRight, Gauge, MemoryStick, Power, PowerOff, RefreshCw, Save, Zap } from "lucide-react";
import { api } from "../lib/api";
import { bytes, duration, percent, relativeTime } from "../lib/format";
import { useApi } from "../lib/hooks";
import type { Config, LoadOutcome, ModelRecord, RuntimeStatus } from "../lib/types";
import { ModelSelect } from "../components/ModelSelect";
import {
  Badge,
  Banner,
  Button,
  EmptyState,
  Field,
  KV,
  PageHeader,
  Panel,
  Progress,
  Select,
  Stat,
  TextInput,
  Toggle,
  cx,
  toast,
} from "../components/ui";

const KV_TYPES = ["f16", "f32", "q8_0", "q4_0", "q4_1", "q5_0", "q5_1", "iq4_nl"];

const PRESETS = [
  {
    id: "balanced",
    name: "Balanced",
    note: "f16 KV cache, mmap, flash attention",
    values: { context_length: 4096, batch_size: 512, kv_cache_type_k: "f16", kv_cache_type_v: "f16", gpu_layers: 0, use_mmap: true, use_mlock: false, flash_attention: true },
  },
  {
    id: "low-memory",
    name: "Low memory",
    note: "q8_0 KV cache, smaller batch and context",
    values: { context_length: 2048, batch_size: 256, kv_cache_type_k: "q8_0", kv_cache_type_v: "q8_0", gpu_layers: 0, use_mmap: true, use_mlock: false, flash_attention: true },
  },
  {
    id: "long-context",
    name: "Long context",
    note: "16k context with quantized KV cache",
    values: { context_length: 16384, batch_size: 512, kv_cache_type_k: "q8_0", kv_cache_type_v: "q8_0", gpu_layers: 0, use_mmap: true, use_mlock: false, flash_attention: true },
  },
  {
    id: "gpu",
    name: "GPU offload",
    note: "35 layers on the GPU when the build supports it",
    values: { context_length: 8192, batch_size: 1024, kv_cache_type_k: "f16", kv_cache_type_v: "f16", gpu_layers: 35, use_mmap: true, use_mlock: false, flash_attention: true },
  },
];

interface LoadRecord {
  at: string;
  model: string;
  load_ms: number;
  ctx: number;
  batch: number;
  kv: string;
  gpu: number;
  threads: number;
}

const HISTORY_KEY = "atom.runtime.loads";

export default function RuntimeOptimization() {
  const navigate = useNavigate();
  const { data: modelsData, reload: reloadModels } = useApi<{ models: ModelRecord[] }>("/models");
  const { data: runtimeData, reload: reloadRuntime } = useApi<{ runtime: RuntimeStatus; tuning: string[] }>("/runtime", 4000);
  const { data: config } = useApi<Config>("/settings");
  const models = modelsData?.models ?? [];
  const runtime = runtimeData?.runtime;

  const [modelId, setModelId] = useState<string | null>(null);
  const [preset, setPreset] = useState("balanced");
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<LoadOutcome | null>(null);
  const [history, setHistory] = useState<LoadRecord[]>(() => {
    try {
      return JSON.parse(window.localStorage.getItem(HISTORY_KEY) ?? "[]") as LoadRecord[];
    } catch {
      return [];
    }
  });
  const [options, setOptions] = useState({
    context_length: 4096,
    threads: config?.threads ?? 4,
    batch_size: 512,
    gpu_layers: 0,
    use_mmap: true,
    use_mlock: false,
    flash_attention: true,
    kv_cache_type_k: "f16",
    kv_cache_type_v: "f16",
    memory_budget_mb: config?.memory_budget_mb ?? 0,
  });

  useEffect(() => {
    if (!modelId && models.length) setModelId(models[0].id);
  }, [models, modelId]);

  useEffect(() => {
    if (config) setOptions((prev) => ({ ...prev, threads: config.threads }));
  }, [config]);

  const model = useMemo(() => models.find((m) => m.id === modelId) ?? null, [models, modelId]);

  const estimate = useMemo(() => {
    if (!model) return null;
    const weights = model.summary.weights_bytes;
    const kvPerToken = model.summary.kv_bytes_per_token ?? null;
    const kvFactor = options.kv_cache_type_k === "f32" ? 4 : options.kv_cache_type_k === "q8_0" ? 1.0625 : options.kv_cache_type_k === "q4_0" ? 0.5625 : 2;
    const kv = kvPerToken === null ? null : Math.round(kvPerToken * options.context_length * (kvFactor / 2));
    const overhead = 256 * 1024 * 1024;
    const total = weights + (kv ?? 0) + overhead;
    return { weights, kv, overhead, total, available: model.summary ? null : null };
  }, [model, options.context_length, options.kv_cache_type_k]);

  const applyPreset = (id: string) => {
    const found = PRESETS.find((p) => p.id === id);
    if (!found) return;
    setPreset(id);
    setOptions((prev) => ({ ...prev, ...found.values }));
  };

  const load = async () => {
    if (!modelId) return;
    setBusy(true);
    try {
      const result = await api<LoadOutcome>("/runtime/load", {
        method: "POST",
        body: JSON.stringify({
          model_id: modelId,
          context_length: options.context_length,
          threads: options.threads,
          batch_size: options.batch_size,
          gpu_layers: options.gpu_layers,
          use_mmap: options.use_mmap,
          use_mlock: options.use_mlock,
          flash_attention: options.flash_attention,
          kv_cache_type_k: options.kv_cache_type_k,
          kv_cache_type_v: options.kv_cache_type_v,
          memory_budget_mb: options.memory_budget_mb > 0 ? options.memory_budget_mb : null,
        }),
      });
      setOutcome(result);
      result.warnings.forEach((warning) => toast(warning, "info"));
      toast(
        `Loaded in ${duration(result.status.load_ms)}${result.estimate.kv_cache_bytes ? ` · ${bytes(result.estimate.total_bytes)} estimated` : ""}`,
        "ok",
      );
      const record: LoadRecord = {
        at: new Date().toISOString(),
        model: result.status.model_label ?? model?.label ?? "model",
        load_ms: result.status.load_ms ?? 0,
        ctx: options.context_length,
        batch: options.batch_size,
        kv: `${options.kv_cache_type_k}/${options.kv_cache_type_v}`,
        gpu: options.gpu_layers,
        threads: options.threads,
      };
      const next = [record, ...history].slice(0, 12);
      setHistory(next);
      window.localStorage.setItem(HISTORY_KEY, JSON.stringify(next));
      void reloadRuntime();
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
      toast("Model unloaded; the OS reclaimed its memory", "ok");
      void reloadRuntime();
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(false);
    }
  };

  const saveDefaults = async () => {
    if (!config) return;
    try {
      await api("/settings", {
        method: "PUT",
        body: JSON.stringify({
          ...config,
          context_length: options.context_length,
          threads: options.threads,
          batch_size: options.batch_size,
          gpu_layers: options.gpu_layers,
          use_mmap: options.use_mmap,
          use_mlock: options.use_mlock,
          flash_attention: options.flash_attention,
          kv_cache_type_k: options.kv_cache_type_k,
          kv_cache_type_v: options.kv_cache_type_v,
          memory_budget_mb: options.memory_budget_mb > 0 ? options.memory_budget_mb : null,
        }),
      });
      toast("Runtime defaults saved", "ok");
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="Reversible tuning"
        title="Live Runtime Optimization"
        description="Change how llama-server loads and runs a model, then measure the real cost. Nothing here modifies the GGUF on disk."
        actions={
          <>
            <Button onClick={() => navigate("/monitor")} icon={<Gauge size={13} />}>
              System Monitor
            </Button>
            <Button variant="primary" onClick={() => navigate("/inference")} icon={<ArrowRight size={13} />}>
              Open chat
            </Button>
          </>
        }
      />

      {!runtime?.loaded && models.length === 0 && (
        <Banner tone="info" title="No models in the library">
          Import a GGUF file first; runtime tuning needs a model to load.
        </Banner>
      )}

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat
          label="Runtime"
          value={runtime?.loaded ? "loaded" : "idle"}
          sub={runtime?.loaded ? `${runtime.model_label}` : "no llama-server process"}
          tone={runtime?.loaded ? "ok" : "default"}
        />
        <Stat label="Load time" value={runtime?.loaded ? duration(runtime.load_ms) : "—"} sub="measured to /health" />
        <Stat
          label="Model process RSS"
          value={runtime?.rss_bytes ? bytes(runtime.rss_bytes) : "—"}
          sub={runtime?.pid ? `pid ${runtime.pid}` : "not running"}
        />
        <Stat label="Threads" value={runtime?.options?.threads ?? options.threads} sub="spawn argument" />
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,1fr)]">
        <Panel title="Configuration" subtitle="Applied when the model is spawned, never to the file." icon={<Zap size={14} />}>
          <div className="space-y-3">
            <Field label="Model">
              <ModelSelect models={models} value={modelId} onChange={setModelId} />
            </Field>

            <div>
              <div className="label mb-1.5">Presets</div>
              <div className="grid grid-cols-2 gap-2">
                {PRESETS.map((item) => (
                  <button
                    key={item.id}
                    onClick={() => applyPreset(item.id)}
                    className={cx(
                      "rounded-lg border px-2.5 py-2 text-left transition-colors",
                      preset === item.id ? "border-accent/45 bg-accent/[0.08]" : "border-line bg-white/[0.015] hover:border-line-strong",
                    )}
                  >
                    <div className="text-[12px] font-medium text-ink">{item.name}</div>
                    <div className="text-[10.5px] text-muted">{item.note}</div>
                  </button>
                ))}
              </div>
            </div>

            <div className="grid grid-cols-2 gap-2">
              <Field label="Context length">
                <TextInput
                  type="number"
                  value={options.context_length}
                  onChange={(e) => setOptions({ ...options, context_length: Number(e.target.value) })}
                />
              </Field>
              <Field label="Threads">
                <TextInput
                  type="number"
                  value={options.threads}
                  onChange={(e) => setOptions({ ...options, threads: Number(e.target.value) })}
                />
              </Field>
              <Field label="Batch size">
                <TextInput
                  type="number"
                  value={options.batch_size}
                  onChange={(e) => setOptions({ ...options, batch_size: Number(e.target.value) })}
                />
              </Field>
              <Field label="GPU layers" hint="-ngl">
                <TextInput
                  type="number"
                  value={options.gpu_layers}
                  onChange={(e) => setOptions({ ...options, gpu_layers: Number(e.target.value) })}
                />
              </Field>
              <Field label="KV cache K">
                <Select
                  value={options.kv_cache_type_k}
                  onChange={(e) => setOptions({ ...options, kv_cache_type_k: e.target.value })}
                >
                  {KV_TYPES.map((type) => (
                    <option key={type}>{type}</option>
                  ))}
                </Select>
              </Field>
              <Field label="KV cache V">
                <Select
                  value={options.kv_cache_type_v}
                  onChange={(e) => setOptions({ ...options, kv_cache_type_v: e.target.value })}
                >
                  {KV_TYPES.map((type) => (
                    <option key={type}>{type}</option>
                  ))}
                </Select>
              </Field>
              <Field label="Memory budget (MiB)" hint="0 = none" className="col-span-2">
                <TextInput
                  type="number"
                  value={options.memory_budget_mb}
                  onChange={(e) => setOptions({ ...options, memory_budget_mb: Number(e.target.value) })}
                />
              </Field>
            </div>

            <div className="rounded-lg border border-line bg-white/[0.015] px-3 py-1">
              <Toggle
                checked={options.use_mmap}
                onChange={(next) => setOptions({ ...options, use_mmap: next })}
                label="mmap"
                description="Map the weights instead of reading them eagerly."
              />
              <Toggle
                checked={options.use_mlock}
                onChange={(next) => setOptions({ ...options, use_mlock: next })}
                label="mlock"
                description="Keep pages resident (needs the OS permission)."
              />
              <Toggle
                checked={options.flash_attention}
                onChange={(next) => setOptions({ ...options, flash_attention: next })}
                label="flash attention"
                description="Faster attention where the backend build supports it."
              />
            </div>

            <div className="flex flex-wrap gap-2">
              <Button variant="primary" onClick={load} busy={busy} disabled={!modelId} icon={<Power size={13} />}>
                Load with these settings
              </Button>
              <Button onClick={unload} busy={busy} disabled={!runtime?.loaded} icon={<PowerOff size={13} />}>
                Unload
              </Button>
              <Button onClick={saveDefaults} icon={<Save size={13} />}>
                Save as defaults
              </Button>
            </div>
          </div>
        </Panel>

        <div className="space-y-4">
          <Panel title="Estimated footprint" subtitle="From GGUF metadata; the measured value appears after loading." icon={<MemoryStick size={14} />}>
            {model && estimate ? (
              <div className="space-y-2">
                <KV label="Weights on disk" value={bytes(estimate.weights)} mono />
                <KV label="KV cache estimate" value={estimate.kv === null ? "metadata missing" : bytes(estimate.kv)} mono />
                <KV label="Runtime overhead" value={bytes(estimate.overhead)} mono />
                <KV label="Estimated total" value={bytes(estimate.total)} mono />
                <div className="mt-2 text-[11px] leading-relaxed text-muted">
                  Formula: weights + KV (context × per-token bytes × cache factor) + a 256 MiB runtime allowance. This is
                  an estimate; load the model to get the real measured load time and RSS.
                </div>
                {outcome && (
                  <div className="mt-2 rounded-lg border border-ok/25 bg-ok/[0.05] px-3 py-2">
                    <div className="text-[11px] font-medium text-ok">Measured after load</div>
                    <div className="mt-1 space-y-1 text-[11px] text-ink-dim">
                      <div>Load time: {duration(outcome.status.load_ms)}</div>
                      <div>KV cache: {outcome.estimate.kv_cache_bytes ? bytes(outcome.estimate.kv_cache_bytes) : "not reported"}</div>
                      <div>Estimate used at spawn: {bytes(outcome.estimate.total_bytes)}</div>
                    </div>
                  </div>
                )}
                {outcome?.warnings.length ? (
                  <div className="space-y-1">
                    {outcome.warnings.map((warning) => (
                      <div key={warning} className="text-[11px] text-warn">
                        {warning}
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>
            ) : (
              <EmptyState compact title="Select a model" description="The estimate uses its parsed GGUF metadata." />
            )}
          </Panel>

          <Panel
            title="Applied at spawn"
            subtitle={runtime?.loaded ? `started ${relativeTime(runtime.started_at)}` : "No process running"}
            icon={<Gauge size={14} />}
            actions={
              <Button size="sm" onClick={() => void reloadRuntime()} icon={<RefreshCw size={12} />}>
                Refresh
              </Button>
            }
          >
            {runtime?.loaded && runtime.options ? (
              <div>
                <KV label="Context" value={runtime.options.context_length.toLocaleString()} mono />
                <KV label="Threads" value={runtime.options.threads} mono />
                <KV label="Batch" value={runtime.options.batch_size} mono />
                <KV label="KV cache" value={`${runtime.options.kv_cache_type_k}/${runtime.options.kv_cache_type_v}`} mono />
                <KV label="GPU layers" value={runtime.options.gpu_layers} mono />
                <KV label="mmap / mlock" value={`${runtime.options.use_mmap ? "on" : "off"} / ${runtime.options.use_mlock ? "on" : "off"}`} />
                <KV label="Flash attention" value={runtime.options.flash_attention ? "on" : "off"} />
                <div className="mt-3">
                  <div className="mb-1 flex items-center justify-between text-[11px] text-muted">
                    <span>Process health</span>
                    <Badge tone={runtime.alive ? "ok" : "warn"}>{runtime.alive ? "serving" : "exited"}</Badge>
                  </div>
                  <Progress value={runtime.rss_bytes && runtime.rss_bytes > 0 ? Math.min(1, runtime.rss_bytes / (4 * 1024 * 1024 * 1024)) : 0} />
                  <div className="mt-1 text-[10.5px] text-muted">
                    RSS {bytes(runtime.rss_bytes ?? 0)} · baseline scale 4 GiB
                  </div>
                </div>
              </div>
            ) : (
              <EmptyState compact title="Nothing loaded" description="Load a model to compare measured footprint." />
            )}
          </Panel>

          <Panel title="Measured load history" subtitle="This session, stored locally." icon={<Zap size={14} />}>
            {history.length === 0 ? (
              <EmptyState compact title="No loads yet" description="Each successful load records its measured start-up time." />
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-[11.5px]">
                  <thead>
                    <tr className="border-b border-line">
                      <th className="table-head px-1.5 py-1.5 text-left">Model</th>
                      <th className="table-head px-1.5 py-1.5 text-right">Load</th>
                      <th className="table-head px-1.5 py-1.5 text-right">ctx</th>
                      <th className="table-head px-1.5 py-1.5 text-right">batch</th>
                      <th className="table-head px-1.5 py-1.5 text-right">kv</th>
                      <th className="table-head px-1.5 py-1.5 text-right">gpu</th>
                      <th className="table-head px-1.5 py-1.5 text-right">when</th>
                    </tr>
                  </thead>
                  <tbody>
                    {history.map((row, index) => (
                      <tr key={`${row.at}-${index}`} className="border-b border-line/60">
                        <td className="max-w-[140px] truncate px-1.5 py-1.5 text-ink-dim">{row.model}</td>
                        <td className="mono-num px-1.5 py-1.5 text-right">{duration(row.load_ms)}</td>
                        <td className="mono-num px-1.5 py-1.5 text-right">{row.ctx}</td>
                        <td className="mono-num px-1.5 py-1.5 text-right">{row.batch}</td>
                        <td className="mono-num px-1.5 py-1.5 text-right">{row.kv}</td>
                        <td className="mono-num px-1.5 py-1.5 text-right">{row.gpu}</td>
                        <td className="px-1.5 py-1.5 text-right text-muted">{relativeTime(row.at)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Panel>
        </div>
      </div>

      <div className="mt-4 rounded-xl border border-line bg-white/[0.015] px-4 py-3 text-[11.5px] leading-relaxed text-muted">
        <span className="font-medium text-ink-dim">Reversible by design.</span> These settings only affect how
        llama-server loads the model. Unloading reaps the process and returns its memory. To make a permanent, smaller
        file, use the Compression Studio — it writes a separate GGUF and never edits the source.
        <span className="ml-1 text-ink-dim">
          {percent((runtime?.options?.context_length ?? 0) / 100)} of context preset applied.
        </span>
        <Button size="sm" className="ml-2" onClick={() => navigate("/compression")}>
          Compression Studio
        </Button>
      </div>
    </div>
  );
}
