import { useEffect, useRef } from "react";
import { Ban, FileText, Terminal } from "lucide-react";
import { api } from "../lib/api";
import { useJobStream } from "../lib/hooks";
import { cx, Badge, Button, Progress, toast } from "./ui";
import type { JobStatus, JobView } from "../lib/types";

export function statusTone(status: JobStatus): "neutral" | "ok" | "warn" | "bad" | "accent" {
  switch (status) {
    case "succeeded":
      return "ok";
    case "failed":
      return "bad";
    case "cancelled":
      return "warn";
    case "running":
      return "accent";
    default:
      return "neutral";
  }
}

export function JobStatusBadge({ status }: { status: JobStatus }) {
  return (
    <Badge tone={statusTone(status)}>
      <span
        className={cx(
          "h-1.5 w-1.5 rounded-full",
          status === "running" && "animate-pulse bg-accent",
          status === "succeeded" && "bg-ok",
          status === "failed" && "bg-bad",
          status === "cancelled" && "bg-warn",
          status === "queued" && "bg-muted",
        )}
      />
      {status}
    </Badge>
  );
}

export function JobConsole({
  jobId,
  onFinished,
  compact,
  autoScroll = true,
}: {
  jobId: string;
  onFinished?: (job: JobView) => void;
  compact?: boolean;
  autoScroll?: boolean;
}) {
  const { job, logs, terminal } = useJobStream(jobId, onFinished);
  const logRef = useRef<HTMLDivElement>(null);
  const followRef = useRef(true);

  useEffect(() => {
    if (!autoScroll || !followRef.current || !logRef.current) return;
    logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [logs, autoScroll]);

  const cancel = async () => {
    try {
      await api(`/jobs/${jobId}/cancel`, { method: "POST" });
      toast("Cancellation requested. Partial output will be discarded.", "info");
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Terminal size={14} className="text-muted" />
          <span className="truncate text-[12.5px] text-ink-dim">{job?.title ?? "Starting job…"}</span>
          {job && <JobStatusBadge status={job.status} />}
        </div>
        <div className="flex items-center gap-2">
          {job && (
            <span className="mono-num text-[11.5px] text-muted">
              {Math.round(job.progress * 100)}%
            </span>
          )}
          {!terminal && jobId && (
            <Button size="sm" variant="danger" icon={<Ban size={12} />} onClick={cancel}>
              Cancel
            </Button>
          )}
        </div>
      </div>

      <Progress
        value={job?.progress ?? 0}
        tone={
          job?.status === "failed"
            ? "bad"
            : job?.status === "cancelled"
              ? "warn"
              : job?.status === "succeeded"
                ? "ok"
                : "accent"
        }
      />
      <div className="flex items-center justify-between gap-3 text-[11.5px] text-muted">
        <span className="truncate">{job?.stage ?? "queued"}</span>
        <span className="mono-num shrink-0">{logs.length} log lines</span>
      </div>

      <div
        ref={logRef}
        onScroll={(e) => {
          const el = e.currentTarget;
          followRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
        className={cx(
          "overflow-y-auto rounded-lg border border-line bg-black/50 px-3 py-2 font-mono text-[11.5px] leading-relaxed",
          compact ? "h-32" : "h-52",
        )}
      >
        {logs.length === 0 && <div className="text-muted">Waiting for the tool to report…</div>}
        {logs.map((line) => (
          <div
            key={line.seq}
            className={cx(
              "whitespace-pre-wrap break-words",
              line.stream === "stderr" ? "text-[#f0b7b7]" : "text-[#b9c3d3]",
            )}
          >
            <span className="mr-2 text-[#4d5566]">{line.ts.slice(11, 19)}</span>
            {line.message}
          </div>
        ))}
      </div>

      {job?.error && (
        <div className="rounded-lg border border-bad/25 bg-bad/[0.06] px-3 py-2 text-[12px] leading-relaxed text-[#ffc0c0]">
          {job.error}
        </div>
      )}
      {job?.status === "succeeded" && (
        <div className="flex items-center gap-2 text-[12px] text-ok">
          <FileText size={13} /> Completed. Output validated and recorded.
        </div>
      )}
    </div>
  );
}
