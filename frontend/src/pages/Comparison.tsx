import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import {
  Beaker,
  CheckCircle2,
  FlaskConical,
  Gauge,
  Layers,
  ListChecks,
  Play,
  RefreshCw,
  Settings2,
  TriangleAlert,
} from "lucide-react";
import { api } from "../lib/api";
import { bytes, relativeTime } from "../lib/format";
import { useApi } from "../lib/hooks";
import type {
  Comparison as ComparisonType,
  ComparisonSummary,
  JobView,
  ModelRecord,
  RunSettings,
  SuiteInfo,
} from "../lib/types";
import { ComparisonDisclaimer, ComparisonView } from "../components/ComparisonView";
import { JobConsole } from "../components/JobConsole";
import { ModelSelect } from "../components/ModelSelect";
import {
  Badge,
  Banner,
  Button,
  EmptyState,
  Field,
  KV,
  Modal,
  PageHeader,
  Panel,
  SegmentedControl,
  Select,
  Stat,
  TextInput,
  Toggle,
  cx,
  toast,
} from "../components/ui";

const defaultSettings: RunSettings = {
  max_tokens: 128,
  temperature: 0.2,
  top_p: 0.95,
  seed: 42,
  context_length: 2048,
  threads: 4,
  batch_size: 512,
  kv_cache_type_k: "f16",
  kv_cache_type_v: "f16",
  gpu_layers: 0,
  use_mmap: true,
  repeats: 3,
};

export default function Comparison() {
  const [params] = useSearchParams();
  const { data: modelsData } = useApi<{ models: ModelRecord[] }>("/models", 0);
  const { data: suitesData } = useApi<{ suites: SuiteInfo[] }>("/bench/suites", 0);
  const { data: listData, reload: reloadList } = useApi<{ comparisons: ComparisonSummary[]; suites: SuiteInfo[] }>(
    "/comparisons",
    0,
  );
  const backend = useApi<{ backend: { inference_ready: boolean; message: string } }>("/backend", 0);

  const models = modelsData?.models ?? [];
  const suites = suitesData?.suites ?? [];
  const history = listData?.comparisons ?? [];

  const preset = (params.get("models") ?? "").split(",").filter(Boolean);
  const [baseline, setBaseline] = useState<string | null>(preset[0] ?? null);
  const [candidate, setCandidate] = useState<string | null>(preset[1] ?? null);
  const [selectedSuites, setSelectedSuites] = useState<string[]>(suites.map((s) => s.id));
  const [settings, setSettings] = useState<RunSettings>(defaultSettings);
  const [jobId, setJobId] = useState<string | null>(null);
  const [comparison, setComparison] = useState<ComparisonType | null>(null);
  const [starting, setStarting] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  useEffect(() => {
    if (suites.length && selectedSuites.length === 0) setSelectedSuites(suites.map((s) => s.id));
  }, [suites, selectedSuites.length]);

  useEffect(() => {
    if (!baseline && models.length) setBaseline(models[0].id);
    if (!candidate && models.length > 1) setCandidate(models[1].id);
  }, [models, baseline, candidate]);

  const canRun = Boolean(baseline && selectedSuites.length && backend.data?.backend.inference_ready);

  const run = async () => {
    if (!baseline) return;
    setStarting(true);
    setComparison(null);
    try {
      const ids = candidate && candidate !== baseline ? [baseline, candidate] : [baseline];
      const response = await api<{ job_id: string }>("/comparisons", {
        method: "POST",
        body: JSON.stringify({
          model_ids: ids,
          suite_ids: selectedSuites,
          settings,
        }),
      });
      setJobId(response.job_id);
      toast("Benchmark queued", "info");
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setStarting(false);
    }
  };

  const onFinished = async (job: JobView) => {
    setJobId(null);
    void reloadList();
    if (job.status === "succeeded" && job.result?.comparison_id) {
      try {
        const loaded = await api<{ comparison: ComparisonType }>(`/comparisons/${job.result.comparison_id}`);
        setComparison(loaded.comparison);
        toast("Comparison complete", "ok");
      } catch (e) {
        toast((e as Error).message, "error");
      }
    } else if (job.status === "failed") {
      toast(job.error ?? "Comparison failed", "error");
    } else if (job.status === "cancelled") {
      toast("Comparison cancelled", "info");
    }
  };

  const openHistory = async (id: string) => {
    try {
      const loaded = await api<{ comparison: ComparisonType }>(`/comparisons/${id}`);
      setComparison(loaded.comparison);
      window.scrollTo({ top: 0, behavior: "smooth" });
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const baselineModel = models.find((m) => m.id === baseline);
  const candidateModel = models.find((m) => m.id === candidate);

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="Comparison Lab"
        title="Measure what actually changed"
        description="Run the same prompts against two models with identical sampling and comparable runtime settings. Every number is measured; quality is derived from deterministic checks on the outputs."
        actions={
          <>
            <Button size="sm" icon={<Settings2 size={13} />} onClick={() => setShowSettings(true)}>
              Run settings
            </Button>
            <Button variant="primary" icon={<Play size={13} />} onClick={run} busy={starting} disabled={!canRun}>
              Run comparison
            </Button>
          </>
        }
      />

      {!backend.data?.backend.inference_ready && (
        <Banner tone="warn" title="Inference backend required">
          {backend.data?.backend.message ??
            "ATOM needs llama-server to benchmark models. Set its path in Settings; ATOM will not fabricate results."}
        </Banner>
      )}

      <div className="mt-4 grid gap-4 lg:grid-cols-[1fr_1.4fr]">
        <div className="space-y-4">
          <Panel title="Models" icon={<Layers size={14} />} subtitle="Baseline first, candidate second.">
            <div className="space-y-3">
              <Field label="Baseline" hint={baselineModel ? `${bytes(baselineModel.size_bytes)} · ${baselineModel.summary.label_quant}` : undefined}>
                <ModelSelect
                  models={models}
                  value={baseline}
                  onChange={(id) => {
                    setBaseline(id);
                    if (candidate === id) setCandidate(null);
                  }}
                />
              </Field>
              <Field label="Candidate" hint={candidateModel ? `${bytes(candidateModel.size_bytes)} · ${candidateModel.summary.label_quant}` : undefined}>
                <Select value={candidate ?? ""} onChange={(e) => setCandidate(e.target.value || null)}>
                  <option value="">(single-model benchmark)</option>
                  {models
                    .filter((m) => m.id !== baseline)
                    .map((m) => (
                      <option key={m.id} value={m.id}>
                        {m.label} · {m.summary.label_quant} · {bytes(m.size_bytes)}
                      </option>
                    ))}
                </Select>
              </Field>
              {baselineModel && candidateModel && (
                <div className="rounded-lg border border-line bg-black/20 px-3 py-2 text-[11.5px] text-muted">
                  Disk difference:{" "}
                  <span className="mono-num text-ink-dim">
                    {bytes(Math.abs(baselineModel.size_bytes - candidateModel.size_bytes))} (
                    {(((baselineModel.size_bytes - candidateModel.size_bytes) / Math.max(1, baselineModel.size_bytes)) * 100).toFixed(1)}
                    %)
                  </span>
                </div>
              )}
            </div>
          </Panel>

          <Panel
            title="Suites"
            icon={<ListChecks size={14} />}
            subtitle="Deterministic checks; each case has one verifiable answer or shape."
            actions={<Badge>{selectedSuites.length}/{suites.length}</Badge>}
          >
            <div className="space-y-1.5">
              {suites.map((suite) => {
                const active = selectedSuites.includes(suite.id);
                return (
                  <button
                    key={suite.id}
                    onClick={() =>
                      setSelectedSuites((prev) =>
                        prev.includes(suite.id) ? prev.filter((s) => s !== suite.id) : [...prev, suite.id],
                      )
                    }
                    className={cx(
                      "flex w-full items-start gap-2.5 rounded-lg border px-2.5 py-2 text-left transition-colors",
                      active ? "border-accent/35 bg-accent/[0.06]" : "border-line bg-white/[0.012] hover:border-line-strong",
                    )}
                  >
                    <span
                      className={cx(
                        "mt-0.5 flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded border",
                        active ? "border-accent bg-accent/25" : "border-line-strong",
                      )}
                    >
                      {active && <CheckCircle2 size={10} className="text-accent" />}
                    </span>
                    <span className="min-w-0">
                      <span className="block text-[12px] text-ink-dim">{suite.title}</span>
                      <span className="block text-[11px] leading-snug text-muted">{suite.description}</span>
                    </span>
                  </button>
                );
              })}
            </div>
            <div className="mt-3 grid grid-cols-2 gap-2">
              <div className="rounded-lg border border-line bg-black/20 px-2.5 py-1.5">
                <div className="label">max tokens</div>
                <div className="mono-num text-[12.5px] text-ink">{settings.max_tokens}</div>
              </div>
              <div className="rounded-lg border border-line bg-black/20 px-2.5 py-1.5">
                <div className="label">context</div>
                <div className="mono-num text-[12.5px] text-ink">{settings.context_length}</div>
              </div>
            </div>
          </Panel>

          <Panel title="Run" icon={<FlaskConical size={14} />}>
            {jobId ? (
              <JobConsole jobId={jobId} onFinished={onFinished} />
            ) : (
              <div className="space-y-3">
                <p className="text-[12px] leading-relaxed text-muted">
                  {baseline
                    ? "ATOM loads each model in turn with identical settings, runs every selected case, and unloads before the next."
                    : "Select a baseline model to begin."}
                </p>
                <div className="flex items-center gap-2">
                  <Button variant="primary" icon={<Play size={13} />} onClick={run} disabled={!canRun} busy={starting} className="flex-1">
                    Run {selectedSuites.length} suites
                  </Button>
                  <Button icon={<RefreshCw size={13} />} onClick={() => setShowSettings(true)}>
                    Settings
                  </Button>
                </div>
                <ComparisonDisclaimer />
              </div>
            )}
          </Panel>
        </div>

        <div className="space-y-4">
          {comparison ? (
            <ComparisonView comparison={comparison} />
          ) : jobId ? (
            <Panel title="Running benchmark" icon={<Beaker size={14} />}>
              <EmptyState
                compact
                icon={<Gauge size={16} />}
                title="Measuring…"
                description="Each case is generating real tokens. Progress and per-case results appear in the job log."
              />
            </Panel>
          ) : history.length ? (
            <Panel
              title="Recent comparisons"
              icon={<FlaskConical size={14} />}
              subtitle="Open a saved run to inspect or export it."
            >
              <div className="space-y-1.5">
                {history.slice(0, 8).map((item) => (
                  <button
                    key={item.id}
                    onClick={() => void openHistory(item.id)}
                    className="flex w-full flex-wrap items-center justify-between gap-2 rounded-lg border border-line bg-white/[0.015] px-3 py-2 text-left hover:border-line-strong"
                  >
                    <div className="min-w-0">
                      <div className="truncate text-[12.5px] text-ink-dim">{item.title}</div>
                      <div className="text-[11px] text-muted">
                        {item.models.join(" → ")} · {relativeTime(item.created_at)}
                      </div>
                    </div>
                    <div className="flex items-center gap-2 text-[11px]">
                      {item.size_saved_percent !== null && (
                        <Badge tone="ok">−{item.size_saved_percent.toFixed(0)}% disk</Badge>
                      )}
                      {item.speed_change_percent !== null && (
                        <Badge tone={item.speed_change_percent >= 0 ? "accent" : "warn"}>
                          {item.speed_change_percent >= 0 ? "+" : ""}
                          {item.speed_change_percent.toFixed(0)}% tok/s
                        </Badge>
                      )}
                    </div>
                  </button>
                ))}
              </div>
            </Panel>
          ) : (
            <Panel title="No results yet" icon={<FlaskConical size={14} />}>
              <EmptyState
                icon={<Gauge size={17} />}
                title="The lab is ready"
                description="Pick a baseline and candidate, choose suites, and run. Results are stored with their settings and environment so they can be reproduced."
              />
            </Panel>
          )}

          {history.length > 0 && comparison && (
            <Panel title="Earlier runs" icon={<FlaskConical size={14} />}>
              <div className="flex flex-wrap gap-1.5">
                {history.slice(0, 10).map((item) => (
                  <button
                    key={item.id}
                    onClick={() => void openHistory(item.id)}
                    className={cx(
                      "chip hover:border-line-strong hover:text-ink",
                      comparison.id === item.id && "border-accent/40 text-accent",
                    )}
                  >
                    {item.title} · {relativeTime(item.created_at)}
                  </button>
                ))}
              </div>
            </Panel>
          )}
        </div>
      </div>

      <Modal
        open={showSettings}
        onClose={() => setShowSettings(false)}
        title="Benchmark run settings"
        description="Applied identically to every model in the comparison and saved with the results."
        width="max-w-2xl"
        footer={<Button variant="primary" onClick={() => setShowSettings(false)}>Done</Button>}
      >
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label="Max tokens per answer">
            <TextInput
              type="number"
              min={16}
              max={1024}
              value={settings.max_tokens}
              onChange={(e) => setSettings({ ...settings, max_tokens: Number(e.target.value) || 128 })}
            />
          </Field>
          <Field label="Temperature">
            <TextInput
              type="number"
              step="0.05"
              min={0}
              max={2}
              value={settings.temperature}
              onChange={(e) => setSettings({ ...settings, temperature: Number(e.target.value) })}
            />
          </Field>
          <Field label="Top-p">
            <TextInput
              type="number"
              step="0.05"
              min={0}
              max={1}
              value={settings.top_p}
              onChange={(e) => setSettings({ ...settings, top_p: Number(e.target.value) })}
            />
          </Field>
          <Field label="Seed" hint="fixed for reproducibility">
            <TextInput
              type="number"
              value={settings.seed ?? ""}
              onChange={(e) => setSettings({ ...settings, seed: e.target.value === "" ? null : Number(e.target.value) })}
            />
          </Field>
          <Field label="Context length">
            <TextInput
              type="number"
              min={256}
              step={256}
              value={settings.context_length}
              onChange={(e) => setSettings({ ...settings, context_length: Number(e.target.value) || 2048 })}
            />
          </Field>
          <Field label="Threads">
            <TextInput
              type="number"
              min={1}
              max={256}
              value={settings.threads}
              onChange={(e) => setSettings({ ...settings, threads: Number(e.target.value) || 4 })}
            />
          </Field>
          <Field label="KV cache K">
            <Select value={settings.kv_cache_type_k} onChange={(e) => setSettings({ ...settings, kv_cache_type_k: e.target.value })}>
              {["f16", "f32", "q8_0", "q4_0", "q5_0"].map((v) => (
                <option key={v} value={v}>{v}</option>
              ))}
            </Select>
          </Field>
          <Field label="KV cache V">
            <Select value={settings.kv_cache_type_v} onChange={(e) => setSettings({ ...settings, kv_cache_type_v: e.target.value })}>
              {["f16", "f32", "q8_0", "q4_0", "q5_0"].map((v) => (
                <option key={v} value={v}>{v}</option>
              ))}
            </Select>
          </Field>
          <Field label="Consistency repeats" hint="same prompt N times">
            <TextInput
              type="number"
              min={2}
              max={8}
              value={settings.repeats}
              onChange={(e) => setSettings({ ...settings, repeats: Number(e.target.value) || 3 })}
            />
          </Field>
          <Field label="GPU layers" hint="0 = CPU only">
            <TextInput
              type="number"
              min={0}
              value={settings.gpu_layers}
              onChange={(e) => setSettings({ ...settings, gpu_layers: Number(e.target.value) || 0 })}
            />
          </Field>
        </div>
        <div className="mt-3 border-t border-line pt-2">
          <Toggle
            checked={settings.use_mmap}
            onChange={(v) => setSettings({ ...settings, use_mmap: v })}
            label="Memory-map model weights"
            description="Lets the OS page weights from disk instead of holding a private copy in RAM."
          />
          <KV label="Batch size" value={settings.batch_size} mono />
        </div>
        <div className="mt-2">
          <Banner tone="warn" title="Comparability note">
            Memory-mapping means the model file counts as page cache, not anonymous memory. Compare peak RSS only
            between runs that used the same mmap setting.
          </Banner>
        </div>
      </Modal>
    </div>
  );
}
