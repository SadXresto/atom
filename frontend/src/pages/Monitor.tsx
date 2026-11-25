import { useNavigate } from "react-router-dom";
import {
  Activity,
  Cpu,
  Database,
  HardDrive,
  ListTree,
  MemoryStick,
  Radio,
  RefreshCw,
  Terminal,
} from "lucide-react";
import { useLiveSystem } from "../lib/hooks";
import { useApi } from "../lib/hooks";
import { bytes, clockTime, percent, relativeTime } from "../lib/format";
import type { BackendStatus, JobView, RuntimeStatus } from "../lib/types";
import { CpuChart, MemoryChart } from "../components/Charts";
import { JobStatusBadge } from "../components/JobConsole";
import {
  Badge,
  Button,
  EmptyState,
  KV,
  PageHeader,
  Panel,
  Progress,
  Stat,
  cx,
} from "../components/ui";

interface LogLine {
  ts: string;
  level: string;
  target: string;
  message: string;
}

export default function Monitor() {
  const navigate = useNavigate();
  const { history, latest, connected } = useLiveSystem();
  const { data: runtimeData } = useApi<{ runtime: RuntimeStatus; tuning: string[] }>("/runtime", 4000);
  const { data: jobsData } = useApi<{ jobs: JobView[] }>("/jobs", 4000);
  const { data: logData } = useApi<{ lines: LogLine[] }>("/logs?limit=120", 5000);
  const { data: backendData } = useApi<{ backend: BackendStatus }>("/backend", 20000);

  const runtime = runtimeData?.runtime;
  const jobs = jobsData?.jobs ?? [];
  const usedRatio = latest ? latest.used_memory / Math.max(1, latest.total_memory) : 0;

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="System Monitor"
        title="Real measurements, clearly separated"
        description="System RAM, the ATOM process, and the llama-server model process are three different numbers. ATOM shows each one from the OS rather than merging them into a single figure."
        actions={
          <div className="flex items-center gap-2">
            <Badge tone={connected ? "ok" : "warn"}>
              <Radio size={10} className={connected ? "text-ok" : "text-warn"} />
              {connected ? "streaming" : "reconnecting"}
            </Badge>
            <Button size="sm" icon={<RefreshCw size={12} />} onClick={() => navigate("/")}>
              Overview
            </Button>
          </div>
        }
      />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat
          label="System RAM used"
          value={latest ? percent(usedRatio * 100, 1) : "—"}
          sub={latest ? `${bytes(latest.used_memory)} / ${bytes(latest.total_memory)}` : "waiting"}
          tone={usedRatio > 0.9 ? "bad" : usedRatio > 0.75 ? "warn" : "default"}
          icon={<MemoryStick size={13} />}
        />
        <Stat
          label="ATOM process"
          value={latest ? bytes(latest.process_rss) : "—"}
          sub={latest ? `cpu ${latest.process_cpu_percent.toFixed(1)}% · pid ${runtime ? "api" : "—"}` : undefined}
          icon={<Cpu size={13} />}
        />
        <Stat
          label="llama-server RSS"
          value={latest?.model_process_rss ? bytes(latest.model_process_rss) : "—"}
          sub={
            latest?.model_process_pid
              ? `pid ${latest.model_process_pid}`
              : runtime?.loaded
                ? "process not sampled yet"
                : "no model loaded"
          }
          tone={latest?.model_process_rss ? "accent" : "default"}
          icon={<Database size={13} />}
        />
        <Stat
          label="System CPU"
          value={latest ? `${latest.system_cpu_percent.toFixed(1)}%` : "—"}
          sub={latest ? `load ${latest.load_average.map((v) => v.toFixed(2)).join(" · ")}` : undefined}
          icon={<Activity size={13} />}
        />
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-[1.5fr_1fr]">
        <Panel
          title="Memory over time"
          subtitle="Sampled every 1.5 s from sysinfo. The model line is llama-server's resident set."
          icon={<MemoryStick size={14} />}
        >
          {history.length ? (
            <MemoryChart history={history} />
          ) : (
            <EmptyState compact icon={<MemoryStick size={16} />} title="Collecting samples…" />
          )}
          <div className="mt-2 grid gap-2 sm:grid-cols-3">
            <MemoryNote label="System RAM" value={latest ? bytes(latest.total_memory) : "—"} hint="Physical memory installed." />
            <MemoryNote
              label="Available"
              value={latest ? bytes(latest.available_memory) : "—"}
              hint="What the OS says can still be allocated without swapping."
            />
            <MemoryNote
              label="Model file"
              value={runtime?.model_path ? bytes(0) : "—"}
              hint="File size on disk is not memory; mmap pages count as page cache."
              hidden
            />
          </div>
        </Panel>

        <Panel title="CPU utilization" subtitle="Whole-system CPU, all cores." icon={<Cpu size={14} />}>
          {history.length ? <CpuChart history={history} /> : <EmptyState compact title="Collecting samples…" />}
          <div className="mt-2">
            <KV label="Samples buffered" value={history.length} mono />
            <KV label="Last sample" value={latest ? clockTime(latest.ts) : "—"} />
            <KV label="Backend" value={backendData?.backend.inference_ready ? "llama-server ready" : "not available"} />
          </div>
        </Panel>
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <Panel
          title="Active runtime"
          icon={<Database size={14} />}
          actions={<Badge tone={runtime?.loaded ? "ok" : "neutral"}>{runtime?.loaded ? "loaded" : "idle"}</Badge>}
        >
          {runtime?.loaded ? (
            <div className="space-y-0.5">
              <KV label="Model" value={runtime.model_label ?? "—"} />
              <KV label="Process" value={`pid ${runtime.pid}`} mono />
              <KV label="Resident now" value={runtime.rss_bytes ? bytes(runtime.rss_bytes) : "sampling…"} mono />
              <KV label="Context" value={runtime.options?.context_length.toLocaleString() ?? "—"} mono />
              <KV label="KV cache" value={`${runtime.options?.kv_cache_type_k}/${runtime.options?.kv_cache_type_v}`} mono />
              <KV label="Loaded" value={relativeTime(runtime.started_at)} />
              <div className="mt-2 rounded-lg border border-line bg-black/20 px-3 py-2 text-[11px] leading-relaxed text-muted">
                {runtimeData?.tuning.join(" · ")}
              </div>
            </div>
          ) : (
            <EmptyState compact icon={<Database size={16} />} title="No model loaded" description="Load one from the Inference workspace." />
          )}
        </Panel>

        <Panel title="Jobs" icon={<ListTree size={14} />} actions={<Badge>{jobs.filter((j) => j.status === "running").length} active</Badge>}>
          {jobs.length === 0 ? (
            <EmptyState compact title="No jobs have run" />
          ) : (
            <div className="max-h-[220px] space-y-1.5 overflow-y-auto pr-1">
              {jobs.slice(0, 10).map((job) => (
                <div key={job.id} className="rounded-lg border border-line bg-white/[0.015] px-2.5 py-2">
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate text-[11.5px] text-ink-dim">{job.title}</span>
                    <JobStatusBadge status={job.status} />
                  </div>
                  <div className="mt-1.5 flex items-center gap-2">
                    <Progress value={job.progress} height={3} tone={job.status === "failed" ? "bad" : job.status === "succeeded" ? "ok" : "accent"} />
                    <span className="mono-num w-9 shrink-0 text-right text-[10px] text-muted">{Math.round(job.progress * 100)}%</span>
                  </div>
                  <div className="mt-1 truncate text-[10.5px] text-muted">{job.stage}</div>
                </div>
              ))}
            </div>
          )}
        </Panel>
      </div>

      <Panel
        className="mt-4"
        title="Runtime log"
        subtitle="Engine, toolchain and llama.cpp messages. Model-provided text is never executed."
        icon={<Terminal size={14} />}
        actions={<Badge>{logData?.lines.length ?? 0} lines</Badge>}
      >
        <div className="max-h-[260px] overflow-y-auto rounded-lg border border-line bg-black/50 p-3 font-mono text-[11px] leading-relaxed">
          {!logData?.lines.length && <div className="text-muted">No log lines yet.</div>}
          {logData?.lines
            .slice()
            .reverse()
            .map((line, index) => (
              <div key={index} className="flex gap-2">
                <span className="shrink-0 text-[#4d5566]">{clockTime(line.ts)}</span>
                <span
                  className={cx(
                    "w-10 shrink-0 uppercase",
                    line.level === "error" ? "text-bad" : line.level === "warn" ? "text-warn" : "text-[#5f6980]",
                  )}
                >
                  {line.level}
                </span>
                <span className="min-w-0 break-words text-[#b9c3d3]">
                  <span className="text-[#6d7688]">[{line.target}] </span>
                  {line.message}
                </span>
              </div>
            ))}
        </div>
      </Panel>

      <div className="mt-4 grid gap-3 sm:grid-cols-3">
        <Clarify
          icon={<HardDrive size={14} />}
          title="Model file size"
          body="Bytes on disk. With mmap, those pages are shared page cache and may be evicted at any time."
        />
        <Clarify
          icon={<Database size={14} />}
          title="Process RSS"
          body="Resident pages of one process right now. It includes shared file-backed pages, so it can exceed a private-memory figure."
        />
        <Clarify
          icon={<MemoryStick size={14} />}
          title="System RAM"
          body="Everything the OS is holding across all processes plus cache. Always greater than the sum of ATOM's processes."
        />
      </div>
    </div>
  );
}

function MemoryNote({ label, value, hint, hidden }: { label: string; value: string; hint: string; hidden?: boolean }) {
  if (hidden) return null;
  return (
    <div className="rounded-lg border border-line bg-black/20 px-2.5 py-2" title={hint}>
      <div className="label">{label}</div>
      <div className="mono-num mt-0.5 text-[13px] text-ink">{value}</div>
      <div className="mt-0.5 text-[10px] leading-snug text-muted">{hint}</div>
    </div>
  );
}

function Clarify({ icon, title, body }: { icon: React.ReactNode; title: string; body: string }) {
  return (
    <div className="panel-flat flex items-start gap-2.5 px-3.5 py-3">
      <span className="mt-0.5 text-muted">{icon}</span>
      <div>
        <div className="text-[12px] font-medium text-ink-dim">{title}</div>
        <div className="mt-0.5 text-[11px] leading-relaxed text-muted">{body}</div>
      </div>
    </div>
  );
}
