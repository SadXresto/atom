import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowRight, BarChart3, FlaskConical, Layers, Zap } from "lucide-react";
import { useApi } from "../lib/hooks";
import { bytes, dateTime, percent, signedPercent } from "../lib/format";
import type { Comparison, ComparisonSummary, ModelRun } from "../lib/types";
import { ComparisonView } from "../components/ComparisonView";
import {
  Badge,
  Button,
  EmptyState,
  PageHeader,
  Panel,
  Stat,
  cx,
} from "../components/ui";

export default function BenchmarkCenter() {
  const navigate = useNavigate();
  const { data } = useApi<{ comparisons: ComparisonSummary[]; suites: Array<{ id: string; title: string; description: string; kind: string }> }>(
    "/comparisons",
    10000,
  );
  const { data: bench } = useApi<{ suites: unknown[]; cases: Array<{ id: string; suite: string; title: string }> }>("/bench/suites");
  const [open, setOpen] = useState<Comparison | null>(null);
  const [loading, setLoading] = useState(false);

  const comparisons = data?.comparisons ?? [];
  const suites = data?.suites ?? [];

  const openRun = async (id: string) => {
    setLoading(true);
    try {
      const result = await fetch(`/api/comparisons/${id}`).then((r) => r.json());
      setOpen(result.comparison as Comparison);
      window.scrollTo({ top: 0, behavior: "smooth" });
    } finally {
      setLoading(false);
    }
  };

  const totals = comparisons.reduce(
    (acc, run) => ({
      size: acc.size + (run.size_saved_percent ?? 0),
      speed: acc.speed + (run.speed_change_percent ?? 0),
      quality: acc.quality + (run.quality_delta ?? 0),
    }),
    { size: 0, speed: 0, quality: 0 },
  );
  const count = comparisons.length || 1;

  const best = (runs: ModelRun[]) => [...runs].sort((a, b) => b.avg_tokens_per_second - a.avg_tokens_per_second)[0];

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="Measurement"
        title="Benchmark Center"
        description="Every saved comparison, aggregated by measured speed, disk size and rule-checked quality. Nothing here is modelled or extrapolated."
        actions={
          <>
            <Button onClick={() => navigate("/history")} icon={<Layers size={13} />}>
              Run history
            </Button>
            <Button variant="primary" onClick={() => navigate("/comparison")} icon={<FlaskConical size={13} />}>
              New comparison
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat label="Saved runs" value={comparisons.length} icon={<BarChart3 size={13} />} />
        <Stat label="Avg disk change" value={signedPercent(totals.size / count)} tone={totals.size < 0 ? "ok" : "default"} />
        <Stat label="Avg speed change" value={signedPercent(totals.speed / count)} tone={totals.speed > 0 ? "ok" : "warn"} />
        <Stat
          label="Avg quality delta"
          value={signedPercent(totals.quality / count, 2)}
          sub="automated estimate"
          tone={totals.quality >= 0 ? "ok" : "warn"}
        />
      </div>

      {open && (
        <div className="mt-4 space-y-2">
          <div className="flex items-center justify-between rounded-xl border border-line bg-white/[0.02] px-3 py-2">
            <span className="text-[12px] text-ink-dim">
              {open.title} · {dateTime(open.created_at)} · {open.environment.os}/{open.environment.arch}
            </span>
            <Button size="sm" onClick={() => setOpen(null)}>
              Close
            </Button>
          </div>
          <ComparisonView comparison={open} />
        </div>
      )}

      <div className="mt-4 grid gap-4 lg:grid-cols-[1.4fr_1fr]">
        <Panel title="Comparisons" subtitle="Click a row to inspect cases, suite pass rates and environment." icon={<FlaskConical size={14} />}>
          {comparisons.length === 0 ? (
            <EmptyState
              icon={<BarChart3 size={17} />}
              title="No benchmarks yet"
              description="The Comparison Lab loads each model with identical settings and records measured throughput, latency, memory and rule-checked outputs."
              action={
                <Button size="sm" variant="primary" onClick={() => navigate("/comparison")}>
                  Run the first comparison
                </Button>
              }
            />
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-[12px]">
                <thead>
                  <tr className="border-b border-line">
                    <th className="table-head px-2 py-2 text-left">Run</th>
                    <th className="table-head px-2 py-2 text-right">Models</th>
                    <th className="table-head px-2 py-2 text-right">Disk</th>
                    <th className="table-head px-2 py-2 text-right">Speed</th>
                    <th className="table-head px-2 py-2 text-right">Quality</th>
                    <th className="table-head px-2 py-2 text-right">When</th>
                  </tr>
                </thead>
                <tbody>
                  {comparisons.map((run) => (
                    <tr
                      key={run.id}
                      onClick={() => (loading ? undefined : void openRun(run.id))}
                      className="cursor-pointer border-b border-line/60 hover:bg-white/[0.02]"
                    >
                      <td className="px-2 py-2">
                        <div className="text-ink-dim">{run.title}</div>
                        <div className="text-[10.5px] text-muted">{run.models.join(" · ")}</div>
                      </td>
                      <td className="mono-num px-2 py-2 text-right text-muted">{run.models.length}</td>
                      <td className={cx("mono-num px-2 py-2 text-right", (run.size_saved_percent ?? 0) > 0 ? "text-ok" : "text-muted")}>
                        {signedPercent(run.size_saved_percent)}
                      </td>
                      <td className={cx("mono-num px-2 py-2 text-right", (run.speed_change_percent ?? 0) > 0 ? "text-ok" : "text-warn")}>
                        {signedPercent(run.speed_change_percent)}
                      </td>
                      <td className="mono-num px-2 py-2 text-right text-muted">
                        {run.quality_delta === null ? "—" : signedPercent(run.quality_delta, 2)}
                      </td>
                      <td className="px-2 py-2 text-right text-[11px] text-muted">{dateTime(run.created_at)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Panel>

        <div className="space-y-4">
          <Panel title="Suites" subtitle="Deterministic, rule-checked cases." icon={<Layers size={14} />}>
            <div className="space-y-2">
              {suites.map((suite) => (
                <div key={suite.id} className="rounded-lg border border-line bg-black/20 px-3 py-2">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[12px] text-ink-dim">{suite.title}</span>
                    <Badge tone={suite.kind === "quality" ? "accent" : "neutral"}>{suite.kind}</Badge>
                  </div>
                  <p className="mt-0.5 text-[11px] leading-relaxed text-muted">{suite.description}</p>
                </div>
              ))}
              {suites.length === 0 && <EmptyState compact title="No suites available" />}
            </div>
            <div className="mt-2 text-[10.5px] text-muted">
              {bench?.cases.length ?? 0} fixed cases. Pass rates are automated checks, not intelligence scores.
            </div>
          </Panel>

          <Panel title="Latest run detail" icon={<Zap size={14} />}>
            {open ? (
              <div className="space-y-2">
                {open.runs.map((run) => {
                  const fastest = best(open.runs);
                  return (
                    <div key={run.model_id} className="rounded-lg border border-line bg-black/20 px-3 py-2">
                      <div className="flex items-center justify-between gap-2">
                        <span className="truncate text-[12px] text-ink-dim">{run.label}</span>
                        {fastest?.model_id === run.model_id && <Badge tone="ok">fastest</Badge>}
                      </div>
                      <div className="mt-1 grid grid-cols-2 gap-x-3 gap-y-0.5 text-[10.5px] text-muted">
                        <span className="mono-num">{run.avg_tokens_per_second.toFixed(1)} tok/s</span>
                        <span className="mono-num">{bytes(run.file_size)}</span>
                        <span className="mono-num">TTFT {run.avg_ttft_ms.toFixed(0)} ms</span>
                        <span className="mono-num">quality {percent(run.quality_index * 100, 1)}</span>
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              <EmptyState
                compact
                icon={<BarChart3 size={16} />}
                title="Select a comparison"
                description="Open a run on the left to see per-model measurements here."
              />
            )}
          </Panel>

          <Panel title="Micro benchmarks" icon={<ArrowRight size={14} />}>
            <p className="text-[11.5px] leading-relaxed text-muted">
              Device measurements (tokens/s, latency, RAM) live with each Micro project and are imported from the
              firmware's own report. Open the Deployment Center to collect them.
            </p>
            <Button size="sm" className="mt-2" onClick={() => navigate("/deployment")}>
              Open Deployment Center
            </Button>
          </Panel>
        </div>
      </div>
    </div>
  );
}
