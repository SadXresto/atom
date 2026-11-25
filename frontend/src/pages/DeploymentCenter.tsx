import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  Download,
  HardDrive,
  Play,
  Rocket,
  Upload,
  FileCode2,
  CircuitBoard,
  Terminal,
} from "lucide-react";
import { api } from "../lib/api";
import { bytes, dateTime } from "../lib/format";
import { useApi } from "../lib/hooks";
import type { MicroOverviewData } from "../lib/types";
import { JobConsole } from "../components/JobConsole";
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
  TextInput,
  toast,
} from "../components/ui";
import { CapabilityGrid, FormatBadge, useMicroProject } from "../components/MicroBits";

export default function DeploymentCenter() {
  const navigate = useNavigate();
  const { data: overview, reload: reloadOverview } = useApi<MicroOverviewData>("/micro/overview", 10000);
  const projects = overview?.projects ?? [];
  const [projectId, setProjectId] = useState<string | null>(null);
  const { data: projectData, reload } = useMicroProject(projectId);
  const project = projectData?.project;
  const caps = projectData?.capabilities;
  const [jobId, setJobId] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [benchPath, setBenchPath] = useState("");

  useEffect(() => {
    if (!projectId && projects.length) setProjectId(projects[0].id);
  }, [projects, projectId]);

  const exportReport = project?.export;
  const firmware = project?.firmware;
  const benchmarks = project?.benchmarks ?? [];

  const runAction = async (key: string, action: () => Promise<unknown>, message: string) => {
    setBusy(key);
    try {
      await action();
      toast(message, "ok");
      void reload();
      void reloadOverview();
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(null);
    }
  };

  const startJob = async (path: string) => {
    setBusy(path);
    try {
      const result = await api<{ job_id: string }>(`/micro/projects/${projectId}/firmware/${path}`, {
        method: "POST",
      });
      setJobId(result.job_id);
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(null);
    }
  };

  const importBench = () =>
    runAction(
      "bench",
      async () => {
        await api(`/micro/projects/${projectId}/benchmarks`, {
          method: "POST",
          body: JSON.stringify({ path: benchPath }),
        });
        setBenchPath("");
      },
      "Benchmark report imported",
    );

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="ATOM Micro"
        title="Embedded Deployment Center"
        description="Export a checksummed artifact, compile firmware with PlatformIO, flash the board, and bring measured device results back into the project."
        actions={
          <>
            <Button onClick={() => navigate("/hardware")} icon={<CircuitBoard size={13} />}>
              Hardware Lab
            </Button>
            <Button variant="primary" onClick={() => navigate("/micro")} icon={<Rocket size={13} />}>
              Micro Studio
            </Button>
          </>
        }
      />

      <Panel className="mb-4" title="Project" icon={<Rocket size={14} />}>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-end">
          <div className="min-w-[240px] flex-1">
            <Field label="Deployment target project">
              <Select value={projectId ?? ""} onChange={(e) => setProjectId(e.target.value)}>
                <option value="" disabled>
                  {projects.length ? "Select a project" : "No Micro projects"}
                </option>
                {projects.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name} · {p.quant_format ?? "no quant"} · step {p.next_step}/11
                  </option>
                ))}
              </Select>
            </Field>
          </div>
          {project && (
            <div className="flex flex-wrap items-center gap-2 pb-1 text-[11.5px]">
              <Badge tone="neutral">{project.target_id}</Badge>
              {project.quantization && <FormatBadge format={project.quantization.format} />}
              <Badge tone={exportReport ? "ok" : "neutral"}>{exportReport ? "exported" : "not exported"}</Badge>
              <Badge tone={firmware?.built ? "ok" : "neutral"}>{firmware?.built ? "firmware built" : "no build"}</Badge>
              <Badge tone={firmware?.deployed ? "accent" : "neutral"}>{firmware?.deployed ? "flashed" : "not flashed"}</Badge>
            </div>
          )}
        </div>
      </Panel>

      {!project ? (
        <EmptyState
          icon={<Rocket size={18} />}
          title="No project selected"
          description="Create a Micro project and take it through export to deploy artifacts and firmware."
          action={
            <Button variant="primary" size="sm" onClick={() => navigate("/micro?new=1")}>
              New project
            </Button>
          }
        />
      ) : (
        <>
          {jobId && (
            <Panel
              className="mb-4"
              title="Firmware job"
              subtitle="PlatformIO output, streamed from the tool."
              icon={<Terminal size={14} />}
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
                  void reloadOverview();
                  if (job.status === "succeeded") toast(`${job.title} completed`, "ok");
                }}
              />
            </Panel>
          )}

          <div className="grid gap-4 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,1fr)]">
            <Panel
              title="Deployment artifacts"
              subtitle="Packed weights, C implementation, manifest with SHA-256, host harness."
              icon={<FileCode2 size={14} />}
              actions={
                <Button
                  size="sm"
                  variant="primary"
                  busy={busy === "export"}
                  disabled={!project.quantization}
                  onClick={() =>
                    runAction(
                      "export",
                      () => api(`/micro/projects/${project.id}/export`, { method: "POST" }),
                      "Artifacts exported",
                    )
                  }
                  icon={<Download size={12} />}
                >
                  {exportReport ? "Re-export" : "Export"}
                </Button>
              }
            >
              {exportReport ? (
                <div className="space-y-3">
                  <div className="grid gap-2 sm:grid-cols-3">
                    <KV label="Format" value={exportReport.quant_format} mono />
                    <KV label="Total size" value={bytes(exportReport.total_bytes)} mono />
                    <KV label="Exported" value={dateTime(exportReport.created_at)} />
                  </div>
                  <div className="space-y-1.5">
                    {exportReport.files.map((file) => (
                      <div
                        key={file.name}
                        className="flex items-center justify-between gap-2 rounded-lg border border-line bg-black/20 px-2.5 py-1.5"
                      >
                        <div className="min-w-0">
                          <div className="font-mono text-[11.5px] text-ink-dim">{file.name}</div>
                          <div className="truncate font-mono text-[10px] text-muted">{file.sha256}</div>
                        </div>
                        <div className="flex shrink-0 items-center gap-2">
                          <span className="mono-num text-[10.5px] text-muted">{bytes(file.bytes)}</span>
                          <Button
                            size="sm"
                            onClick={() => window.open(`/api/micro/projects/${project.id}/files/${file.name}`, "_blank")}
                          >
                            save
                          </Button>
                        </div>
                      </div>
                    ))}
                  </div>
                  {exportReport.compile_check && (
                    <Banner
                      tone={exportReport.compile_check.startsWith("compiled") ? "ok" : "warn"}
                      title="Host C verification"
                    >
                      <span className="font-mono text-[10.5px]">{exportReport.compile_check}</span>
                    </Banner>
                  )}
                  <div className="truncate font-mono text-[10px] text-muted">{exportReport.dir}</div>
                </div>
              ) : (
                <EmptyState
                  compact
                  icon={<FileCode2 size={16} />}
                  title={project.quantization ? "Nothing exported yet" : "Quantize the model first"}
                  description="Export writes the artifact bundle and verifies the generated C code with your compiler when one is available."
                  action={
                    project.quantization ? (
                      <Button
                        size="sm"
                        onClick={() =>
                          runAction(
                            "export",
                            () => api(`/micro/projects/${project.id}/export`, { method: "POST" }),
                            "Artifacts exported",
                          )
                        }
                      >
                        Export now
                      </Button>
                    ) : (
                      <Button size="sm" onClick={() => navigate(`/micro/${project.id}`)}>
                        Open step 7
                      </Button>
                    )
                  }
                />
              )}
            </Panel>

            <div className="space-y-4">
              <Panel
                title="Firmware"
                subtitle="Generated PlatformIO project for this board."
                icon={<Rocket size={14} />}
                actions={
                  <>
                    <Button
                      size="sm"
                      busy={busy === "build"}
                      disabled={!exportReport}
                      onClick={() => startJob("build")}
                      icon={<Play size={12} />}
                    >
                      Build
                    </Button>
                    <Button
                      size="sm"
                      variant="primary"
                      busy={busy === "deploy"}
                      disabled={!firmware?.built}
                      onClick={() => startJob("deploy")}
                      icon={<Rocket size={12} />}
                    >
                      Flash
                    </Button>
                  </>
                }
              >
                <CapabilityGrid caps={caps} />
                <div className="mt-3">
                  <Banner tone={caps?.platformio.available ? "ok" : "warn"} title={caps?.platformio.available ? "PlatformIO ready" : "PlatformIO not found"}>
                    {caps?.platformio.available
                      ? "Builds and flashing run `pio run` with explicit arguments in the generated project."
                      : "Install PlatformIO with `pip install platformio`, then retry. Export and checksum verification work without it."}
                  </Banner>
                </div>
                {firmware && (
                  <div className="mt-3 space-y-1.5">
                    <KV label="Board" value={firmware.board || "—"} mono />
                    <KV label="Build tool" value={firmware.build_tool} mono />
                    <KV label="Artifact" value={firmware.artifact_path ?? "none"} mono />
                    <KV label="Built" value={firmware.built ? "yes" : "no"} />
                    <KV label="Flashed" value={firmware.deployed ? dateTime(firmware.deployed_at) : "no"} />
                    <KV label="Project dir" value={firmware.dir} mono />
                  </div>
                )}
                {firmware?.error && (
                  <div className="mt-2">
                    <Banner tone="bad" title="Last firmware error">
                      {firmware.error}
                    </Banner>
                  </div>
                )}
                {firmware?.log_tail && firmware.log_tail.length > 0 && (
                  <details className="mt-3">
                    <summary className="cursor-pointer text-[11px] text-muted">Build log ({firmware.log_tail.length} lines)</summary>
                    <div className="mt-2 h-40 overflow-y-auto rounded-lg border border-line bg-black/50 px-3 py-2 font-mono text-[10.5px] leading-relaxed text-[#b9c3d3]">
                      {firmware.log_tail.map((line, index) => (
                        <div key={index} className="whitespace-pre-wrap break-words">
                          {line}
                        </div>
                      ))}
                    </div>
                  </details>
                )}
              </Panel>

              <Panel title="Device benchmarks" subtitle="Imported from the board; never fabricated." icon={<HardDrive size={14} />}>
                <div className="flex flex-col gap-2 sm:flex-row">
                  <TextInput
                    value={benchPath}
                    onChange={(e) => setBenchPath(e.target.value)}
                    placeholder="path to a device benchmark JSON"
                    className="flex-1"
                  />
                  <Button size="sm" busy={busy === "bench"} onClick={importBench} icon={<Upload size={12} />}>
                    Import
                  </Button>
                </div>
                {benchmarks.length > 0 ? (
                  <div className="mt-3 overflow-x-auto">
                    <table className="w-full text-[11.5px]">
                      <thead>
                        <tr className="border-b border-line">
                          <th className="table-head px-2 py-1.5 text-left">Run</th>
                          <th className="table-head px-2 py-1.5 text-right">tok/s</th>
                          <th className="table-head px-2 py-1.5 text-right">Latency</th>
                          <th className="table-head px-2 py-1.5 text-right">RAM</th>
                        </tr>
                      </thead>
                      <tbody>
                        {benchmarks.map((run) => (
                          <tr key={run.id} className="border-b border-line/60">
                            <td className="px-2 py-1.5 text-ink-dim">{run.label}</td>
                            <td className="mono-num px-2 py-1.5 text-right">{run.tokens_per_second.toFixed(2)}</td>
                            <td className="mono-num px-2 py-1.5 text-right">{run.latency_ms.toFixed(2)} ms</td>
                            <td className="mono-num px-2 py-1.5 text-right">{bytes(run.ram_bytes)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                ) : (
                  <p className="mt-2 text-[11px] leading-relaxed text-muted">
                    Flash the firmware, open the serial monitor and send <span className="font-mono">bench</span>. Save
                    the printed JSON to a file and import it here. ATOM rejects reports with zero or missing values.
                  </p>
                )}
                {project.memory && (
                  <div className="mt-3 text-[11px] text-muted">
                    Estimated artifact for {project.memory.target_name}: {bytes(project.memory.total_flash_bytes)} of{" "}
                    {bytes(project.memory.flash_bytes)} flash, {bytes(project.memory.total_ram_bytes)} RAM. See the
                    Hardware Lab for the full budget.
                  </div>
                )}
              </Panel>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
