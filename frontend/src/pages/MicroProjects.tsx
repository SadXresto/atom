import { useEffect, useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  ArrowRight,
  Boxes,
  Cpu,
  HardDrive,
  Layers,
  Microchip,
  Table2,
  Plus,
  Rocket,
  Sparkles,
  Wand2,
} from "lucide-react";
import { api } from "../lib/api";
import { bytes, dateTime, params, percent } from "../lib/format";
import { useApi } from "../lib/hooks";
import type {
  MicroArchitecture,
  MicroCapabilities,
  MicroOverviewData,
  MicroTarget,
  ProjectSummary,
} from "../lib/types";
import {
  Badge,
  Button,
  EmptyState,
  Field,
  Modal,
  PageHeader,
  Panel,
  Progress,
  Select,
  Stat,
  TextInput,
  cx,
  toast,
} from "../components/ui";
import { CapabilityGrid, FormatBadge } from "../components/MicroBits";

export default function MicroProjects() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { data, reload } = useApi<MicroOverviewData>("/micro/overview", 10000);
  const { data: catalog } = useApi<{ targets: MicroTarget[]; architectures: MicroArchitecture[] }>("/micro/catalog");
  const { data: caps } = useApi<MicroCapabilities>("/micro/capabilities", 60000);
  const [open, setOpen] = useState(searchParams.get("new") === "1");
  const [busy, setBusy] = useState(false);
  const [form, setForm] = useState({
    name: "",
    domain: "",
    description: "",
    target_id: "esp32s3",
  });

  useEffect(() => {
    if (searchParams.get("new") === "1") setOpen(true);
  }, [searchParams]);

  const projects = data?.projects ?? [];
  const targets = catalog?.targets ?? [];
  const targetOf = useMemo(() => {
    const map = new Map<string, MicroTarget>();
    targets.forEach((t) => map.set(t.id, t));
    return map;
  }, [targets]);

  const create = async () => {
    if (form.name.trim().length < 2) {
      toast("Give the project a name first", "error");
      return;
    }
    setBusy(true);
    try {
      const result = await api<{ project: { id: string } }>("/micro/projects", {
        method: "POST",
        body: JSON.stringify(form),
      });
      toast("Micro project created", "ok");
      setOpen(false);
      navigate(`/micro/${result.project.id}`);
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(false);
    }
  };

  const counts = data?.counts ?? { projects: 0, trained: 0, quantized: 0, built: 0 };

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="ATOM Micro"
        title="Specialist models for real hardware"
        description="Create a project, train a small model on your own dataset, quantize it, verify the memory budget for a specific microcontroller, export a checksummed artifact and build firmware."
        actions={
          <>
            {projects.length > 0 && (
              <Button
                onClick={() => navigate(`/micro/${projects[0].id}/classifier`)}
                icon={<Table2 size={13} />}
              >
                Classify CSV
              </Button>
            )}
            <Button onClick={() => navigate("/hardware")} icon={<Cpu size={13} />}>
              Hardware Lab
            </Button>
            <Button variant="primary" onClick={() => setOpen(true)} icon={<Plus size={13} />}>
              New project
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat label="Projects" value={counts.projects} sub="whole 11-step workflow" icon={<Microchip size={13} />} />
        <Stat
          label="Trained"
          value={counts.trained}
          sub="real weights on disk"
          tone={counts.trained ? "ok" : "default"}
          icon={<Wand2 size={13} />}
        />
        <Stat
          label="Quantized"
          value={counts.quantized}
          sub="measured loss delta"
          tone={counts.quantized ? "accent" : "default"}
          icon={<Layers size={13} />}
        />
        <Stat
          label="Firmware built"
          value={counts.built}
          sub={data ? `${bytes(data.storage_bytes)} of artifacts` : undefined}
          tone={counts.built ? "ok" : "default"}
          icon={<Rocket size={13} />}
        />
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-[1.55fr_1fr]">
        <Panel
          title="Projects"
          subtitle="Each project owns its dataset copy, weights, exports and firmware build."
          icon={<Boxes size={14} />}
          actions={<Badge>{projects.length}</Badge>}
        >
          {projects.length === 0 ? (
            <EmptyState
              icon={<Microchip size={18} />}
              title="No Micro projects yet"
              description="Start with a dataset of a few hundred characters and an ESP32-S3. The built-in byte MLP trainers run here without Python; transformer architectures require PyTorch."
              action={
                <Button variant="primary" onClick={() => setOpen(true)} icon={<Plus size={13} />}>
                  Create the first project
                </Button>
              }
            />
          ) : (
            <div className="grid gap-3 sm:grid-cols-2">
              {projects.map((project) => (
                <ProjectCard
                  key={project.id}
                  project={project}
                  target={targetOf.get(project.target_id)}
                  onOpen={() => navigate(`/micro/${project.id}`)}
                />
              ))}
            </div>
          )}
        </Panel>

        <div className="space-y-4">
          <Panel title="Toolchain" subtitle="Probed from PATH when this page loads." icon={<Cpu size={14} />}>
            <CapabilityGrid caps={caps} />
            <div className="mt-3 space-y-2 text-[11.5px] leading-relaxed text-muted">
              <div className="flex items-start gap-2">
                <Sparkles size={12} className={cx("mt-0.5", caps?.pytorch.available ? "text-ok" : "text-warn")} />
                <span>
                  {caps?.pytorch.available
                    ? "PyTorch detected: transformer architectures can be trained by the external adapter."
                    : "PyTorch not detected: built-in Atom MLP architectures train here; transformer options stay disabled."}
                </span>
              </div>
              <div className="flex items-start gap-2">
                <Rocket size={12} className={cx("mt-0.5", caps?.platformio.available ? "text-ok" : "text-warn")} />
                <span>
                  {caps?.platformio.available
                    ? "PlatformIO detected: firmware generation can be compiled and flashed."
                    : "PlatformIO not detected: export works, firmware compilation reports unavailable until `pio` is installed."}
                </span>
              </div>
            </div>
          </Panel>

          <Panel title="How a project flows" icon={<Layers size={14} />}>
            <ol className="space-y-2 text-[12px] leading-relaxed text-muted">
              {[
                "Name the specialist and pick the exact board.",
                "Import a dataset; ATOM computes real statistics and a deterministic split.",
                "Train a byte-level model here and watch the measured loss curve.",
                "Quantize and re-measure validation loss from the dequantized weights.",
                "Check RAM and flash against the datasheet figures.",
                "Export checksummed artifacts and compile firmware with PlatformIO.",
                "Flash the board and import the benchmark report it prints.",
              ].map((line, index) => (
                <li key={line} className="flex gap-2.5">
                  <span className="mono-num mt-px flex h-4 w-4 shrink-0 items-center justify-center rounded-full border border-line text-[9.5px] text-muted">
                    {index + 1}
                  </span>
                  <span>{line}</span>
                </li>
              ))}
            </ol>
          </Panel>
        </div>
      </div>

      <Modal
        open={open}
        onClose={() => setOpen(false)}
        title="New Micro project"
        description="The project name and target can change later; dataset and weights stay with the project."
        footer={
          <>
            <Button onClick={() => setOpen(false)}>Cancel</Button>
            <Button variant="primary" busy={busy} onClick={create} icon={<ArrowRight size={13} />}>
              Create project
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Field label="Project name">
            <TextInput
              autoFocus
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder="Desk controller commands"
            />
          </Field>
          <Field label="Specialist domain" hint="what the model should become good at">
            <TextInput
              value={form.domain}
              onChange={(e) => setForm({ ...form, domain: e.target.value })}
              placeholder="Home automation · command syntax"
            />
          </Field>
          <Field label="Description">
            <TextInput
              value={form.description}
              onChange={(e) => setForm({ ...form, description: e.target.value })}
              placeholder="Short phrases that control a desk light over serial"
            />
          </Field>
          <Field label="Target hardware" hint="datasheet memory is used for fit analysis">
            <Select value={form.target_id} onChange={(e) => setForm({ ...form, target_id: e.target.value })}>
              {targets.map((target) => (
                <option key={target.id} value={target.id}>
                  {target.name} — {target.vendor}
                </option>
              ))}
            </Select>
          </Field>
          {targetOf.get(form.target_id) && (
            <div className="rounded-lg border border-line bg-black/25 px-3 py-2 text-[11.5px] text-muted">
              {bytes(targetOf.get(form.target_id)!.sram_bytes)} SRAM ·{" "}
              {bytes(targetOf.get(form.target_id)!.flash_bytes)} flash
              {targetOf.get(form.target_id)!.psram_bytes > 0
                ? ` · ${bytes(targetOf.get(form.target_id)!.psram_bytes)} PSRAM`
                : ""}{" "}
              · {targetOf.get(form.target_id)!.mcu}
            </div>
          )}
        </div>
      </Modal>
    </div>
  );
}

function ProjectCard({
  project,
  target,
  onOpen,
}: {
  project: ProjectSummary;
  target?: MicroTarget;
  onOpen: () => void;
}) {
  return (
    <button
      onClick={onOpen}
      className="panel-flat group flex w-full flex-col gap-2.5 px-3.5 py-3 text-left transition-colors hover:border-line-strong hover:bg-white/[0.03]"
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="truncate text-[13px] font-medium text-ink">{project.name}</div>
          <div className="mt-0.5 truncate text-[11px] text-muted">
            {project.domain || "specialist model"} · {target?.name ?? project.target_id}
          </div>
        </div>
        <Badge tone={project.completed_steps >= 9 ? "ok" : project.completed_steps >= 5 ? "accent" : "neutral"}>
          step {project.next_step}/11
        </Badge>
      </div>
      <Progress value={project.completed_steps / 11} height={4} />
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[10.5px] text-muted">
        <span className="mono-num">{project.dataset_documents ?? 0} docs</span>
        <span className="mono-num">{project.params ? `${params(project.params)} params` : "untrained"}</span>
        {project.val_perplexity !== null && (
          <span className="mono-num">ppl {project.val_perplexity.toFixed(2)}</span>
        )}
        {project.quant_format && <FormatBadge format={project.quant_format} />}
        {project.firmware_built && <Badge tone="ok">firmware</Badge>}
      </div>
      <div className="mt-0.5 flex items-center justify-between gap-2 text-[10.5px] text-muted">
        <span>updated {dateTime(project.updated_at)}</span>
        <span className="flex items-center gap-1 text-accent opacity-0 transition-opacity group-hover:opacity-100">
          Continue <ArrowRight size={11} />
        </span>
      </div>
      <div className="sr-only">{percent(project.completed_steps * (100 / 11))} complete</div>
    </button>
  );
}
