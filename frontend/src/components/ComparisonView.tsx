import { useState } from "react";
import {
  Activity,
  AlertTriangle,
  Clock,
  Download,
  Gauge,
  HardDrive,
  Layers,
  MemoryStick,
  Percent,
  Target,
  Timer,
} from "lucide-react";
import { downloadText, fetchText } from "../lib/api";
import { bytes, dateTime, duration, percent, signedPercent } from "../lib/format";
import type { Comparison, ModelRun } from "../lib/types";
import { SuiteBars } from "./Charts";
import { Badge, Banner, Button, CopyButton, DisclosureRow, EmptyState, KV, Panel, Stat, cx, toast } from "./ui";

export function ComparisonView({ comparison }: { comparison: Comparison }) {
  const [exporting, setExporting] = useState(false);
  const baseline = comparison.runs[0];
  const candidate = comparison.runs[1];

  const sizeSaved =
    baseline && candidate && baseline.file_size > 0
      ? ((baseline.file_size - candidate.file_size) / baseline.file_size) * 100
      : null;
  const rssChange =
    baseline && candidate && baseline.peak_rss_bytes > 0
      ? ((candidate.peak_rss_bytes - baseline.peak_rss_bytes) / baseline.peak_rss_bytes) * 100
      : null;
  const speedChange =
    baseline && candidate && baseline.avg_tokens_per_second > 0
      ? ((candidate.avg_tokens_per_second - baseline.avg_tokens_per_second) /
          baseline.avg_tokens_per_second) *
        100
      : null;
  const qualityDelta =
    baseline && candidate ? candidate.quality_index - baseline.quality_index : null;
  const loadChange =
    baseline && candidate && baseline.load_ms > 0
      ? ((candidate.load_ms - baseline.load_ms) / baseline.load_ms) * 100
      : null;

  const exportReport = async () => {
    setExporting(true);
    try {
      const markdown = await fetchText(`/comparisons/${comparison.id}/report`);
      downloadText(`atom-comparison-${comparison.id.slice(0, 8)}.md`, markdown);
      toast("Report exported as Markdown", "ok");
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setExporting(false);
    }
  };

  const suiteNames = collectSuites(comparison.runs);
  const suiteChart = suiteNames.map((suite) => ({
    title: suite.title,
    values: comparison.runs.map((run) => {
      const found = run.suites.find((s) => s.suite === suite.suite);
      return found ? found.pass_rate : null;
    }),
  }));

  const jsonReport = JSON.stringify(comparison, null, 2);

  return (
    <div className="space-y-4">
      <Panel
        title={comparison.title}
        subtitle={`Run ${dateTime(comparison.created_at)} · ${comparison.environment.os} ${comparison.environment.arch} · ${comparison.environment.cpu_count} CPUs · ${bytes(comparison.environment.total_memory)} RAM`}
        icon={<Target size={14} />}
        actions={
          <>
            <CopyButton text={jsonReport} label="JSON" />
            <Button size="sm" icon={<Download size={12} />} busy={exporting} onClick={exportReport}>
              Report
            </Button>
          </>
        }
      >
        <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
          <Stat
            label="Disk saved"
            value={sizeSaved === null ? "—" : `${sizeSaved.toFixed(1)}%`}
            sub={baseline && candidate ? `${bytes(baseline.file_size)} → ${bytes(candidate.file_size)}` : undefined}
            tone={sizeSaved && sizeSaved > 0 ? "ok" : "default"}
            icon={<HardDrive size={13} />}
          />
          <Stat
            label="Throughput"
            value={speedChange === null ? "—" : signedPercent(speedChange)}
            sub={baseline && candidate ? `${baseline.avg_tokens_per_second.toFixed(1)} → ${candidate.avg_tokens_per_second.toFixed(1)} tok/s` : undefined}
            tone={speedChange === null ? "default" : speedChange >= 0 ? "ok" : "warn"}
            icon={<Gauge size={13} />}
          />
          <Stat
            label="Peak RSS"
            value={rssChange === null ? "—" : signedPercent(rssChange)}
            sub={baseline && candidate ? `${bytes(baseline.peak_rss_bytes)} → ${bytes(candidate.peak_rss_bytes)}` : undefined}
            tone={rssChange === null ? "default" : rssChange <= 0 ? "ok" : "warn"}
            icon={<MemoryStick size={13} />}
          />
          <Stat
            label="Quality index"
            value={qualityDelta === null ? "—" : `${qualityDelta >= 0 ? "+" : ""}${qualityDelta.toFixed(1)} pts`}
            sub={baseline && candidate ? `${baseline.quality_index.toFixed(1)} → ${candidate.quality_index.toFixed(1)} / 100` : undefined}
            tone={qualityDelta === null ? "default" : qualityDelta >= 0 ? "ok" : "warn"}
            icon={<Activity size={13} />}
          />
        </div>

        <Banner tone="info" title="What these numbers are">
          Throughput, latency and RSS are measured. The quality index is the mean pass rate of rule-checked prompts on
          this suite — an automated estimate of behaviour, not proof the models are equivalent.
        </Banner>
      </Panel>

      <div className="grid gap-4 lg:grid-cols-[1.15fr_1fr]">
        <Panel title="Suite pass rates" subtitle="Per-suite automated check results." icon={<Percent size={14} />}>
          {suiteChart.length ? (
            <SuiteBars suites={suiteChart} labels={comparison.runs.map((r) => r.label)} />
          ) : (
            <EmptyState compact title="No quality suites were selected" />
          )}
        </Panel>

        <Panel title="Run configuration" subtitle="Identical prompts and comparable runtime settings." icon={<Layers size={14} />}>
          <div className="grid gap-x-6 sm:grid-cols-2">
            <div>
              <KV label="Context" value={`${comparison.settings.context_length.toLocaleString()} tokens`} mono />
              <KV label="Threads" value={comparison.settings.threads} mono />
              <KV label="Batch" value={comparison.settings.batch_size} mono />
              <KV label="GPU layers" value={comparison.settings.gpu_layers} mono />
            </div>
            <div>
              <KV label="Temperature" value={comparison.settings.temperature.toFixed(2)} mono />
              <KV label="Top-p" value={comparison.settings.top_p.toFixed(2)} mono />
              <KV label="Seed" value={comparison.settings.seed ?? "random"} mono />
              <KV label="Max tokens" value={comparison.settings.max_tokens} mono />
            </div>
          </div>
          <div className="mt-2 border-t border-line pt-2">
            <KV label="KV cache" value={`${comparison.settings.kv_cache_type_k} / ${comparison.settings.kv_cache_type_v}`} mono />
            <KV label="Mmap" value={comparison.settings.use_mmap ? "enabled" : "disabled"} />
            <KV label="Backend" value={comparison.environment.backend_version ?? "not reported"} mono />
            <KV label="ATOM" value={comparison.environment.atom_version} mono />
            {loadChange !== null && (
              <KV label="Load time change" value={`${signedPercent(loadChange)} (${duration(baseline?.load_ms)} → ${duration(candidate?.load_ms)})`} mono />
            )}
          </div>
        </Panel>
      </div>

      <Panel title="Model comparison" icon={<Layers size={14} />}>
        <div className="overflow-x-auto">
          <table className="w-full text-[12px]">
            <thead>
              <tr className="border-b border-line">
                <th className="table-head px-2 py-2 text-left">Metric</th>
                {comparison.runs.map((run) => (
                  <th key={run.model_id} className="table-head px-2 py-2 text-right">
                    {run.label}
                    <div className="mt-0.5 text-[10px] font-normal normal-case tracking-normal text-muted">
                      {run.kind} · {run.quant}
                    </div>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              <Row label="File size" values={comparison.runs.map((r) => bytes(r.file_size))} />
              <Row label="Load time" values={comparison.runs.map((r) => duration(r.load_ms))} />
              <Row label="Peak process RSS" values={comparison.runs.map((r) => bytes(r.peak_rss_bytes))} />
              <Row label="RSS after load" values={comparison.runs.map((r) => bytes(r.rss_after_load_bytes))} />
              <Row label="Avg tokens/s" values={comparison.runs.map((r) => r.avg_tokens_per_second.toFixed(2))} />
              <Row label="Avg time to first token" values={comparison.runs.map((r) => `${Math.round(r.avg_ttft_ms)} ms`)} />
              <Row label="Generation time (sum)" values={comparison.runs.map((r) => duration(r.total_generation_ms))} />
              <Row label="Quality index" values={comparison.runs.map((r) => `${r.quality_index.toFixed(1)} / 100`)} />
              <Row
                label="Consistency"
                values={comparison.runs.map((r) => (r.consistency ? percent(r.consistency.agreement * 100, 0) : "not measured"))}
              />
              <Row label="Cases passed" values={comparison.runs.map((r) => `${r.cases.filter((c) => c.passed).length} / ${r.cases.length}`)} />
            </tbody>
          </table>
        </div>
        <div className="mt-2 flex items-center gap-2 text-[11px] text-muted">
          <Clock size={11} /> Peak RSS is the maximum resident set of the llama-server process sampled every 300 ms.
        </div>
      </Panel>

      {comparison.runs.map((run) => (
        <RunDetail key={run.model_id} run={run} />
      ))}
    </div>
  );
}

function Row({ label, values }: { label: string; values: string[] }) {
  return (
    <tr className="border-b border-line/60 last:border-0">
      <td className="px-2 py-1.5 text-muted">{label}</td>
      {values.map((value, index) => (
        <td key={index} className="mono-num px-2 py-1.5 text-right text-ink-dim">
          {value}
        </td>
      ))}
    </tr>
  );
}

function RunDetail({ run }: { run: ModelRun }) {
  return (
    <Panel
      title={run.label}
      subtitle={`${run.kind} · ${run.quant} · ${bytes(run.file_size)}`}
      icon={<Timer size={14} />}
      actions={
        run.error ? (
          <Badge tone="bad">load failed</Badge>
        ) : (
          <Badge tone={run.quality_index >= 80 ? "ok" : run.quality_index >= 50 ? "warn" : "bad"}>
            quality {run.quality_index.toFixed(0)}
          </Badge>
        )
      }
    >
      {run.error ? (
        <Banner tone="bad" title="This model could not be measured">
          {run.error}
        </Banner>
      ) : (
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
            <MiniMetric label="tok/s" value={run.avg_tokens_per_second.toFixed(1)} />
            <MiniMetric label="TTFT" value={`${Math.round(run.avg_ttft_ms)} ms`} />
            <MiniMetric label="load" value={duration(run.load_ms)} />
            <MiniMetric label="peak RSS" value={bytes(run.peak_rss_bytes)} />
          </div>
          <div className="space-y-1.5">
            {run.suites.map((suite) => (
              <div key={suite.suite} className="flex items-center gap-3">
                <span className="w-40 shrink-0 truncate text-[11.5px] text-muted">{suite.title}</span>
                <div className="h-[5px] flex-1 overflow-hidden rounded-full bg-white/[0.05]">
                  <div
                    className={cx(
                      "h-full rounded-full",
                      suite.pass_rate >= 0.8 ? "bg-ok" : suite.pass_rate >= 0.5 ? "bg-warn" : "bg-bad",
                    )}
                    style={{ width: `${Math.max(2, suite.pass_rate * 100)}%` }}
                  />
                </div>
                <span className="mono-num w-16 shrink-0 text-right text-[11px] text-ink-dim">
                  {suite.passed}/{suite.total}
                </span>
              </div>
            ))}
          </div>
          {run.consistency && (
            <div className="rounded-lg border border-line bg-black/20 px-3 py-2">
              <div className="flex items-center justify-between gap-2">
                <span className="text-[11.5px] text-ink-dim">Consistency across {run.consistency.samples.length} repeats</span>
                <Badge tone={run.consistency.agreement === 1 ? "ok" : "warn"}>
                  {percent(run.consistency.agreement * 100, 0)} agreement
                </Badge>
              </div>
              <div className="mt-1 text-[11px] text-muted">{run.consistency.detail}</div>
              <div className="mt-1.5 flex flex-wrap gap-1">
                {run.consistency.samples.map((sample, i) => (
                  <span key={i} className="chip font-mono text-[10.5px]">
                    {sample.slice(0, 40) || "∅"}
                  </span>
                ))}
              </div>
            </div>
          )}
          <div className="border-t border-line pt-1">
            {run.cases.map((testCase) => (
              <DisclosureRow
                key={testCase.case_id}
                title={
                  <span className="flex items-center gap-2">
                    <span className={cx("h-1.5 w-1.5 rounded-full", testCase.passed ? "bg-ok" : "bg-bad")} />
                    <span>{testCase.title}</span>
                  </span>
                }
                meta={
                  <span className="mono-num">
                    {testCase.tokens_per_second.toFixed(1)} tok/s · {testCase.completion_tokens} tok
                  </span>
                }
              >
                <div className="space-y-2 text-[12px]">
                  <div>
                    <span className="label">Prompt</span>
                    <p className="mt-0.5 whitespace-pre-wrap text-ink-dim">{testCase.prompt}</p>
                  </div>
                  <div>
                    <span className="label">Output</span>
                    <pre className="mt-0.5 max-h-52 overflow-y-auto whitespace-pre-wrap rounded-lg border border-line bg-black/40 p-2 font-mono text-[11.5px] text-[#c3ccdb]">
                      {testCase.output || "∅ empty output"}
                    </pre>
                  </div>
                  <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted">
                    <Badge tone={testCase.passed ? "ok" : "bad"}>{testCase.passed ? "pass" : "fail"}</Badge>
                    <span>{testCase.detail}</span>
                  </div>
                  {testCase.output && (
                    <div className="flex gap-2">
                      <CopyButton text={testCase.output} />
                    </div>
                  )}
                </div>
              </DisclosureRow>
            ))}
          </div>
        </div>
      )}
    </Panel>
  );
}

function MiniMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-line bg-black/20 px-2.5 py-1.5">
      <div className="label">{label}</div>
      <div className="mono-num mt-0.5 text-[13px] text-ink">{value}</div>
    </div>
  );
}

function collectSuites(runs: ModelRun[]): Array<{ suite: string; title: string }> {
  const map = new Map<string, string>();
  runs.forEach((run) => run.suites.forEach((suite) => map.set(suite.suite, suite.title)));
  return Array.from(map.entries()).map(([suite, title]) => ({ suite, title }));
}

export function ComparisonDisclaimer() {
  return (
    <div className="flex items-start gap-2 rounded-lg border border-line bg-white/[0.015] px-3 py-2 text-[11px] leading-relaxed text-muted">
      <AlertTriangle size={12} className="mt-0.5 shrink-0 text-warn" />
      ATOM never invents quality scores. Everything shown was produced by running the selected models on the same
      prompts with the same settings and checking the outputs mechanically.
    </div>
  );
}
