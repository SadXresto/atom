import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Download, FileJson, FlaskConical, Trash2, X } from "lucide-react";
import { api, downloadText, fetchText } from "../lib/api";
import { dateTime, relativeTime, signedPercent } from "../lib/format";
import { useApi } from "../lib/hooks";
import type { Comparison, ComparisonSummary, JobView } from "../lib/types";
import { ComparisonView } from "../components/ComparisonView";
import { JobStatusBadge } from "../components/JobConsole";
import {
  Badge,
  Button,
  ConfirmDialog,
  EmptyState,
  PageHeader,
  Panel,
  cx,
  toast,
} from "../components/ui";

export default function History() {
  const navigate = useNavigate();
  const { data, reload } = useApi<{ comparisons: ComparisonSummary[] }>("/comparisons", 8000);
  const { data: jobsData } = useApi<{ jobs: JobView[] }>("/jobs", 8000);
  const [open, setOpen] = useState<Comparison | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ComparisonSummary | null>(null);

  const comparisons = data?.comparisons ?? [];
  const jobs = jobsData?.jobs ?? [];

  const openRun = async (id: string) => {
    try {
      const result = await api<{ comparison: Comparison }>(`/comparisons/${id}`);
      setOpen(result.comparison);
      window.scrollTo({ top: 0, behavior: "smooth" });
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const exportRun = async (id: string) => {
    try {
      const markdown = await fetchText(`/comparisons/${id}/report`);
      downloadText(`atom-comparison-${id.slice(0, 8)}.md`, markdown);
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const exportJson = async (id: string) => {
    try {
      const result = await api<{ comparison: Comparison }>(`/comparisons/${id}`);
      downloadText(`atom-comparison-${id.slice(0, 8)}.json`, JSON.stringify(result.comparison, null, 2), "application/json");
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const doDelete = async () => {
    if (!deleteTarget) return;
    try {
      await api(`/comparisons/${deleteTarget.id}`, { method: "DELETE" });
      toast("Comparison deleted", "ok");
      if (open?.id === deleteTarget.id) setOpen(null);
      setDeleteTarget(null);
      void reload();
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="Benchmark History"
        title="Saved runs and reports"
        description="Every comparison is stored with its prompts, settings, environment and measured results, so a result can be reproduced or audited later."
        actions={
          <Button variant="primary" icon={<FlaskConical size={13} />} onClick={() => navigate("/comparison")}>
            New comparison
          </Button>
        }
      />

      {open && (
        <div className="mb-4 space-y-2">
          <div className="flex items-center justify-between rounded-xl border border-line bg-white/[0.02] px-3 py-2">
            <span className="text-[12px] text-ink-dim">Viewing saved run from {dateTime(open.created_at)}</span>
            <Button size="sm" onClick={() => setOpen(null)} icon={<X size={12} />}>
              Close
            </Button>
          </div>
          <ComparisonView comparison={open} />
        </div>
      )}

      <Panel title="Comparisons" icon={<FlaskConical size={14} />} actions={<Badge>{comparisons.length}</Badge>}>
        {comparisons.length === 0 ? (
          <EmptyState
            icon={<FlaskConical size={17} />}
            title="No benchmark runs yet"
            description="Runs appear here after the Comparison Lab finishes. ATOM stores the environment and settings with every result."
            action={
              <Button variant="primary" size="sm" onClick={() => navigate("/comparison")}>
                Open Comparison Lab
              </Button>
            }
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-[12px]">
              <thead>
                <tr className="border-b border-line">
                  <th className="table-head px-2 py-2 text-left">Run</th>
                  <th className="table-head px-2 py-2 text-right">Disk</th>
                  <th className="table-head px-2 py-2 text-right">Throughput</th>
                  <th className="table-head px-2 py-2 text-right">Quality</th>
                  <th className="table-head px-2 py-2 text-right">When</th>
                  <th className="table-head px-2 py-2 text-right">Actions</th>
                </tr>
              </thead>
              <tbody>
                {comparisons.map((item) => (
                  <tr
                    key={item.id}
                    className={cx("border-b border-line/60 last:border-0 hover:bg-white/[0.015]", open?.id === item.id && "bg-accent/[0.04]")}
                  >
                    <td className="px-2 py-2">
                      <button onClick={() => void openRun(item.id)} className="text-left">
                        <div className="text-[12.5px] text-ink-dim hover:text-ink">{item.title}</div>
                        <div className="text-[11px] text-muted">
                          {item.models.map((m) => `${m} (${m === item.models[0] ? item.baseline_quant : item.candidate_quant})`).join(" → ")}
                        </div>
                      </button>
                    </td>
                    <td className="mono-num px-2 py-2 text-right text-ink-dim">
                      {item.size_saved_percent === null ? "—" : `−${item.size_saved_percent.toFixed(1)}%`}
                    </td>
                    <td className="mono-num px-2 py-2 text-right">
                      <span
                        className={cx(
                          item.speed_change_percent === null
                            ? "text-muted"
                            : item.speed_change_percent >= 0
                              ? "text-ok"
                              : "text-warn",
                        )}
                      >
                        {signedPercent(item.speed_change_percent)}
                      </span>
                    </td>
                    <td className="mono-num px-2 py-2 text-right text-ink-dim">
                      {item.quality_delta === null ? "—" : `${item.quality_delta >= 0 ? "+" : ""}${item.quality_delta.toFixed(1)}`}
                    </td>
                    <td className="px-2 py-2 text-right text-[11px] text-muted">{relativeTime(item.created_at)}</td>
                    <td className="px-2 py-2">
                      <div className="flex items-center justify-end gap-1">
                        <Button size="sm" onClick={() => void exportRun(item.id)} icon={<Download size={11} />}>
                          MD
                        </Button>
                        <Button size="sm" onClick={() => void exportJson(item.id)} icon={<FileJson size={11} />}>
                          JSON
                        </Button>
                        <button
                          onClick={() => setDeleteTarget(item)}
                          className="rounded p-1 text-muted hover:text-bad"
                          aria-label="Delete comparison"
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>

      <Panel className="mt-4" title="All jobs" icon={<FlaskConical size={14} />} subtitle="Imports, verifications and quantization runs.">
        {jobs.length === 0 ? (
          <EmptyState compact title="No jobs recorded in this session" />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-[12px]">
              <thead>
                <tr className="border-b border-line">
                  <th className="table-head px-2 py-2 text-left">Job</th>
                  <th className="table-head px-2 py-2 text-left">Kind</th>
                  <th className="table-head px-2 py-2 text-left">Status</th>
                  <th className="table-head px-2 py-2 text-right">Progress</th>
                  <th className="table-head px-2 py-2 text-right">Created</th>
                </tr>
              </thead>
              <tbody>
                {jobs.map((job) => (
                  <tr key={job.id} className="border-b border-line/60 last:border-0">
                    <td className="max-w-[320px] truncate px-2 py-1.5 text-ink-dim">{job.title}</td>
                    <td className="px-2 py-1.5 text-muted">{job.kind}</td>
                    <td className="px-2 py-1.5">
                      <JobStatusBadge status={job.status} />
                    </td>
                    <td className="mono-num px-2 py-1.5 text-right text-muted">{Math.round(job.progress * 100)}%</td>
                    <td className="px-2 py-1.5 text-right text-[11px] text-muted">{relativeTime(job.created_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>

      <ConfirmDialog
        open={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={doDelete}
        danger
        title="Delete comparison"
        confirmLabel="Delete"
        body={
          <p>
            The saved result for <span className="text-ink">{deleteTarget?.title}</span> and its Markdown report will be
            removed. Model files are not affected.
          </p>
        }
      />
    </div>
  );
}
