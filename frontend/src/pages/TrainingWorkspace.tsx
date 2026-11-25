import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowRight, GraduationCap, Layers, Play, Sparkles, Wand2 } from "lucide-react";
import { api } from "../lib/api";
import { dateTime, duration, number, params, percent } from "../lib/format";
import { useApi } from "../lib/hooks";
import type {
  Config,
  MicroArchitecture,
  MicroOverviewData,
  ProjectSummary,
  TrainReport,
} from "../lib/types";
import { JobConsole } from "../components/JobConsole";
import { LossChart } from "../components/Charts";
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
  toast,
} from "../components/ui";
import { useMicroProject } from "../components/MicroBits";

export default function TrainingWorkspace() {
  const navigate = useNavigate();
  const { data: overview, reload: reloadOverview } = useApi<MicroOverviewData>("/micro/overview", 10000);
  const { data: catalog } = useApi<{ architectures: MicroArchitecture[] }>("/micro/catalog");
  useApi<Config>("/settings");
  const projects = overview?.projects ?? [];
  const [projectId, setProjectId] = useState<string | null>(null);
  const { data: projectData, reload: reloadProject } = useMicroProject(projectId);
  const project = projectData?.project;
  const architectures = catalog?.architectures ?? [];

  const [archId, setArchId] = useState("atom-mlp-small");
  const [hyper, setHyper] = useState({
    epochs: 12,
    learning_rate: 0.04,
    batch: 16,
    seed: 1337,
    windows_per_epoch: 10000,
  });
  const [jobId, setJobId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [prompt, setPrompt] = useState("LED");
  const [output, setOutput] = useState<string | null>(null);

  useEffect(() => {
    if (!projectId && projects.length) setProjectId(projects[0].id);
  }, [projects, projectId]);

  useEffect(() => {
    if (project?.architecture) {
      setArchId(project.architecture.spec_id);
      setHyper({
        epochs: project.architecture.epochs,
        learning_rate: project.architecture.learning_rate,
        batch: project.architecture.batch,
        seed: project.architecture.seed,
        windows_per_epoch: project.architecture.windows_per_epoch,
      });
    }
  }, [project?.architecture?.spec_id]);

  const spec = useMemo(() => architectures.find((a) => a.id === archId), [architectures, archId]);
  const trained = projects.filter((p) => p.params !== null);

  const start = async () => {
    if (!projectId) return;
    setBusy(true);
    try {
      const result = await api<{ job_id: string }>(`/micro/projects/${projectId}/train`, {
        method: "POST",
        body: JSON.stringify({ spec_id: archId, ...hyper }),
      });
      setJobId(result.job_id);
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(false);
    }
  };

  const generate = async () => {
    if (!projectId) return;
    setBusy(true);
    try {
      const result = await api<{ output: string; total_ms: number }>(`/micro/projects/${projectId}/infer`, {
        method: "POST",
        body: JSON.stringify({ prompt, max_tokens: 180, temperature: 0.7 }),
      });
      setOutput(`${result.output}\n\n— ${duration(result.total_ms)}`);
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(false);
    }
  };

  const report: TrainReport | undefined = project?.training?.report;

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="ATOM Micro"
        title="Model Training Workspace"
        description="Train a real byte-level language model on a project dataset, watch measured epochs, and run it right after training finishes."
        actions={
          <>
            <Button onClick={() => navigate("/micro")} icon={<Layers size={13} />}>
              Micro Studio
            </Button>
            <Button variant="primary" onClick={() => navigate("/micro?new=1")} icon={<Wand2 size={13} />}>
              New project
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat label="Projects" value={projects.length} icon={<Layers size={13} />} />
        <Stat label="Trained models" value={trained.length} tone={trained.length ? "ok" : "default"} icon={<GraduationCap size={13} />} />
        <Stat
          label="Best perplexity"
          value={
            trained.length
              ? Math.min(...trained.map((p) => p.val_perplexity ?? Infinity)).toFixed(2)
              : "—"
          }
          sub="held-out validation"
          icon={<Sparkles size={13} />}
        />
        <Stat
          label="Dataset documents"
          value={number(projects.reduce((sum, p) => sum + (p.dataset_documents ?? 0), 0))}
          sub="across all Micro projects"
        />
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.35fr)]">
        <Panel title="Training configuration" subtitle="Only built-in MLP architectures run here." icon={<GraduationCap size={14} />}>
          <div className="space-y-3">
            <Field label="Project">
              <Select value={projectId ?? ""} onChange={(e) => setProjectId(e.target.value)}>
                <option value="" disabled>
                  {projects.length ? "Select a project" : "No Micro projects yet"}
                </option>
                {projects.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name} · step {p.next_step}/11
                  </option>
                ))}
              </Select>
            </Field>

            {project ? (
              <>
                <div className="rounded-lg border border-line bg-black/25 px-3 py-2">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[12px] text-ink-dim">Dataset</span>
                    <Badge tone={project.dataset ? "ok" : "warn"}>{project.dataset ? "ready" : "missing"}</Badge>
                  </div>
                  {project.dataset ? (
                    <div className="mt-1 space-y-1 text-[11px] text-muted">
                      <div>
                        {number(project.dataset.stats.documents)} documents · {number(project.dataset.stats.chars)} chars
                      </div>
                      <div className="mono-num">
                        train {number(project.dataset.stats.train_chars)} · val {number(project.dataset.stats.val_chars)}
                      </div>
                    </div>
                  ) : (
                    <button onClick={() => navigate(`/micro/${project.id}`)} className="mt-1 text-[11px] text-accent">
                      Import a dataset in step 3 →
                    </button>
                  )}
                </div>

                <Field label="Architecture" hint={spec ? `${params(spec.parameters)} params` : undefined}>
                  <Select value={archId} onChange={(e) => setArchId(e.target.value)}>
                    {architectures.map((arch) => (
                      <option key={arch.id} value={arch.id} disabled={!arch.trainable_here}>
                        {arch.name} {arch.trainable_here ? "" : "· needs PyTorch"}
                      </option>
                    ))}
                  </Select>
                </Field>
                {spec && <p className="text-[11px] leading-relaxed text-muted">{spec.description}</p>}

                <div className="grid grid-cols-2 gap-2">
                  <Field label="Epochs">
                    <TextInput type="number" value={hyper.epochs} onChange={(e) => setHyper({ ...hyper, epochs: Number(e.target.value) })} />
                  </Field>
                  <Field label="Learning rate">
                    <TextInput
                      type="number"
                      step="0.005"
                      value={Number(hyper.learning_rate.toPrecision(4))}
                      onChange={(e) => setHyper({ ...hyper, learning_rate: Number(e.target.value) })}
                    />
                  </Field>
                  <Field label="Batch">
                    <TextInput type="number" value={hyper.batch} onChange={(e) => setHyper({ ...hyper, batch: Number(e.target.value) })} />
                  </Field>
                  <Field label="Seed">
                    <TextInput type="number" value={hyper.seed} onChange={(e) => setHyper({ ...hyper, seed: Number(e.target.value) })} />
                  </Field>
                  <Field label="Windows / epoch" className="col-span-2">
                    <TextInput
                      type="number"
                      value={hyper.windows_per_epoch}
                      onChange={(e) => setHyper({ ...hyper, windows_per_epoch: Number(e.target.value) })}
                    />
                  </Field>
                </div>

                <Button
                  variant="primary"
                  className="w-full"
                  busy={busy}
                  disabled={!project.dataset || !spec?.trainable_here}
                  onClick={start}
                  icon={<Play size={13} />}
                >
                  Start training run
                </Button>
              </>
            ) : (
              <EmptyState
                compact
                icon={<Layers size={16} />}
                title="No project selected"
                description="Create a Micro project to give training a dataset and a target."
              />
            )}
          </div>
        </Panel>

        <div className="min-w-0 space-y-4">
          {jobId ? (
            <Panel title="Training run" subtitle="Progress comes from completed epochs and windows." icon={<Play size={14} />}>
              <JobConsole
                jobId={jobId}
                onFinished={(job) => {
                  void reloadProject();
                  void reloadOverview();
                  if (job.status === "succeeded") toast("Training finished and the model was saved", "ok");
                }}
              />
            </Panel>
          ) : (
            <Panel title="Training run" icon={<Play size={14} />}>
              <EmptyState
                compact
                icon={<GraduationCap size={16} />}
                title="No run started"
                description="Configure the model on the left and start a run. Every epoch reports its real training and validation loss."
              />
            </Panel>
          )}

          {report && (
            <Panel
              title="Latest measured run"
              subtitle={project?.training ? `saved ${dateTime(project.training.finished_at)}` : undefined}
              icon={<Sparkles size={14} />}
              actions={<Badge tone="ok">val ppl {report.perplexity.toFixed(2)}</Badge>}
            >
              <div className="grid grid-cols-2 gap-2">
                <MiniStat label="Train loss" value={report.train_loss.toFixed(4)} />
                <MiniStat label="Val loss" value={report.val_loss.toFixed(4)} />
                <MiniStat label="Top-1" value={percent(report.top1 * 100, 1)} />
                <MiniStat label="Parameters" value={params(report.params)} />
                <MiniStat label="Windows" value={number(report.windows)} />
                <MiniStat label="Val windows" value={number(report.val_windows)} />
                <MiniStat label="Epochs" value={report.epochs} />
                <MiniStat label="Duration" value={duration(report.duration_ms)} />
              </div>
              <div className="mt-3">
                <div className="label mb-1.5">Loss curve</div>
                <LossChart curve={report.curve} />
              </div>
              <div className="mt-3">
                <div className="label mb-1.5">Run inference</div>
                <div className="flex gap-2">
                  <TextInput value={prompt} onChange={(e) => setPrompt(e.target.value)} className="flex-1" />
                  <Button onClick={generate} busy={busy} icon={<Sparkles size={13} />}>
                    Generate
                  </Button>
                </div>
                {output && (
                  <pre className="mt-2 max-h-40 overflow-y-auto whitespace-pre-wrap rounded-md border border-line bg-black/50 px-2.5 py-2 font-mono text-[11px] text-ink-dim">
                    {output}
                  </pre>
                )}
              </div>
              <div className="mt-3 flex items-center justify-between">
                <span className="text-[11px] text-muted">Next: quantize and export in Micro Studio.</span>
                <Button size="sm" onClick={() => project && navigate(`/micro/${project.id}`)} icon={<ArrowRight size={12} />}>
                  Open step 7
                </Button>
              </div>
              <div className="mt-2">
                <KV label="Model file" value={project?.training?.model_path ?? "—"} mono />
              </div>
            </Panel>
          )}
        </div>
      </div>

      <Panel className="mt-4" title="Training history" subtitle="Every trained project in this workspace." icon={<GraduationCap size={14} />}>
        {trained.length === 0 ? (
          <EmptyState compact title="No trained models" description="Finished runs appear here with their measured validation perplexity." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-[12px]">
              <thead>
                <tr className="border-b border-line">
                  <th className="table-head px-2 py-2 text-left">Project</th>
                  <th className="table-head px-2 py-2 text-left">Target</th>
                  <th className="table-head px-2 py-2 text-right">Params</th>
                  <th className="table-head px-2 py-2 text-right">Val ppl</th>
                  <th className="table-head px-2 py-2 text-right">Docs</th>
                  <th className="table-head px-2 py-2 text-right">Quant</th>
                  <th className="table-head px-2 py-2 text-right">Updated</th>
                </tr>
              </thead>
              <tbody>
                {trained.map((p) => (
                  <tr
                    key={p.id}
                    className="cursor-pointer border-b border-line/60 hover:bg-white/[0.02]"
                    onClick={() => navigate(`/micro/${p.id}`)}
                  >
                    <td className="px-2 py-1.5 text-ink-dim">{p.name}</td>
                    <td className="px-2 py-1.5 text-muted">{p.target_id}</td>
                    <td className="mono-num px-2 py-1.5 text-right">{params(p.params)}</td>
                    <td className="mono-num px-2 py-1.5 text-right">{p.val_perplexity?.toFixed(2) ?? "—"}</td>
                    <td className="mono-num px-2 py-1.5 text-right">{p.dataset_documents ?? "—"}</td>
                    <td className="px-2 py-1.5 text-right text-muted">{p.quant_format ?? "—"}</td>
                    <td className="px-2 py-1.5 text-right text-muted">{dateTime(p.updated_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>

      {projects.some((p) => p.params !== null) && (
        <div className="mt-3">
          <Banner tone="info" title="All numbers are measured">
            Training loss, validation loss, perplexity and next-byte accuracy come from this process. Hardware speed
            only exists after you run the firmware on a device.
          </Banner>
        </div>
      )}
      <div className="mt-3">
        <ProjectProgress projects={projects} />
      </div>
    </div>
  );
}

function MiniStat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="panel-flat px-2.5 py-2">
      <div className="label">{label}</div>
      <div className="mono-num mt-1 truncate text-[14px] font-semibold text-ink">{value}</div>
    </div>
  );
}

function ProjectProgress({ projects }: { projects: ProjectSummary[] }) {
  if (projects.length === 0) return null;
  return (
    <Panel title="Workflow progress" icon={<ArrowRight size={14} />}>
      <div className="space-y-2">
        {projects.map((p) => (
          <div key={p.id} className="flex items-center gap-3">
            <span className="w-40 shrink-0 truncate text-[11.5px] text-ink-dim">{p.name}</span>
            <div className="flex-1">
              <Progress value={p.completed_steps / 11} height={5} />
            </div>
            <span className="mono-num w-14 shrink-0 text-right text-[11px] text-muted">{p.completed_steps}/11</span>
            <Badge tone={p.completed_steps >= 9 ? "ok" : "neutral"}>step {p.next_step}</Badge>
          </div>
        ))}
      </div>
    </Panel>
  );
}
