import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  ArrowLeft,
  ArrowRight,
  Boxes,
  Check,
  Cpu,
  Download,
  FileCode2,
  Gauge,
  HardDrive,
  MemoryStick,
  Microchip,
  Play,
  Rocket,
  Search,
  Sparkles,
  Trash2,
  Upload,
  Wand2,
  Zap,
} from "lucide-react";
import { api } from "../lib/api";
import { bytes, dateTime, duration, number, params, percent, relativeTime } from "../lib/format";
import { useApi } from "../lib/hooks";
import type {
  Config,
  MicroArchitecture,
  MicroInferResponse,
  MicroProject,
  MicroTarget,
} from "../lib/types";
import { JobConsole } from "../components/JobConsole";
import { LossChart } from "../components/Charts";
import {
  Badge,
  Banner,
  Button,
  ConfirmDialog,
  EmptyState,
  Field,
  KV,
  Modal,
  PageHeader,
  Panel,
  Progress,
  Select,
  Spinner,
  TextInput,
  cx,
  toast,
} from "../components/ui";
import {
  CapabilityNote,
  FormatBadge,
  MICRO_STEPS,
  StepRail,
  WarningLine,
  useMicroProject,
} from "../components/MicroBits";

const QUANT_FORMATS = [
  { id: "int8", label: "Int8", bits: 8, note: "Symmetric per-row. The default for microcontrollers." },
  { id: "int4", label: "Int4", bits: 4, note: "Half the flash again; the largest loss delta." },
  { id: "f16", label: "Float16", bits: 16, note: "Lossless-ish reference; too large for most MCUs." },
  { id: "f32", label: "Float32", bits: 32, note: "Quantization reference only. Largest artifact." },
];

export default function MicroStudio() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { data, error, reload, loading } = useMicroProject(id ?? null);
  const { data: catalog } = useApi<{ targets: MicroTarget[]; architectures: MicroArchitecture[] }>("/micro/catalog");
  const { data: config } = useApi<Config>("/settings");

  const project = data?.project;
  const targets = catalog?.targets ?? [];
  const architectures = catalog?.architectures ?? [];
  const target = useMemo(() => targets.find((t) => t.id === project?.target_id), [targets, project?.target_id]);

  const [step, setStep] = useState(1);
  const [initialized, setInitialized] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [jobId, setJobId] = useState<string | null>(null);
  const [archId, setArchId] = useState("atom-mlp-small");
  const [hyper, setHyper] = useState({
    context: 8,
    hidden: 48,
    epochs: 12,
    learning_rate: 0.04,
    batch: 16,
    seed: 1337,
    windows_per_epoch: 10000,
  });
  const [datasetPath, setDatasetPath] = useState("");
  const [benchPath, setBenchPath] = useState("");
  const [quantFormat, setQuantFormat] = useState("int8");
  const [inferPrompt, setInferPrompt] = useState("LED");
  const [inferOut, setInferOut] = useState<MicroInferResponse | null>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [meta, setMeta] = useState({ name: "", domain: "", description: "", target_id: "" });

  useEffect(() => {
    if (!project || initialized) return;
    setStep(Math.min(11, project.next_step));
    setMeta({
      name: project.name,
      domain: project.domain,
      description: project.description,
      target_id: project.target_id,
    });
    if (project.architecture) {
      setArchId(project.architecture.spec_id);
      setHyper({
        context: project.architecture.context,
        hidden: project.architecture.hidden,
        epochs: project.architecture.epochs,
        learning_rate: project.architecture.learning_rate,
        batch: project.architecture.batch,
        seed: project.architecture.seed,
        windows_per_epoch: project.architecture.windows_per_epoch,
      });
    }
    if (project.quantization) setQuantFormat(project.quantization.format);
    setInitialized(true);
  }, [project, initialized]);

  useEffect(() => {
    if (!architectures.length) return;
    const spec = architectures.find((a) => a.id === archId);
    if (!spec) return;
    setHyper((prev) => ({ ...prev, context: spec.context, hidden: spec.hidden, epochs: spec.default_epochs, learning_rate: spec.default_lr, batch: spec.default_batch, windows_per_epoch: spec.windows_per_epoch }));
  }, [archId, architectures]);

  const call = async (key: string, action: () => Promise<unknown>, after?: () => void) => {
    setBusy(key);
    try {
      await action();
      after?.();
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(null);
    }
  };

  if (!id) {
    return <EmptyState title="No project selected" description="Open a project from Micro Studio." />;
  }
  if (error) {
    return (
      <Banner tone="bad" title="Could not load this project">
        {error.message}
      </Banner>
    );
  }
  if (!project) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Spinner size={20} />
      </div>
    );
  }

  const completed = project.completed_steps ?? [];
  const nextStep = Math.min(11, project.next_step ?? 1);
  const spec = architectures.find((a) => a.id === archId);
  const report = project.training?.report;
  const quant = project.quantization;
  const exportReport = project.export;
  const firmware = project.firmware;
  const benchmarks = project.benchmarks;

  const patchMeta = () =>
    call("meta", async () => {
      await api(`/micro/projects/${project.id}`, { method: "PATCH", body: JSON.stringify(meta) });
      toast("Project updated", "ok");
      void reload();
    });

  const importDataset = () =>
    call("dataset", async () => {
      const result = await api<{ project: MicroProject }>(`/micro/projects/${project.id}/dataset`, {
        method: "POST",
        body: JSON.stringify({ path: datasetPath, seed: 1337, val_ratio: 0.15 }),
      });
      toast(`Imported ${result.project.dataset?.stats.documents ?? 0} documents`, "ok");
      setDatasetPath("");
      void reload();
    });

  const startTraining = async () => {
    if (!spec) return;
    setBusy("train");
    try {
      const result = await api<{ job_id: string }>(`/micro/projects/${project.id}/train`, {
        method: "POST",
        body: JSON.stringify({ spec_id: archId, ...hyper }),
      });
      setJobId(result.job_id);
      setStep(5);
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(null);
    }
  };

  const evaluate = () =>
    call("evaluate", async () => {
      const result = await api<{ evaluation: unknown }>(`/micro/projects/${project.id}/evaluate`, { method: "POST" });
      toast(`Validation perplexity ${(result.evaluation as { perplexity: number }).perplexity.toFixed(3)}`, "ok");
      void reload();
    });

  const quantize = () =>
    call("quantize", async () => {
      const result = await api<{ quantization: { report: { perplexity: number } } }>(
        `/micro/projects/${project.id}/quantize`,
        { method: "POST", body: JSON.stringify({ format: quantFormat }) },
      );
      toast(`Quantized to ${quantFormat}; measured perplexity ${result.quantization.report.perplexity.toFixed(3)}`, "ok");
      void reload();
    });

  const analyseMemory = (targetId?: string) =>
    call("memory", async () => {
      await api(`/micro/projects/${project.id}/memory`, {
        method: "POST",
        body: JSON.stringify(targetId ? { target_id: targetId } : {}),
      });
      void reload();
    });

  const runExport = () =>
    call("export", async () => {
      await api(`/micro/projects/${project.id}/export`, { method: "POST" });
      toast("Deployment artifacts written with checksums", "ok");
      void reload();
    });

  const buildFirmware = async (upload: boolean) => {
    setBusy(upload ? "deploy" : "build");
    try {
      const result = await api<{ job_id: string }>(
        `/micro/projects/${project.id}/firmware/${upload ? "deploy" : "build"}`,
        { method: "POST" },
      );
      setJobId(result.job_id);
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(null);
    }
  };

  const importBench = () =>
    call("bench", async () => {
      const result = await api<{ runs: unknown[] }>(`/micro/projects/${project.id}/benchmarks`, {
        method: "POST",
        body: JSON.stringify({ path: benchPath }),
      });
      toast(`Imported ${result.runs.length} measured runs`, "ok");
      setBenchPath("");
      void reload();
    });

  const infer = () =>
    call("infer", async () => {
      const result = await api<MicroInferResponse>(`/micro/projects/${project.id}/infer`, {
        method: "POST",
        body: JSON.stringify({ prompt: inferPrompt, max_tokens: 160, temperature: 0.7 }),
      });
      setInferOut(result);
    });

  const remove = async () => {
    try {
      await api(`/micro/projects/${project.id}?confirm=${project.id}`, { method: "DELETE" });
      toast("Project deleted", "ok");
      navigate("/micro");
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const go = (next: number) => {
    setStep(Math.max(1, Math.min(11, next)));
    window.scrollTo({ top: 0, behavior: "smooth" });
  };

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow={
          <span className="flex items-center gap-2">
            <button onClick={() => navigate("/micro")} className="flex items-center gap-1 hover:text-ink-dim">
              <ArrowLeft size={12} /> Micro Studio
            </button>
          </span>
        }
        title={project.name}
        description={
          <>
            {project.description || "Specialist model project"} ·{" "}
            <span className="text-ink-dim">{project.domain || "no domain set"}</span>
          </>
        }
        actions={
          <>
            <Badge tone="neutral">{target?.name ?? project.target_id}</Badge>
            <Badge tone="accent">
              step {nextStep} of 11
            </Badge>
            <Button onClick={() => go(nextStep)} icon={<ArrowRight size={13} />}>
              Continue
            </Button>
          </>
        }
      />

      <div className="panel-flat mb-4 flex flex-wrap items-center gap-x-5 gap-y-2 px-3.5 py-2.5 text-[11.5px] text-muted">
        <span className="flex items-center gap-1.5">
          <Boxes size={12} /> {project.dataset?.stats.documents ?? 0} documents
        </span>
        <span className="flex items-center gap-1.5">
          <Wand2 size={12} /> {report ? `${params(report.params)} params` : "untrained"}
        </span>
        <span className="flex items-center gap-1.5">
          <Gauge size={12} /> {report ? `val ppl ${report.perplexity.toFixed(2)}` : "no validation yet"}
        </span>
        <span className="flex items-center gap-1.5">
          <MemoryStick size={12} /> {target ? `${bytes(target.sram_bytes)} SRAM` : "—"}
        </span>
        <span className="flex items-center gap-1.5">
          <HardDrive size={12} /> {target ? `${bytes(target.flash_bytes)} flash` : "—"}
        </span>
        <span className="ml-auto">updated {relativeTime(project.updated_at)}</span>
      </div>

      <div className="grid gap-4 lg:grid-cols-[236px_minmax(0,1fr)]">
        <div className="lg:sticky lg:top-4 lg:self-start">
          <StepRail active={step} completed={completed} onSelect={go} />
          <div className="mt-2 px-1 text-[10.5px] leading-relaxed text-muted">
            {completed.length} of 11 steps complete. Click any step to revisit it.
          </div>
        </div>

        <div className="min-w-0 space-y-4">
          {jobId && (
            <Panel
              title="Running job"
              subtitle="Real progress and tool output. Cancelling discards partial artifacts."
              icon={<Zap size={14} />}
              actions={
                <Button size="sm" onClick={() => setJobId(null)}>
                  Dismiss
                </Button>
              }
            >
              <JobConsole
                jobId={jobId}
                onFinished={(job) => {
                  void reload();
                  if (job.status === "succeeded") toast(`${job.title} finished`, "ok");
                }}
              />
            </Panel>
          )}

          {step === 1 && (
            <Panel title="Step 1 · Create the specialist project" icon={<Microchip size={14} />}>
              <div className="grid gap-3 sm:grid-cols-2">
                <Field label="Project name">
                  <TextInput value={meta.name} onChange={(e) => setMeta({ ...meta, name: e.target.value })} />
                </Field>
                <Field label="Specialist domain">
                  <TextInput value={meta.domain} onChange={(e) => setMeta({ ...meta, domain: e.target.value })} />
                </Field>
                <Field label="Description" className="sm:col-span-2">
                  <TextInput
                    value={meta.description}
                    onChange={(e) => setMeta({ ...meta, description: e.target.value })}
                  />
                </Field>
              </div>
              <div className="mt-4 grid gap-3 sm:grid-cols-2">
                <div className="panel-flat p-3">
                  <div className="label mb-2">Record</div>
                  <KV label="Created" value={dateTime(project.created_at)} />
                  <KV label="Project id" value={project.id.slice(0, 8)} mono />
                  <KV label="Data" value={`saved under the Micro root`} />
                </div>
                <div className="panel-flat p-3">
                  <div className="label mb-2">What happens next</div>
                  <p className="text-[12px] leading-relaxed text-muted">
                    Pick the exact board, import a dataset and train a real model. Deleting this project removes its
                    weights, exports and firmware, but never touches the dataset source file.
                  </p>
                </div>
              </div>
              <div className="mt-4 flex flex-wrap items-center justify-between gap-2">
                <Button variant="danger" size="sm" onClick={() => setDeleteOpen(true)} icon={<Trash2 size={12} />}>
                  Delete project
                </Button>
                <Button variant="primary" busy={busy === "meta"} onClick={patchMeta} icon={<Check size={13} />}>
                  Save project
                </Button>
              </div>
            </Panel>
          )}

          {step === 2 && (
            <Panel
              title="Step 2 · Target hardware"
              subtitle="Memory limits come from the part's datasheet. The fit analysis in step 8 uses them."
              icon={<Cpu size={14} />}
            >
              <div className="grid gap-2.5 md:grid-cols-2">
                {targets.map((item) => {
                  const selected = item.id === project.target_id;
                  return (
                    <button
                      key={item.id}
                      onClick={() =>
                        call("target", async () => {
                          await api(`/micro/projects/${project.id}`, {
                            method: "PATCH",
                            body: JSON.stringify({ target_id: item.id }),
                          });
                          setMeta({ ...meta, target_id: item.id });
                          void reload();
                        })
                      }
                      className={cx(
                        "rounded-xl border px-3.5 py-3 text-left transition-colors",
                        selected ? "border-accent/45 bg-accent/[0.07]" : "border-line bg-white/[0.015] hover:border-line-strong",
                      )}
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="text-[12.5px] font-medium text-ink">{item.name}</span>
                        {selected && <Badge tone="accent">selected</Badge>}
                      </div>
                      <div className="mt-1 text-[11px] text-muted">
                        {item.vendor} · {item.mcu} · {item.cores} core{item.cores > 1 ? "s" : ""}
                        {item.cpu_mhz ? ` @ ${item.cpu_mhz} MHz` : ""}
                      </div>
                      <div className="mt-2 flex flex-wrap gap-3 text-[10.5px] text-muted">
                        <span className="mono-num">{bytes(item.sram_bytes)} SRAM</span>
                        <span className="mono-num">{bytes(item.flash_bytes)} flash</span>
                        {item.psram_bytes > 0 && <span className="mono-num">{bytes(item.psram_bytes)} PSRAM</span>}
                        <span>{item.fp32_fpu ? "FPU" : "no FPU"}</span>
                      </div>
                      <p className="mt-2 text-[11px] leading-relaxed text-muted">{item.notes}</p>
                    </button>
                  );
                })}
              </div>
              <div className="mt-3 text-[11px] text-muted">
                Toolchain: PlatformIO with board <span className="font-mono text-ink-dim">{target?.platformio_board ?? "—"}</span>.
              </div>
            </Panel>
          )}

          {step === 3 && (
            <Panel
              title="Step 3 · Import or prepare a dataset"
              subtitle="Plain text, JSON or JSONL. ATOM computes the statistics from the bytes it reads and keeps a normalized copy in the project."
              icon={<Upload size={14} />}
              actions={project.dataset ? <Badge tone="ok">imported</Badge> : <Badge tone="warn">required</Badge>}
            >
              <div className="flex flex-col gap-2 sm:flex-row">
                <TextInput
                  value={datasetPath}
                  onChange={(e) => setDatasetPath(e.target.value)}
                  placeholder="~/datasets/desk-commands.jsonl"
                  className="flex-1"
                />
                <Button variant="primary" busy={busy === "dataset"} onClick={importDataset} icon={<Upload size={13} />}>
                  Import dataset
                </Button>
              </div>
              <div className="mt-2 text-[11px] leading-relaxed text-muted">
                Allowed roots:{" "}
                {config?.dataset_roots.map((root) => (
                  <span key={root} className="mr-2 font-mono text-[10.5px] text-ink-dim">
                    {root}
                  </span>
                ))}
              </div>

              {project.dataset ? (
                <>
                  <div className="mt-4 grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                    <MiniMetric label="Documents" value={number(project.dataset.stats.documents)} />
                    <MiniMetric label="Characters" value={number(project.dataset.stats.chars)} />
                    <MiniMetric label="Training split" value={`${number(project.dataset.stats.train_chars)} chars`} />
                    <MiniMetric label="Validation split" value={`${number(project.dataset.stats.val_chars)} chars`} />
                    <MiniMetric label="Unique bytes" value={`${project.dataset.stats.unique_bytes} / 256`} />
                    <MiniMetric label="Entropy" value={`${project.dataset.stats.entropy_bits_per_byte.toFixed(2)} bits/B`} />
                    <MiniMetric
                      label="Duplicates"
                      value={`${project.dataset.stats.duplicate_documents}`}
                      tone={project.dataset.stats.duplicate_documents > 0 ? "warn" : undefined}
                    />
                    <MiniMetric label="Format" value={project.dataset.format} />
                  </div>

                  <div className="mt-4 grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
                    <div>
                      <div className="label mb-2">Most frequent bytes</div>
                      <div className="space-y-1.5">
                        {project.dataset.stats.top_bytes.slice(0, 8).map((share) => (
                          <div key={share.byte}>
                            <div className="mb-0.5 flex items-center justify-between text-[10.5px]">
                              <span className="font-mono text-ink-dim">{share.display}</span>
                              <span className="mono-num text-muted">{percent(share.share * 100, 2)}</span>
                            </div>
                            <div className="h-[5px] overflow-hidden rounded-full bg-white/[0.05]">
                              <div
                                className="h-full rounded-full bg-gradient-to-r from-accent to-accent-2"
                                style={{ width: `${Math.max(2, share.share * 100)}%` }}
                              />
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                    <div>
                      <div className="label mb-2">Preview</div>
                      <div className="space-y-2">
                        {project.dataset.preview.map((item) => (
                          <div key={item.index} className="rounded-lg border border-line bg-black/25 px-2.5 py-2">
                            <div className="mb-1 flex items-center justify-between text-[10px] text-muted">
                              <span>document {item.index}</span>
                              <span className="mono-num">{item.chars} chars</span>
                            </div>
                            <div className="font-mono text-[11px] leading-relaxed text-ink-dim">{item.excerpt}</div>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>

                  <div className="mt-3 flex items-center justify-between gap-2 text-[11px] text-muted">
                    <span className="truncate font-mono">{project.dataset.source_path}</span>
                    <span className="shrink-0">split seed {project.dataset.split_seed} · val {percent(project.dataset.stats.val_ratio * 100)}</span>
                  </div>
                </>
              ) : (
                <div className="mt-4">
                  <Banner tone="info" title="Supported shapes">
                    <code className="font-mono text-[11px]">{"{\"text\": \"...\"}"}</code> per JSONL line, or{" "}
                    <code className="font-mono text-[11px]">{"{\"prompt\": \"...\", \"completion\": \"...\"}"}</code>, or a plain
                    text file. At least 200 characters are required.
                  </Banner>
                </div>
              )}
            </Panel>
          )}

          {step === 4 && (
            <Panel
              title="Step 4 · Model architecture"
              subtitle="The built-in byte MLP is trained in this process. Transformer options are executed by the external PyTorch adapter."
              icon={<Boxes size={14} />}
            >
              <div className="grid gap-2.5 md:grid-cols-2">
                {architectures.map((arch) => {
                  const selected = arch.id === archId;
                  const enabled = arch.trainable_here || Boolean(data?.capabilities.pytorch.available);
                  return (
                    <button
                      key={arch.id}
                      disabled={!enabled}
                      onClick={() => setArchId(arch.id)}
                      className={cx(
                        "rounded-xl border px-3.5 py-3 text-left transition-colors",
                        selected ? "border-accent/45 bg-accent/[0.07]" : "border-line bg-white/[0.015] hover:border-line-strong",
                        !enabled && "opacity-55",
                      )}
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="text-[12.5px] font-medium text-ink">{arch.name}</span>
                        <Badge tone={arch.trainable_here ? "ok" : enabled ? "warn" : "bad"}>
                          {arch.trainable_here ? "built-in" : enabled ? "adapter" : "unavailable"}
                        </Badge>
                      </div>
                      <div className="mt-1 text-[11px] text-muted">{arch.summary}</div>
                      <div className="mt-2 flex flex-wrap gap-3 text-[10.5px] text-muted">
                        <span className="mono-num">{params(arch.parameters)} params</span>
                        <span className="mono-num">ctx {arch.context}</span>
                        <span className="mono-num">hidden {arch.hidden}</span>
                        <span className="mono-num">int8 {bytes(Math.round(arch.parameters * 1.1))}</span>
                      </div>
                      <p className="mt-2 text-[11px] leading-relaxed text-muted">{arch.description}</p>
                      {!arch.trainable_here && (
                        <div className="mt-1.5 text-[10.5px] text-warn">Requires {arch.requires}</div>
                      )}
                    </button>
                  );
                })}
              </div>
              {!data?.capabilities.pytorch.available && (
                <div className="mt-3">
                  <Banner tone="warn" title="PyTorch not detected">
                    Transformer choices are disabled. The built-in Atom MLP architectures train in this process and
                    support the whole workflow. Install PyTorch to enable the adapter.
                  </Banner>
                </div>
              )}
              {spec && (
                <div className="mt-4 grid gap-3 sm:grid-cols-2">
                  <div className="panel-flat p-3">
                    <div className="label mb-2">Selected</div>
                    <KV label="Architecture" value={spec.name} />
                    <KV label="Family" value={spec.family} />
                    <KV label="Best for" value={spec.best_for} />
                  </div>
                  <div className="panel-flat p-3">
                    <div className="label mb-2">Training defaults</div>
                    <KV label="Epochs" value={spec.default_epochs} mono />
                    <KV label="Learning rate" value={spec.default_lr} mono />
                    <KV label="Batch" value={spec.default_batch} mono />
                    <KV label="Windows / epoch" value={number(spec.windows_per_epoch)} mono />
                  </div>
                </div>
              )}
            </Panel>
          )}

          {step === 5 && (
            <Panel
              title="Step 5 · Configure and run training"
              subtitle="Byte-level SGD with softmax cross-entropy. Progress and the loss curve come from the actual epochs."
              icon={<Wand2 size={14} />}
              actions={report ? <Badge tone="ok">trained</Badge> : <Badge tone="neutral">not trained</Badge>}
            >
              <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                <Field label="Epochs">
                  <TextInput
                    type="number"
                    value={hyper.epochs}
                    onChange={(e) => setHyper({ ...hyper, epochs: Number(e.target.value) })}
                  />
                </Field>
                <Field label="Learning rate" hint="SGD">
                  <TextInput
                    type="number"
                    step="0.005"
                    value={hyper.learning_rate}
                    onChange={(e) => setHyper({ ...hyper, learning_rate: Number(e.target.value) })}
                  />
                </Field>
                <Field label="Batch size">
                  <TextInput
                    type="number"
                    value={hyper.batch}
                    onChange={(e) => setHyper({ ...hyper, batch: Number(e.target.value) })}
                  />
                </Field>
                <Field label="Seed">
                  <TextInput
                    type="number"
                    value={hyper.seed}
                    onChange={(e) => setHyper({ ...hyper, seed: Number(e.target.value) })}
                  />
                </Field>
                <Field label="Context bytes">
                  <TextInput
                    type="number"
                    value={hyper.context}
                    onChange={(e) => setHyper({ ...hyper, context: Number(e.target.value) })}
                  />
                </Field>
                <Field label="Hidden units">
                  <TextInput
                    type="number"
                    value={hyper.hidden}
                    onChange={(e) => setHyper({ ...hyper, hidden: Number(e.target.value) })}
                  />
                </Field>
                <Field label="Windows / epoch">
                  <TextInput
                    type="number"
                    value={hyper.windows_per_epoch}
                    onChange={(e) => setHyper({ ...hyper, windows_per_epoch: Number(e.target.value) })}
                  />
                </Field>
                <div className="flex items-end">
                  <Button
                    variant="primary"
                    className="w-full"
                    busy={busy === "train"}
                    disabled={!project.dataset || !spec?.trainable_here}
                    onClick={startTraining}
                    icon={<Play size={13} />}
                  >
                    Start training
                  </Button>
                </div>
              </div>
              {!project.dataset && (
                <div className="mt-3">
                  <Banner tone="warn" title="Import a dataset first">
                    Step 3 creates the split this trainer consumes.
                  </Banner>
                </div>
              )}

              {report && (
                <div className="mt-5 space-y-4">
                  <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                    <MiniMetric label="Validation loss" value={report.val_loss.toFixed(4)} />
                    <MiniMetric label="Perplexity" value={report.perplexity.toFixed(3)} />
                    <MiniMetric label="Top-1 next byte" value={percent(report.top1 * 100, 1)} />
                    <MiniMetric label="Top-5 next byte" value={percent(report.top5 * 100, 1)} />
                    <MiniMetric label="Parameters" value={params(report.params)} />
                    <MiniMetric label="Training windows" value={number(report.windows)} />
                    <MiniMetric label="Validation windows" value={number(report.val_windows)} />
                    <MiniMetric label="Duration" value={duration(report.duration_ms)} />
                  </div>

                  <div>
                    <div className="label mb-2">Loss per epoch · measured</div>
                    <LossChart curve={report.curve} />
                  </div>

                  <div className="grid gap-3 sm:grid-cols-2">
                    {report.samples.map((sample, index) => (
                      <div key={index} className="panel-flat p-3">
                        <div className="label mb-1">Sample {index + 1}</div>
                        <div className="mb-1 text-[11px] text-muted">prompt: {sample.prompt}</div>
                        <div className="whitespace-pre-wrap rounded-md border border-line bg-black/40 px-2 py-1.5 font-mono text-[11.5px] text-ink-dim">
                          {sample.output}
                        </div>
                      </div>
                    ))}
                  </div>

                  <div className="panel-flat p-3">
                    <div className="label mb-2">Generate from this model</div>
                    <div className="flex flex-col gap-2 sm:flex-row">
                      <TextInput
                        value={inferPrompt}
                        onChange={(e) => setInferPrompt(e.target.value)}
                        placeholder="Prompt prefix"
                        className="flex-1"
                      />
                      <Button busy={busy === "infer"} onClick={infer} icon={<Sparkles size={13} />}>
                        Generate
                      </Button>
                    </div>
                    {inferOut && (
                      <>
                        <div className="mt-2 whitespace-pre-wrap rounded-md border border-line bg-black/50 px-2.5 py-2 font-mono text-[11.5px] text-ink-dim">
                          {inferOut.output}
                        </div>
                        <div className="mt-1.5 text-[10.5px] text-muted">
                          {inferOut.tokens} bytes in {duration(inferOut.total_ms)} ·{" "}
                          {inferOut.bytes_per_second.toFixed(0)} B/s · {inferOut.format}
                        </div>
                      </>
                    )}
                  </div>
                </div>
              )}
            </Panel>
          )}

          {step === 6 && (
            <Panel
              title="Step 6 · Evaluate the model"
              subtitle="Held-out validation windows from the same deterministic split."
              icon={<Search size={14} />}
              actions={
                <Button size="sm" busy={busy === "evaluate"} onClick={evaluate} disabled={!report} icon={<Play size={12} />}>
                  Re-evaluate
                </Button>
              }
            >
              {project.evaluation ? (
                <div className="space-y-3">
                  <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                    <MiniMetric label="Validation loss" value={project.evaluation.loss.toFixed(4)} />
                    <MiniMetric label="Perplexity" value={project.evaluation.perplexity.toFixed(3)} />
                    <MiniMetric label="Top-1" value={percent(project.evaluation.top1 * 100, 1)} />
                    <MiniMetric label="Top-5" value={percent(project.evaluation.top5 * 100, 1)} />
                  </div>
                  <p className="text-[11.5px] leading-relaxed text-muted">{project.evaluation.note}</p>
                  <div className="grid gap-3 sm:grid-cols-2">
                    {project.evaluation.samples.map((sample, index) => (
                      <div key={index} className="panel-flat p-3">
                        <div className="label mb-1">Continuation {index + 1}</div>
                        <div className="whitespace-pre-wrap rounded-md border border-line bg-black/40 px-2 py-1.5 font-mono text-[11.5px] text-ink-dim">
                          {sample.output}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              ) : (
                <EmptyState
                  compact
                  icon={<Search size={16} />}
                  title="No evaluation yet"
                  description="Training writes an evaluation automatically; you can also re-run it against the held-out split."
                />
              )}
            </Panel>
          )}

          {step === 7 && (
            <Panel
              title="Step 7 · Quantize and optimize"
              subtitle="Symmetric per-row quantization. The loss delta is measured by dequantizing and re-running validation."
              icon={<Gauge size={14} />}
              actions={quant ? <FormatBadge format={quant.format} /> : <Badge tone="warn">not quantized</Badge>}
            >
              <div className="grid gap-2.5 sm:grid-cols-2 lg:grid-cols-4">
                {QUANT_FORMATS.map((format) => {
                  const selected = quantFormat === format.id;
                  return (
                    <button
                      key={format.id}
                      onClick={() => setQuantFormat(format.id)}
                      className={cx(
                        "rounded-xl border px-3 py-2.5 text-left transition-colors",
                        selected ? "border-accent/45 bg-accent/[0.07]" : "border-line bg-white/[0.015] hover:border-line-strong",
                      )}
                    >
                      <div className="flex items-center justify-between">
                        <span className="text-[12.5px] font-medium text-ink">{format.label}</span>
                        <span className="mono-num text-[10.5px] text-muted">{format.bits} bit</span>
                      </div>
                      <div className="mono-num mt-1 text-[10.5px] text-muted">
                        {report ? bytes(Math.round(report.params * (format.bits / 8))) : "—"}
                      </div>
                      <div className="mt-1 text-[10.5px] leading-snug text-muted">{format.note}</div>
                    </button>
                  );
                })}
              </div>
              <div className="mt-3 flex items-center gap-2">
                <Button
                  variant="primary"
                  busy={busy === "quantize"}
                  disabled={!report}
                  onClick={quantize}
                  icon={<Gauge size={13} />}
                >
                  Quantize to {quantFormat}
                </Button>
                {!report && <span className="text-[11px] text-muted">Train a model first.</span>}
              </div>

              {quant && (
                <div className="mt-4 grid gap-4 lg:grid-cols-2">
                  <div className="panel-flat p-3">
                    <div className="label mb-2">Measured result</div>
                    <KV label="Format" value={quant.format} mono />
                    <KV label="Weight bytes" value={bytes(quant.report.weight_bytes)} mono />
                    <KV label="Float32 reference" value={bytes(quant.report.f32_bytes)} mono />
                    <KV label="Compression" value={`${quant.report.ratio.toFixed(2)}×`} mono />
                    <KV label="Bits per weight" value={quant.report.bits_per_weight.toFixed(2)} mono />
                    <KV label="Validation loss" value={quant.report.val_loss.toFixed(4)} mono />
                    <KV
                      label="Loss delta"
                      value={`${quant.report.delta_loss >= 0 ? "+" : ""}${quant.report.delta_loss.toFixed(4)}`}
                      mono
                    />
                    <KV label="Perplexity" value={quant.report.perplexity.toFixed(3)} mono />
                    <KV label="Top-1" value={percent(quant.report.top1 * 100, 1)} mono />
                  </div>
                  <div className="panel-flat p-3">
                    <div className="label mb-2">What this means</div>
                    <p className="text-[11.5px] leading-relaxed text-muted">{quant.report.note}</p>
                    <div className="mt-2 text-[10.5px] text-muted">
                      Artifact: <span className="font-mono">{quant.path}</span>
                    </div>
                    <div className="mt-3 flex items-center justify-between">
                      <span className="text-[11px] text-muted">Measured in {duration(quant.report.duration_ms)}</span>
                      <Badge tone={quant.report.ratio > 1 ? "ok" : "neutral"}>
                        {quant.report.ratio.toFixed(2)}× smaller than f32
                      </Badge>
                    </div>
                  </div>
                </div>
              )}
            </Panel>
          )}

          {step === 8 && (
            <Panel
              title="Step 8 · RAM and flash requirements"
              subtitle="Weights are counted at their quantized size; the plan adds explicit activation and runtime budgets."
              icon={<MemoryStick size={14} />}
              actions={
                <Button size="sm" busy={busy === "memory"} disabled={!quant} onClick={() => analyseMemory()} icon={<Play size={12} />}>
                  Analyse
                </Button>
              }
            >
              {project.memory ? (
                <div className="space-y-4">
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge tone={project.memory.verdict === "fits" ? "ok" : project.memory.verdict === "fits with PSRAM" ? "warn" : "bad"}>
                      {project.memory.verdict}
                    </Badge>
                    <span className="text-[12px] text-ink-dim">{project.memory.target_name}</span>
                    <FormatBadge format={project.memory.quant_format} />
                    <span className="text-[11px] text-muted">estimate · datasheet limits</span>
                  </div>

                  <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                    <MiniMetric label="Weights" value={bytes(project.memory.weight_bytes)} />
                    <MiniMetric label="Activations" value={bytes(project.memory.activation_bytes)} />
                    <MiniMetric label="Runtime heap" value={bytes(project.memory.runtime_ram_bytes)} />
                    <MiniMetric label="Firmware image" value={bytes(project.memory.firmware_bytes)} />
                  </div>

                  <div className="grid gap-4 sm:grid-cols-2">
                    <div>
                      <div className="mb-1 flex items-baseline justify-between text-[11.5px]">
                        <span className="text-muted">RAM budget</span>
                        <span className="mono-num text-ink-dim">
                          {bytes(project.memory.total_ram_bytes)} / {bytes(project.memory.sram_bytes)}
                        </span>
                      </div>
                      <Progress
                        value={project.memory.ram_used_percent / 100}
                        tone={project.memory.ram_used_percent > 100 ? "bad" : project.memory.ram_used_percent > 80 ? "warn" : "accent"}
                      />
                      <div className="mt-1 text-[10.5px] text-muted">{percent(project.memory.ram_used_percent, 1)} of internal SRAM</div>
                    </div>
                    <div>
                      <div className="mb-1 flex items-baseline justify-between text-[11.5px]">
                        <span className="text-muted">Flash budget</span>
                        <span className="mono-num text-ink-dim">
                          {bytes(project.memory.total_flash_bytes)} / {bytes(project.memory.flash_bytes)}
                        </span>
                      </div>
                      <Progress
                        value={project.memory.flash_used_percent / 100}
                        tone={project.memory.flash_used_percent > 100 ? "bad" : project.memory.flash_used_percent > 85 ? "warn" : "ok"}
                      />
                      <div className="mt-1 text-[10.5px] text-muted">{percent(project.memory.flash_used_percent, 1)} of flash</div>
                    </div>
                  </div>

                  <p className="rounded-lg border border-line bg-black/25 px-3 py-2 text-[11px] leading-relaxed text-muted">
                    {project.memory.formula}
                  </p>
                  <div className="flex items-center justify-between">
                    <span className="text-[11px] text-muted">Detailed per-target comparison lives in the Hardware Lab.</span>
                    <Button size="sm" onClick={() => navigate("/hardware")} icon={<Cpu size={12} />}>
                      Open Hardware Lab
                    </Button>
                  </div>
                </div>
              ) : (
                <EmptyState
                  compact
                  icon={<MemoryStick size={16} />}
                  title={quant ? "No memory plan yet" : "Quantize the model first"}
                  description="The plan uses the real quantized weight bytes plus documented activation and runtime budgets."
                  action={
                    quant ? (
                      <Button size="sm" onClick={() => analyseMemory()} busy={busy === "memory"}>
                        Analyse now
                      </Button>
                    ) : undefined
                  }
                />
              )}
            </Panel>
          )}

          {step === 9 && (
            <Panel
              title="Step 9 · Export the deployment artifact"
              subtitle="Packed weights, portable C implementation, generated model code, manifest with SHA-256, and a host harness."
              icon={<FileCode2 size={14} />}
              actions={
                <Button size="sm" variant="primary" busy={busy === "export"} disabled={!quant} onClick={runExport} icon={<Download size={12} />}>
                  Export bundle
                </Button>
              }
            >
              {exportReport ? (
                <div className="space-y-3">
                  <div className="flex flex-wrap items-center gap-2 text-[11.5px] text-muted">
                    <Badge tone="ok">exported</Badge>
                    <span className="font-mono text-[10.5px]">{exportReport.dir}</span>
                    <span>· {bytes(exportReport.total_bytes)}</span>
                  </div>
                  <div className="overflow-x-auto">
                    <table className="w-full text-[12px]">
                      <thead>
                        <tr className="border-b border-line">
                          <th className="table-head px-2 py-2 text-left">File</th>
                          <th className="table-head px-2 py-2 text-right">Bytes</th>
                          <th className="table-head px-2 py-2 text-left">SHA-256</th>
                          <th className="table-head px-2 py-2 text-right">Download</th>
                        </tr>
                      </thead>
                      <tbody>
                        {exportReport.files.map((file) => (
                          <tr key={file.name} className="border-b border-line/60">
                            <td className="px-2 py-1.5 font-mono text-[11px] text-ink-dim">{file.name}</td>
                            <td className="mono-num px-2 py-1.5 text-right text-muted">{file.bytes}</td>
                            <td className="px-2 py-1.5 font-mono text-[10px] text-muted">
                              {file.sha256.slice(0, 10)}…{file.sha256.slice(-6)}
                            </td>
                            <td className="px-2 py-1.5 text-right">
                              <Button
                                size="sm"
                                onClick={() =>
                                  window.open(`/api/micro/projects/${project.id}/files/${file.name}`, "_blank")
                                }
                              >
                                save
                              </Button>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                  {exportReport.compile_check && (
                    <div
                      className={cx(
                        "rounded-lg border px-3 py-2 text-[11.5px] leading-relaxed",
                        exportReport.compile_check.startsWith("compiled")
                          ? "border-ok/25 bg-ok/[0.05] text-ink-dim"
                          : "border-warn/25 bg-warn/[0.05] text-ink-dim",
                      )}
                    >
                      <span className="font-medium text-ink">C verification: </span>
                      <span className="font-mono text-[10.5px]">{exportReport.compile_check}</span>
                    </div>
                  )}
                </div>
              ) : (
                <EmptyState
                  compact
                  icon={<FileCode2 size={16} />}
                  title={quant ? "Nothing exported yet" : "Quantize before exporting"}
                  description="The bundle is written under the Micro exports directory. Source models and datasets are never modified."
                />
              )}
            </Panel>
          )}

          {step === 10 && (
            <Panel
              title="Step 10 · Build and deploy firmware"
              subtitle="A real PlatformIO project is generated from the exported model and built with `pio run`."
              icon={<Rocket size={14} />}
              actions={
                <>
                  <Button
                    size="sm"
                    busy={busy === "build"}
                    disabled={!exportReport || !target?.platformio_board}
                    onClick={() => buildFirmware(false)}
                    icon={<Play size={12} />}
                  >
                    Build
                  </Button>
                  <Button
                    size="sm"
                    variant="primary"
                    busy={busy === "deploy"}
                    disabled={!firmware?.built}
                    onClick={() => buildFirmware(true)}
                    icon={<Rocket size={12} />}
                  >
                    Flash board
                  </Button>
                </>
              }
            >
              <CapabilityNote probe={data?.capabilities.platformio} />
              {target && (
                <div className="mt-3 grid gap-3 sm:grid-cols-2">
                  <div className="panel-flat p-3">
                    <div className="label mb-2">Generated project</div>
                    <KV label="Board" value={target.platformio_board ?? "—"} mono />
                    <KV label="Platform" value={target.toolchain} />
                    <KV label="Framework" value="Arduino" />
                    <KV label="Serial" value="115200 baud · prompt or `bench`" />
                  </div>
                  <div className="panel-flat p-3">
                    <div className="label mb-2">Status</div>
                    <KV label="Export" value={exportReport ? "ready" : "missing"} />
                    <KV label="Build" value={firmware?.built ? "succeeded" : firmware?.error ? "failed" : "not built"} />
                    <KV label="Artifact" value={firmware?.artifact_path ? "written" : "—"} />
                    <KV label="Flashed" value={firmware?.deployed ? dateTime(firmware.deployed_at) : "no"} />
                  </div>
                </div>
              )}
              {firmware?.artifact_path && (
                <div className="mt-3 truncate font-mono text-[10.5px] text-muted">{firmware.artifact_path}</div>
              )}
              {firmware?.log_tail && firmware.log_tail.length > 0 && (
                <div className="mt-3 h-40 overflow-y-auto rounded-lg border border-line bg-black/50 px-3 py-2 font-mono text-[11px] leading-relaxed text-[#b9c3d3]">
                  {firmware.log_tail.map((line, index) => (
                    <div key={index} className="whitespace-pre-wrap break-words">
                      {line}
                    </div>
                  ))}
                </div>
              )}
              {firmware?.error && (
                <div className="mt-3">
                  <Banner tone="bad" title="Firmware step failed">
                    {firmware.error}
                  </Banner>
                </div>
              )}
            </Panel>
          )}

          {step === 11 && (
            <Panel
              title="Step 11 · Collect hardware benchmark results"
              subtitle="Only numbers measured by the device are accepted; zero or missing measurements are rejected."
              icon={<HardDrive size={14} />}
              actions={<Badge tone={benchmarks.length ? "ok" : "neutral"}>{benchmarks.length} runs</Badge>}
            >
              <div className="flex flex-col gap-2 sm:flex-row">
                <TextInput
                  value={benchPath}
                  onChange={(e) => setBenchPath(e.target.value)}
                  placeholder="~/downloads/esp32s3-bench.json"
                  className="flex-1"
                />
                <Button variant="primary" busy={busy === "bench"} onClick={importBench} icon={<Upload size={13} />}>
                  Import report
                </Button>
              </div>
              <p className="mt-2 text-[11px] leading-relaxed text-muted">
                The firmware's serial `bench` command prints a JSON report. Save that line to a file and import it here.
                The exported host harness (`./atom_model --bench 2000`) produces the same shape.
              </p>

              {benchmarks.length > 0 ? (
                <div className="mt-4 space-y-3">
                  <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                    <MiniMetric label="Device" value={benchmarks[0].device} />
                    <MiniMetric
                      label="Best throughput"
                      value={`${Math.max(...benchmarks.map((b) => b.tokens_per_second)).toFixed(2)} tok/s`}
                      tone="ok"
                    />
                    <MiniMetric
                      label="Mean latency"
                      value={`${(benchmarks.reduce((sum, b) => sum + b.latency_ms, 0) / benchmarks.length).toFixed(2)} ms`}
                    />
                    <MiniMetric
                      label="Peak RAM"
                      value={bytes(Math.max(...benchmarks.map((b) => b.ram_bytes)))}
                    />
                  </div>
                  <div className="overflow-x-auto">
                    <table className="w-full text-[12px]">
                      <thead>
                        <tr className="border-b border-line">
                          <th className="table-head px-2 py-2 text-left">Run</th>
                          <th className="table-head px-2 py-2 text-right">tok/s</th>
                          <th className="table-head px-2 py-2 text-right">Latency</th>
                          <th className="table-head px-2 py-2 text-right">RAM</th>
                          <th className="table-head px-2 py-2 text-right">Flash</th>
                          <th className="table-head px-2 py-2 text-left">Notes</th>
                        </tr>
                      </thead>
                      <tbody>
                        {benchmarks.map((run) => (
                          <tr key={run.id} className="border-b border-line/60">
                            <td className="px-2 py-1.5 text-ink-dim">{run.label}</td>
                            <td className="mono-num px-2 py-1.5 text-right">{run.tokens_per_second.toFixed(2)}</td>
                            <td className="mono-num px-2 py-1.5 text-right">{run.latency_ms.toFixed(2)} ms</td>
                            <td className="mono-num px-2 py-1.5 text-right">{bytes(run.ram_bytes)}</td>
                            <td className="mono-num px-2 py-1.5 text-right">{run.flash_bytes ? bytes(run.flash_bytes) : "—"}</td>
                            <td className="px-2 py-1.5 text-[11px] text-muted">{run.notes || "—"}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              ) : (
                <EmptyState
                  compact
                  icon={<HardDrive size={16} />}
                  title="No device measurements yet"
                  description="Build and flash the firmware, open the serial monitor and send `bench`, then import the JSON line."
                  action={
                    <Button size="sm" onClick={() => setStep(10)} icon={<Rocket size={12} />}>
                      Go to firmware
                    </Button>
                  }
                />
              )}

              <details className="mt-4 rounded-lg border border-line bg-black/25 px-3 py-2">
                <summary className="cursor-pointer text-[11.5px] text-ink-dim">Expected report shape</summary>
                <pre className="mt-2 overflow-x-auto font-mono text-[10.5px] leading-relaxed text-muted">
{`{
  "device": "ESP32-S3 DevKitC-1",
  "firmware": "atom-micro 0.1.0",
  "runs": [{
    "label": "ATOM_BITS=8 · forward",
    "tokens_per_second": 41.2,
    "latency_ms": 24.3,
    "ram_bytes": 118784,
    "flash_bytes": 2163456
  }]
}`}
                </pre>
              </details>
            </Panel>
          )}

          <div className="flex items-center justify-between gap-2">
            <Button onClick={() => go(step - 1)} disabled={step === 1} icon={<ArrowLeft size={13} />}>
              Previous
            </Button>
            <div className="text-[11px] text-muted">
              {MICRO_STEPS[step - 1]?.title} · {step} / 11
            </div>
            <Button variant="primary" onClick={() => go(step + 1)} disabled={step === 11} icon={<ArrowRight size={13} />}>
              Next step
            </Button>
          </div>
        </div>
      </div>

      <ConfirmDialog
        open={deleteOpen}
        onClose={() => setDeleteOpen(false)}
        onConfirm={remove}
        title="Delete this Micro project?"
        body={
          <>
            <p>
              This removes <span className="text-ink">{project.name}</span>, its trained weights, exports and firmware
              directory. The dataset source file you imported is not touched.
            </p>
            <p className="text-warn">This cannot be undone.</p>
          </>
        }
        confirmWord={project.name}
        confirmLabel="Delete project"
        danger
      />

      {loading && !project && (
        <Modal open onClose={() => undefined} title="Loading project">
          <Spinner />
        </Modal>
      )}
    </div>
  );
}

function MiniMetric({
  label,
  value,
  tone,
}: {
  label: string;
  value: string | number;
  tone?: "ok" | "warn" | "bad";
}) {
  return (
    <div className="panel-flat px-3 py-2">
      <div className="label">{label}</div>
      <div
        className={cx(
          "mono-num mt-1 truncate text-[15px] font-semibold",
          tone === "ok" ? "text-ok" : tone === "warn" ? "text-warn" : tone === "bad" ? "text-bad" : "text-ink",
        )}
      >
        {value}
      </div>
    </div>
  );
}
