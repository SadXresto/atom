import { useEffect, useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  CircleSlash,
  Cpu,
  FileCheck2,
  Gauge,
  Layers,
  ShieldAlert,
  Sparkles,
  Terminal,
  XCircle,
  Zap,
} from "lucide-react";
import { api } from "../lib/api";
import { bytes, percent, relativeTime } from "../lib/format";
import { useApi } from "../lib/hooks";
import type { FormatOption, JobView, ModelRecord, ToolStatus, Compatibility } from "../lib/types";
import { JobConsole } from "../components/JobConsole";
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
  TextInput,
  cx,
  toast,
} from "../components/ui";

interface QuantizeOptions {
  model_id: string;
  formats: FormatOption[];
  tool: ToolStatus;
  compatibility: Compatibility;
  source_size: number;
}

interface ValidationReport {
  ok: boolean;
  checks: Array<{ name: string; passed: boolean; severity: string; detail: string }>;
  output_sha256: string | null;
  output_bytes: number;
  source_bytes: number;
  savings_percent: number;
  tensor_count: number;
  dominant_quant: string;
  validated_at: string;
}

interface QuantizeResult {
  model_id: string;
  model: ModelRecord;
  validation: ValidationReport;
  duration_ms: number;
  log_path: string;
}

export default function Compression() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const [modelId, setModelId] = useState<string | null>(params.get("model"));
  const [format, setFormat] = useState<string | null>(null);
  const [outputName, setOutputName] = useState("");
  const [jobId, setJobId] = useState<string | null>(null);
  const [result, setResult] = useState<QuantizeResult | null>(null);
  const [starting, setStarting] = useState(false);

  const { data: modelsData, reload: reloadModels } = useApi<{ models: ModelRecord[] }>("/models", 0);
  const models = modelsData?.models ?? [];
  const { data: options, reload: reloadOptions } = useApi<QuantizeOptions>(
    modelId ? `/models/${modelId}/quantize/options` : null,
    0,
  );

  useEffect(() => {
    if (!modelId && models.length) setModelId(models[0].id);
  }, [models, modelId]);

  useEffect(() => {
    setResult(null);
    setJobId(null);
    setOutputName("");
    setFormat(null);
  }, [modelId]);

  useEffect(() => {
    if (!format && options) {
      const first = options.formats.find((f) => f.supported_for_source);
      if (first) setFormat(first.name);
    }
  }, [options, format]);

  const selectedFormat = useMemo(
    () => options?.formats.find((f) => f.name === format) ?? null,
    [options, format],
  );
  const source = models.find((m) => m.id === modelId) ?? null;
  const canRun = Boolean(
    modelId && selectedFormat?.supported_for_source && options?.tool.available && !jobId,
  );

  const start = async () => {
    if (!modelId || !selectedFormat) return;
    setStarting(true);
    setResult(null);
    try {
      const response = await api<{ job_id: string }>(`/models/${modelId}/quantize`, {
        method: "POST",
        body: JSON.stringify({
          format: selectedFormat.name,
          output_name: outputName.trim() || undefined,
        }),
      });
      setJobId(response.job_id);
      toast(`Quantizing to ${selectedFormat.name}`, "info");
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setStarting(false);
    }
  };

  const onFinished = (job: JobView) => {
    if (job.status === "succeeded" && job.result) {
      setResult(job.result as QuantizeResult);
      toast("Compression finished and validated", "ok");
      void reloadModels();
    } else if (job.status === "failed") {
      toast(job.error ?? "Compression failed", "error");
    } else if (job.status === "cancelled") {
      toast("Compression cancelled; partial output discarded", "info");
    }
    setJobId(null);
  };

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="Compression Studio"
        title="Permanent model compression"
        description="Create a separate GGUF file that is genuinely smaller. ATOM runs the installed llama-quantize, parses its real progress, validates the output, and stores it next to — never over — your source."
        actions={
          <Button icon={<Zap size={13} />} onClick={() => navigate("/inference")}>
            Live runtime tuning
          </Button>
        }
      />

      <div className="mb-4 grid gap-3 md:grid-cols-2">
        <ModeCard
          title="Live runtime optimization"
          icon={<Zap size={15} />}
          active={false}
          body="Reversible. Changes context, threads, mmap and KV-cache precision when the model is loaded. The file on disk stays byte-identical."
          action={<Button size="sm" onClick={() => navigate("/inference")} icon={<ArrowRight size={12} />}>Open runtime controls</Button>}
        />
        <ModeCard
          title="Permanent compression"
          icon={<Layers size={15} />}
          active
          body="Irreversible for the output file. Produces a new, smaller GGUF with a recorded source checksum and a validation report."
          action={<Badge tone="accent">you are here</Badge>}
        />
      </div>

      {options && !options.tool.available && (
        <Banner tone="bad" title="llama-quantize is not available">
          {options.tool.message} ATOM will not simulate compression. Install llama.cpp or point Settings at the binary.
        </Banner>
      )}

      <div className="mt-4 grid gap-4 lg:grid-cols-[1fr_1.15fr]">
        <div className="space-y-4">
          <Panel title="Source model" icon={<Cpu size={14} />} subtitle="The original is opened read-only.">
            {models.length === 0 ? (
              <EmptyState
                compact
                icon={<Layers size={16} />}
                title="No models to compress"
                description="Import a GGUF file first."
                action={<Button size="sm" onClick={() => navigate("/library?import=1")}>Import model</Button>}
              />
            ) : (
              <div className="space-y-3">
                <ModelSelect models={models} value={modelId} onChange={setModelId} />
                {source && (
                  <div className="space-y-0.5">
                    <KV label="Quantization" value={source.summary.label_quant} mono />
                    <KV label="File size" value={bytes(source.size_bytes)} mono />
                    <KV label="Architecture" value={source.summary.architecture ?? "—"} />
                    <KV label="Tensors" value={source.summary.tensor_count} mono />
                    <KV label="Params" value={source.summary.parameter_count.toLocaleString()} mono />
                    <KV label="Source SHA-256" value={source.sha256 ? `${source.sha256.slice(0, 16)}…` : "not recorded"} mono />
                  </div>
                )}
                {options && (
                  <div
                    className={cx(
                      "rounded-lg border px-3 py-2 text-[11.5px] leading-relaxed",
                      options.compatibility.quantizable
                        ? "border-ok/25 bg-ok/[0.05] text-ink-dim"
                        : "border-warn/30 bg-warn/[0.06] text-ink-dim",
                    )}
                  >
                    <div className="mb-1 flex items-center gap-1.5 font-medium text-ink">
                      {options.compatibility.quantizable ? (
                        <CheckCircle2 size={13} className="text-ok" />
                      ) : (
                        <AlertTriangle size={13} className="text-warn" />
                      )}
                      {options.compatibility.quantizable ? "Compatible source" : "Not directly quantizable"}
                    </div>
                    {options.compatibility.reason}
                  </div>
                )}
              </div>
            )}
          </Panel>

          <Panel title="Tool" icon={<Terminal size={14} />}>
            {options ? (
              <div className="space-y-0.5">
                <KV label="Binary" value={options.tool.path ?? "not found"} mono />
                <KV
                  label="Format list"
                  value={options.tool.formats_from_tool ? "read from installed binary" : "fallback names"}
                />
                <KV label="Re-quantize flag" value={options.tool.allow_requantize ? "supported" : "not advertised"} />
                <KV
                  label="Advertised types"
                  value={options.tool.supported_types.length ? options.tool.supported_types.join(", ") : "—"}
                  mono
                />
                <div className="pt-2 text-[11px] leading-relaxed text-muted">{options.tool.message}</div>
              </div>
            ) : (
              <div className="text-[12px] text-muted">Select a model to inspect the toolchain.</div>
            )}
          </Panel>
        </div>

        <Panel
          title="Quantization target"
          icon={<Sparkles size={14} />}
          subtitle="Only formats the installed tool advertises and this source supports are enabled."
          actions={selectedFormat && <Badge tone="accent">{selectedFormat.name}</Badge>}
        >
          {options ? (
            <div className="space-y-4">
              <div className="grid gap-2 sm:grid-cols-2">
                {options.formats.map((option) => (
                  <FormatCard
                    key={option.name}
                    option={option}
                    selected={format === option.name}
                    onSelect={() => option.supported_for_source && setFormat(option.name)}
                  />
                ))}
              </div>

              {selectedFormat && (
                <div className="rounded-xl border border-line bg-black/25 px-3.5 py-3">
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <div className="text-[12.5px] font-medium text-ink">
                      {selectedFormat.name} · {selectedFormat.label}
                    </div>
                    <Badge>{selectedFormat.quality_tier} quality</Badge>
                  </div>
                  <div className="grid gap-x-5 sm:grid-cols-2">
                    <KV label="Source size" value={bytes(options.source_size)} mono />
                    <KV label="Estimated output" value={bytes(selectedFormat.estimated_bytes)} mono />
                    <KV
                      label="Estimated saving"
                      value={`${bytes(Math.max(0, selectedFormat.estimated_savings_bytes))} (${percent(
                        Math.max(0, selectedFormat.estimated_savings_percent),
                        1,
                      )})`}
                      mono
                    />
                    <KV label="Bits per weight" value={selectedFormat.bits_per_weight.toFixed(3)} mono />
                  </div>
                  <p className="mt-2 text-[11.5px] leading-relaxed text-muted">{selectedFormat.tradeoff}</p>
                  <p className="mt-1 text-[11px] leading-relaxed text-muted">
                    Size estimate is computed from this file's real tensor element counts; the tool's output will
                    differ slightly. Quality impact is only known by measuring it in the Comparison Lab.
                  </p>
                </div>
              )}

              <Field label="Output file name" hint="stored in the ATOM compressed-models directory">
                <TextInput
                  value={outputName}
                  onChange={(e) => setOutputName(e.target.value)}
                  placeholder={
                    source
                      ? `${source.file_name.replace(/\.gguf$/i, "")}-${(format ?? "q4_k_m").toLowerCase()}.gguf`
                      : "model-q4_k_m.gguf"
                  }
                />
              </Field>

              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="text-[11.5px] text-muted">
                  The source is never overwritten or deleted. Output is validated before it enters the library.
                </div>
                <Button
                  variant="primary"
                  icon={<Layers size={13} />}
                  disabled={!canRun}
                  busy={starting}
                  onClick={start}
                >
                  Start quantization
                </Button>
              </div>

              {jobId && (
                <div className="rounded-xl border border-line bg-black/20 p-3">
                  <JobConsole jobId={jobId} onFinished={onFinished} />
                </div>
              )}

              {result && (
                <div className="space-y-3 rounded-xl border border-ok/25 bg-ok/[0.04] p-3.5">
                  <div className="flex items-center justify-between gap-2">
                    <div className="flex items-center gap-2 text-[12.5px] font-medium text-ink">
                      <FileCheck2 size={14} className="text-ok" /> Output validated and added to the library
                    </div>
                    <Badge tone="ok">{percent(result.validation.savings_percent, 1)} smaller</Badge>
                  </div>
                  <div className="grid gap-x-5 sm:grid-cols-2">
                    <KV label="New model" value={result.model.label} />
                    <KV label="Size" value={`${bytes(result.validation.output_bytes)} (was ${bytes(result.validation.source_bytes)})`} mono />
                    <KV
                      label="Compression ratio"
                      value={`${(result.validation.source_bytes / Math.max(1, result.validation.output_bytes)).toFixed(2)}× smaller`}
                      mono
                    />
                    <KV
                      label="Estimated before run"
                      value={selectedFormat ? `${bytes(selectedFormat.estimated_bytes)} (estimate)` : "—"}
                      mono
                    />
                    <KV label="Output path" value={result.model.path} mono />
                    <KV label="Dominant tensors" value={result.validation.dominant_quant} mono />
                    <KV label="Tensors" value={result.validation.tensor_count} mono />
                    <KV label="Tool time" value={`${(result.duration_ms / 1000).toFixed(1)} s`} mono />
                    <KV label="Output SHA-256" value={result.validation.output_sha256?.slice(0, 16) ?? "—"} mono />
                  </div>
                  <div className="space-y-1.5">
                    {result.validation.checks.map((check) => (
                      <div key={check.name} className="flex items-start gap-2 text-[11.5px]">
                        {check.passed ? (
                          <CheckCircle2 size={12} className="mt-0.5 shrink-0 text-ok" />
                        ) : check.severity === "required" ? (
                          <XCircle size={12} className="mt-0.5 shrink-0 text-bad" />
                        ) : (
                          <AlertTriangle size={12} className="mt-0.5 shrink-0 text-warn" />
                        )}
                        <span className="text-ink-dim">
                          {check.name}
                          <span className="text-muted"> — {check.detail}</span>
                        </span>
                      </div>
                    ))}
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="primary"
                      icon={<Gauge size={12} />}
                      onClick={() => navigate(`/comparison?models=${modelId},${result.model.id}`)}
                    >
                      Measure against the original
                    </Button>
                    <Button size="sm" onClick={() => navigate("/library")}>
                      Open library
                    </Button>
                  </div>
                </div>
              )}

              <div className="rounded-lg border border-line bg-white/[0.015] px-3 py-2.5">
                <div className="label mb-1.5">Workflow</div>
                <ol className="space-y-1 text-[11.5px] leading-relaxed text-muted">
                  <li>1 · Parse the source header and check quantization compatibility.</li>
                  <li>2 · Select a format the installed binary advertises.</li>
                  <li>3 · Run llama-quantize with an explicit argv; stream its tensor progress.</li>
                  <li>4 · Re-parse, checksum and validate the output; discard anything that fails.</li>
                  <li>5 · Store as a separate model linked to its source checksum.</li>
                </ol>
              </div>
            </div>
          ) : (
            <EmptyState compact icon={<ShieldAlert size={16} />} title="Choose a source model" />
          )}
        </Panel>
      </div>

      <div className="mt-4 grid gap-3 sm:grid-cols-3">
        <FootNote
          icon={<ShieldAlert size={14} />}
          title="Originals are protected"
          body="ATOM refuses an output path that equals the source and never deletes source files, even after a failed run."
        />
        <FootNote
          icon={<CircleSlash size={14} />}
          title="No partial results"
          body="Cancellation kills the child process and deletes the partial file. Validation failures are removed too."
        />
        <FootNote
          icon={<AlertTriangle size={14} />}
          title="No quality promises"
          body="Smaller models lose capability. ATOM measures the difference on a fixed prompt suite instead of asserting equivalence."
        />
      </div>
    </div>
  );
}

function ModeCard({
  title,
  body,
  icon,
  active,
  action,
}: {
  title: string;
  body: string;
  icon: React.ReactNode;
  active: boolean;
  action: React.ReactNode;
}) {
  return (
    <div
      className={cx(
        "rounded-xl border p-3.5",
        active ? "border-accent/35 bg-accent/[0.05]" : "border-line bg-white/[0.015]",
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex items-center gap-2">
          <span className={cx("flex h-7 w-7 items-center justify-center rounded-lg border", active ? "border-accent/40 bg-accent/10 text-accent" : "border-line bg-white/[0.03] text-muted")}>
            {icon}
          </span>
          <span className="text-[12.5px] font-medium text-ink">{title}</span>
        </div>
        {active && <span className="h-1.5 w-1.5 rounded-full bg-accent" />}
      </div>
      <p className="mt-2 text-[11.5px] leading-relaxed text-muted">{body}</p>
      <div className="mt-2.5">{action}</div>
    </div>
  );
}

function FormatCard({
  option,
  selected,
  onSelect,
}: {
  option: FormatOption;
  selected: boolean;
  onSelect: () => void;
}) {
  const disabled = !option.supported_for_source;
  return (
    <button
      type="button"
      onClick={onSelect}
      disabled={disabled}
      title={option.note || option.tradeoff}
      className={cx(
        "rounded-xl border px-3 py-2.5 text-left transition-colors",
        selected
          ? "border-accent/55 bg-accent/[0.08]"
          : disabled
            ? "cursor-not-allowed border-line bg-black/20 opacity-55"
            : "border-line bg-white/[0.015] hover:border-line-strong hover:bg-white/[0.03]",
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="mono-num text-[12.5px] font-semibold text-ink">{option.name}</span>
        {selected && <CheckCircle2 size={12} className="text-accent" />}
      </div>
      <div className="mt-0.5 text-[11px] text-muted">{option.label}</div>
      <div className="mt-1.5 flex items-center justify-between gap-2 text-[11px]">
        <span className="mono-num text-ink-dim">{bytes(option.estimated_bytes)}</span>
        <span className={cx("mono-num", option.estimated_savings_percent > 0 ? "text-ok" : "text-muted")}>
          {option.estimated_savings_percent > 0 ? `−${option.estimated_savings_percent.toFixed(0)}%` : "—"}
        </span>
      </div>
      {disabled && <div className="mt-1 line-clamp-2 text-[10.5px] leading-snug text-warn/90">{option.note}</div>}
    </button>
  );
}

function FootNote({ icon, title, body }: { icon: React.ReactNode; title: string; body: string }) {
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
