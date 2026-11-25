import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowRight, FlaskConical, GraduationCap, Play, Sparkles, TriangleAlert } from "lucide-react";
import { api } from "../lib/api";
import { dateTime, duration, percent } from "../lib/format";
import { useApi } from "../lib/hooks";
import type { MicroArchitecture, MicroOverviewData, RuntimeStatus } from "../lib/types";
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
  Select,
  Stat,
  TextInput,
  toast,
} from "../components/ui";
import { useMicroProject } from "../components/MicroBits";

export default function DistillationLab() {
  const navigate = useNavigate();
  const { data: runtimeData } = useApi<{ runtime: RuntimeStatus }>("/runtime", 5000);
  const { data: overview, reload: reloadOverview } = useApi<MicroOverviewData>("/micro/overview", 10000);
  const { data: catalog } = useApi<{ architectures: MicroArchitecture[] }>("/micro/catalog");
  const runtime = runtimeData?.runtime;
  const projects = (overview?.projects ?? []).filter((p) => (p.dataset_documents ?? 0) > 0);
  const [projectId, setProjectId] = useState<string | null>(null);
  const { data: projectData, reload: reloadProject } = useMicroProject(projectId);
  const project = projectData?.project;
  const [archId, setArchId] = useState("atom-mlp-small");
  const [prompts, setPrompts] = useState(12);
  const [maxTokens, setMaxTokens] = useState(96);
  const [jobId, setJobId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!projectId && projects.length) setProjectId(projects[0].id);
  }, [projects, projectId]);

  const architectures = catalog?.architectures ?? [];
  const spec = architectures.find((a) => a.id === archId);
  const report = project?.distillation;

  const start = async () => {
    if (!projectId) return;
    setBusy(true);
    try {
      const result = await api<{ job_id: string }>(`/micro/projects/${projectId}/distill`, {
        method: "POST",
        body: JSON.stringify({ spec_id: archId, prompts, max_tokens: maxTokens }),
      });
      setJobId(result.job_id);
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="ATOM Micro"
        title="Distillation Lab"
        description="Use a loaded GGUF teacher to label your dataset, train a small student on those completions, and measure how often the student agrees with the teacher."
        actions={
          <Button onClick={() => navigate("/inference")} icon={<ArrowRight size={13} />}>
            Inference workspace
          </Button>
        }
      />

      {!runtime?.loaded ? (
        <Banner
          tone="warn"
          title="Distillation needs a teacher model"
          action={
            <Button size="sm" onClick={() => navigate("/inference")} icon={<ArrowRight size={12} />}>
              Load a model
            </Button>
          }
        >
          Load a GGUF model in the Inference workspace. ATOM calls it through llama-server for the labelling phase,
          then trains the student here. Without a teacher the pipeline is unavailable rather than simulated.
        </Banner>
      ) : (
        <div className="mb-4 flex flex-wrap items-center gap-2 rounded-xl border border-line bg-white/[0.02] px-3.5 py-2.5 text-[12px]">
          <Badge tone="ok">teacher loaded</Badge>
          <span className="text-ink-dim">{runtime.model_label}</span>
          <span className="font-mono text-[10.5px] text-muted">{runtime.base_url}</span>
        </div>
      )}

      <div className="mt-4 grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.35fr)]">
        <Panel title="Run configuration" subtitle="Student training runs in this process; teacher calls use llama-server." icon={<FlaskConical size={14} />}>
          <div className="space-y-3">
            <Field label="Project (dataset source)">
              <Select value={projectId ?? ""} onChange={(e) => setProjectId(e.target.value)}>
                <option value="" disabled>
                  {projects.length ? "Select a project" : "No project with a dataset"}
                </option>
                {projects.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name} · {p.dataset_documents} docs
                  </option>
                ))}
              </Select>
            </Field>

            <Field label="Student architecture">
              <Select value={archId} onChange={(e) => setArchId(e.target.value)}>
                {architectures
                  .filter((a) => a.trainable_here)
                  .map((arch) => (
                    <option key={arch.id} value={arch.id}>
                      {arch.name} · {arch.summary}
                    </option>
                  ))}
              </Select>
            </Field>

            <div className="grid grid-cols-2 gap-2">
              <Field label="Teacher prompts" hint="max 64">
                <TextInput type="number" value={prompts} onChange={(e) => setPrompts(Number(e.target.value))} />
              </Field>
              <Field label="Tokens per answer">
                <TextInput type="number" value={maxTokens} onChange={(e) => setMaxTokens(Number(e.target.value))} />
              </Field>
            </div>

            {spec && <p className="text-[11px] leading-relaxed text-muted">{spec.description}</p>}

            <Button
              variant="primary"
              className="w-full"
              busy={busy}
              disabled={!runtime?.loaded || !project}
              onClick={start}
              icon={<Play size={13} />}
            >
              Generate corpus and train student
            </Button>
            {!runtime?.loaded && <div className="text-[11px] text-warn">Load a teacher model first.</div>}
          </div>

          <div className="mt-4 rounded-lg border border-line bg-black/25 p-3">
            <div className="label mb-1.5">What this actually does</div>
            <ul className="space-y-1.5 text-[11px] leading-relaxed text-muted">
              <li>1. Takes prompt lines from the project's dataset (teacher never sees the held-out text).</li>
              <li>2. Calls the loaded teacher for each prompt and records its completions.</li>
              <li>3. Trains the student on prompt + completion text in this process.</li>
              <li>4. Measures teacher-forced next-byte agreement on the teacher corpus.</li>
            </ul>
            <p className="mt-2 text-[10.5px] text-warn">
              This is completion-level distillation, not logit distillation: it does not read the teacher's logits, and it
              does not claim to reproduce them.
            </p>
          </div>
        </Panel>

        <div className="space-y-4">
          {jobId ? (
            <Panel title="Distillation run" icon={<Play size={14} />}>
              <JobConsole
                jobId={jobId}
                onFinished={(job) => {
                  void reloadProject();
                  void reloadOverview();
                  if (job.status === "succeeded") toast("Distillation finished", "ok");
                }}
              />
            </Panel>
          ) : (
            <Panel title="Distillation run" icon={<Play size={14} />}>
              <EmptyState
                compact
                icon={<FlaskConical size={16} />}
                title="No distillation run yet"
                description="The run logs each teacher call, then the student's measured training curve."
              />
            </Panel>
          )}

          {report && (
            <Panel
              title="Result"
              subtitle={`${report.teacher_label} → ${project?.architecture?.name ?? "student"}`}
              icon={<Sparkles size={14} />}
              actions={<Badge tone="accent">agreement {percent(report.agreement * 100, 1)}</Badge>}
            >
              <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                <MiniStat label="Teacher prompts" value={report.prompts} />
                <MiniStat label="Teacher tokens" value={report.teacher_tokens} />
                <MiniStat label="Student perplexity" value={report.report.perplexity.toFixed(3)} />
                <MiniStat label="Agreement" value={percent(report.agreement * 100, 1)} />
                <MiniStat label="Corpus documents" value={report.dataset.stats.documents} />
                <MiniStat label="Corpus chars" value={report.dataset.stats.chars} />
                <MiniStat label="Student params" value={report.report.params.toLocaleString()} />
                <MiniStat label="Duration" value={duration(report.duration_ms)} />
              </div>
              <div className="mt-3">
                <div className="label mb-1.5">Student loss curve</div>
                <LossChart curve={report.report.curve} />
              </div>
              <div className="mt-3 grid gap-2 sm:grid-cols-2">
                {report.report.samples.map((sample, index) => (
                  <div key={index} className="panel-flat p-2.5">
                    <div className="label mb-1">Sample {index + 1}</div>
                    <div className="whitespace-pre-wrap rounded border border-line bg-black/40 px-2 py-1.5 font-mono text-[10.5px] text-ink-dim">
                      {sample.output}
                    </div>
                  </div>
                ))}
              </div>
              <div className="mt-3 space-y-1.5">
                <KV label="Teacher corpus" value={report.dataset.source_path} mono />
                <KV label="Student model" value={report.model_path} mono />
                <KV label="Finished" value={dateTime(report.created_at)} />
              </div>
              <p className="mt-2 text-[11px] leading-relaxed text-muted">{report.method}</p>
              <div className="mt-3 flex justify-end">
                <Button size="sm" onClick={() => project && navigate(`/micro/${project.id}`)} icon={<ArrowRight size={12} />}>
                  Continue in Micro Studio
                </Button>
              </div>
            </Panel>
          )}

          <Panel title="Method limits" icon={<TriangleAlert size={14} />}>
            <div className="space-y-2 text-[11.5px] leading-relaxed text-muted">
              <p>
                Agreement is a next-byte metric on the teacher corpus. It measures how closely the student's greedy
                prediction follows the teacher's text; it is not a general quality score.
              </p>
              <p>
                Student capacity is far smaller than the teacher. Distillation transfers style and local statistics, not
                the teacher's knowledge.
              </p>
              <p className="flex items-center gap-1.5 text-ink-dim">
                <GraduationCap size={12} /> Teacher calls are measured by the inference layer and shown in the job log.
              </p>
            </div>
          </Panel>
        </div>
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
