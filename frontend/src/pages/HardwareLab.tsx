import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowRight, CircuitBoard, Cpu, MemoryStick, RefreshCw } from "lucide-react";
import { api } from "../lib/api";
import { bytes, percent } from "../lib/format";
import { useApi } from "../lib/hooks";
import type { MemoryPlan, MicroOverviewData, MicroTarget, ProjectSummary } from "../lib/types";
import {
  Badge,
  Banner,
  Button,
  EmptyState,
  PageHeader,
  Panel,
  Progress,
  Select,
  Stat,
  cx,
  toast,
} from "../components/ui";

export default function HardwareLab() {
  const navigate = useNavigate();
  const { data: overview } = useApi<MicroOverviewData>("/micro/overview", 15000);
  const { data: catalog } = useApi<{ targets: MicroTarget[] }>("/micro/catalog");
  const projects = overview?.projects ?? [];
  const targets = catalog?.targets ?? [];
  const [projectId, setProjectId] = useState<string | null>(null);
  const [plans, setPlans] = useState<Array<{ target: MicroTarget; plan: MemoryPlan }>>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!projectId && projects.length) {
      const quantized = projects.find((p) => p.quant_format);
      setProjectId((quantized ?? projects[0]).id);
    }
  }, [projects, projectId]);

  const project: ProjectSummary | undefined = useMemo(
    () => projects.find((p) => p.id === projectId),
    [projects, projectId],
  );

  const analyse = async () => {
    if (!projectId) return;
    setLoading(true);
    setError(null);
    try {
      const results: Array<{ target: MicroTarget; plan: MemoryPlan }> = [];
      for (const target of targets) {
        const result = await api<{ memory: MemoryPlan }>(`/micro/projects/${projectId}/memory`, {
          method: "POST",
          body: JSON.stringify({ target_id: target.id }),
        });
        results.push({ target, plan: result.memory });
      }
      setPlans(results);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    setPlans([]);
  }, [projectId]);

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="ATOM Micro"
        title="Hardware Lab"
        description="Compare the exported artifact against every supported board. RAM and flash budgets use datasheet limits plus explicit activation and runtime figures."
        actions={
          <>
            <Button onClick={() => void analyse()} busy={loading} icon={<RefreshCw size={13} />} disabled={!projectId}>
              Analyse all targets
            </Button>
            <Button variant="primary" onClick={() => navigate("/micro")} icon={<CircuitBoard size={13} />}>
              Micro Studio
            </Button>
          </>
        }
      />

      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)]">
        <Panel title="Artifact" subtitle="Only real quantized weights are measured; the plan is an estimate." icon={<Cpu size={14} />}>
          <div className="space-y-3">
            <label className="block">
              <span className="mb-1.5 block text-[12px] font-medium text-ink-dim">Project</span>
              <Select value={projectId ?? ""} onChange={(e) => setProjectId(e.target.value)}>
                <option value="" disabled>
                  {projects.length ? "Select a project" : "No Micro projects"}
                </option>
                {projects.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name} · {p.quant_format ?? "not quantized"}
                  </option>
                ))}
              </Select>
            </label>

            {project ? (
              <div className="grid grid-cols-2 gap-2">
                <Stat label="Target" value={project.target_id} mono={false} />
                <Stat label="Parameters" value={project.params?.toLocaleString() ?? "—"} />
                <Stat label="Quant" value={project.quant_format ?? "—"} mono={false} />
                <Stat label="Val ppl" value={project.val_perplexity?.toFixed(2) ?? "—"} />
              </div>
            ) : (
              <EmptyState compact title="No project selected" />
            )}

            {project && !project.quant_format && (
              <Banner tone="warn" title="Analyse needs a quantized artifact">
                Run step 7 in Micro Studio to create real quantized weights; the plan uses their measured byte size.
              </Banner>
            )}

            {error && (
              <Banner tone="bad" title="Analysis failed">
                {error}
              </Banner>
            )}

            <div className="rounded-lg border border-line bg-black/25 p-3 text-[11px] leading-relaxed text-muted">
              <div className="label mb-1.5">Budget model</div>
              RAM = activation buffers (hidden vector, logits, context window ×2) + the runtime heap for that board.
              Flash = firmware image + quantized weights + 16 KiB metadata. Weights are treated as flash-resident and
              streamed at inference.
              <div className="mt-2 text-warn">Estimates, not device measurements.</div>
            </div>
          </div>
        </Panel>

        <Panel
          title="Target comparison"
          subtitle="Datasheet memory limits. Red means the estimated budget does not fit."
          icon={<MemoryStick size={14} />}
          actions={<Badge>{plans.length} analysed</Badge>}
        >
          {plans.length === 0 ? (
            <EmptyState
              icon={<CircuitBoard size={17} />}
              title="No analysis yet"
              description={
                project?.quant_format
                  ? "Run the analysis to compute the estimated RAM and flash budget for every supported board."
                  : "Quantize a model in Micro Studio first, then analyse it against the catalog."
              }
              action={
                <Button
                  size="sm"
                  variant="primary"
                  busy={loading}
                  disabled={!project?.quant_format}
                  onClick={() => void analyse()}
                >
                  Analyse artifact
                </Button>
              }
            />
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-[12px]">
                <thead>
                  <tr className="border-b border-line">
                    <th className="table-head px-2 py-2 text-left">Board</th>
                    <th className="table-head px-2 py-2 text-right">RAM</th>
                    <th className="table-head px-2 py-2 text-right">Flash</th>
                    <th className="table-head px-2 py-2 text-left">Verdict</th>
                    <th className="table-head px-2 py-2 text-right">Set target</th>
                  </tr>
                </thead>
                <tbody>
                  {plans.map(({ target, plan }) => (
                    <tr key={target.id} className="border-b border-line/60">
                      <td className="px-2 py-2">
                        <div className="text-[12px] text-ink-dim">{target.name}</div>
                        <div className="text-[10.5px] text-muted">
                          {target.mcu} · {target.cpu_mhz} MHz · {target.fp32_fpu ? "FPU" : "no FPU"}
                        </div>
                      </td>
                      <td className="px-2 py-2 text-right">
                        <div className="mono-num whitespace-nowrap text-[11px] text-ink-dim">
                          {bytes(plan.total_ram_bytes)} / {bytes(plan.sram_bytes)}
                        </div>
                        <div className="mt-1 w-28">
                          <Progress
                            value={plan.ram_used_percent / 100}
                            height={4}
                            tone={plan.ram_used_percent > 100 ? "bad" : plan.ram_used_percent > 80 ? "warn" : "accent"}
                          />
                        </div>
                      </td>
                      <td className="px-2 py-2 text-right">
                        <div className="mono-num whitespace-nowrap text-[11px] text-ink-dim">
                          {bytes(plan.total_flash_bytes)} / {bytes(plan.flash_bytes)}
                        </div>
                        <div className="mt-1 w-28">
                          <Progress
                            value={plan.flash_used_percent / 100}
                            height={4}
                            tone={plan.flash_used_percent > 100 ? "bad" : plan.flash_used_percent > 85 ? "warn" : "ok"}
                          />
                        </div>
                      </td>
                      <td className="px-2 py-2">
                        <span
                          className={cx(
                            "text-[11px]",
                            plan.verdict === "fits"
                              ? "text-ok"
                              : plan.verdict === "fits with PSRAM"
                                ? "text-warn"
                                : plan.verdict === "reference target"
                                  ? "text-muted"
                                  : "text-bad",
                          )}
                        >
                          {plan.verdict}
                        </span>
                        <div className="text-[10px] text-muted">{percent(plan.ram_used_percent, 0)} RAM</div>
                      </td>
                      <td className="px-2 py-2 text-right">
                        <Button
                          size="sm"
                          disabled={target.id === project?.target_id}
                          onClick={async () => {
                            try {
                              await api(`/micro/projects/${projectId}`, {
                                method: "PATCH",
                                body: JSON.stringify({ target_id: target.id }),
                              });
                              toast(`Target set to ${target.name}`, "ok");
                            } catch (e) {
                              toast((e as Error).message, "error");
                            }
                          }}
                        >
                          {target.id === project?.target_id ? "current" : "use"}
                        </Button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Panel>
      </div>

      <Panel className="mt-4" title="Supported boards" subtitle="Every figure is from the part's datasheet." icon={<CircuitBoard size={14} />}>
        <div className="overflow-x-auto">
          <table className="w-full text-[12px]">
            <thead>
              <tr className="border-b border-line">
                <th className="table-head px-2 py-2 text-left">Board</th>
                <th className="table-head px-2 py-2 text-left">MCU</th>
                <th className="table-head px-2 py-2 text-right">SRAM</th>
                <th className="table-head px-2 py-2 text-right">Flash</th>
                <th className="table-head px-2 py-2 text-right">PSRAM</th>
                <th className="table-head px-2 py-2 text-left">Toolchain</th>
                <th className="table-head px-2 py-2 text-left">Notes</th>
              </tr>
            </thead>
            <tbody>
              {targets.map((target) => (
                <tr key={target.id} className="border-b border-line/60">
                  <td className="px-2 py-1.5 text-ink-dim">{target.name}</td>
                  <td className="px-2 py-1.5 text-[11px] text-muted">
                    {target.mcu} · {target.cores}c @ {target.cpu_mhz} MHz
                  </td>
                  <td className="mono-num px-2 py-1.5 text-right">{bytes(target.sram_bytes)}</td>
                  <td className="mono-num px-2 py-1.5 text-right">{bytes(target.flash_bytes)}</td>
                  <td className="mono-num px-2 py-1.5 text-right">{target.psram_bytes ? bytes(target.psram_bytes) : "—"}</td>
                  <td className="px-2 py-1.5 text-[11px] text-muted">{target.toolchain}</td>
                  <td className="px-2 py-1.5 text-[11px] text-muted">{target.notes}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="mt-3 flex items-center justify-between gap-2">
          <span className="text-[11px] text-muted">
            Board selection changes the target used by Export and Firmware generation.
          </span>
          <Button size="sm" onClick={() => navigate("/deployment")} icon={<ArrowRight size={12} />}>
            Go to deployment
          </Button>
        </div>
      </Panel>
    </div>
  );
}
