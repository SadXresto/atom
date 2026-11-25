import type { ReactNode } from "react";
import { AlertTriangle, Check, CircleSlash, Loader2 } from "lucide-react";
import { useApi } from "../lib/hooks";
import type { MicroCapabilities, MicroProjectResponse, ToolProbe } from "../lib/types";
import { Badge, Banner, Button, cx } from "./ui";

export const MICRO_STEPS = [
  { id: 1, title: "Create project", short: "Project" },
  { id: 2, title: "Target hardware", short: "Hardware" },
  { id: 3, title: "Prepare dataset", short: "Dataset" },
  { id: 4, title: "Architecture", short: "Architecture" },
  { id: 5, title: "Train", short: "Training" },
  { id: 6, title: "Evaluate", short: "Evaluation" },
  { id: 7, title: "Quantize", short: "Quantize" },
  { id: 8, title: "RAM & flash", short: "Memory" },
  { id: 9, title: "Export", short: "Export" },
  { id: 10, title: "Firmware", short: "Firmware" },
  { id: 11, title: "Benchmarks", short: "Benchmarks" },
];

export function useMicroProject(id: string | null) {
  return useApi<MicroProjectResponse>(id ? `/micro/projects/${id}` : null, 15000);
}

export function StepRail({
  active,
  completed,
  onSelect,
}: {
  active: number;
  completed: number[];
  onSelect: (step: number) => void;
}) {
  return (
    <nav className="panel-flat overflow-hidden">
      {MICRO_STEPS.map((step) => {
        const done = completed.includes(step.id);
        const current = active === step.id;
        return (
          <button
            key={step.id}
            onClick={() => onSelect(step.id)}
            className={cx(
              "flex w-full items-center gap-2.5 border-b border-line px-3 py-2 text-left last:border-0 transition-colors",
              current ? "bg-accent/[0.09]" : "hover:bg-white/[0.025]",
            )}
          >
            <span
              className={cx(
                "flex h-5 w-5 shrink-0 items-center justify-center rounded-full border text-[10.5px] font-semibold",
                done && !current && "border-ok/40 bg-ok/10 text-ok",
                current && "border-accent/60 bg-accent/20 text-accent",
                !done && !current && "border-line text-muted",
              )}
            >
              {done && !current ? <Check size={11} /> : step.id}
            </span>
            <span className={cx("truncate text-[12px]", current ? "text-ink" : done ? "text-ink-dim" : "text-muted")}>
              {step.title}
            </span>
          </button>
        );
      })}
    </nav>
  );
}

export function CapabilityNote({
  probe,
  children,
}: {
  probe: ToolProbe | undefined;
  children?: ReactNode;
}) {
  if (!probe) return null;
  if (probe.available) {
    return (
      <Banner tone="ok" title={`${probe.name} available`}>
        {probe.version ? `${probe.version} · ` : ""}
        <span className="font-mono text-[11px]">{probe.path}</span>
      </Banner>
    );
  }
  return (
    <Banner tone="warn" title={`${probe.name} is required for this step`}>
      {probe.required_for}. {probe.install_hint}
    </Banner>
  );
}

export function CapabilityGrid({ caps }: { caps?: MicroCapabilities | null }) {
  if (!caps) return null;
  const entries: Array<[string, ToolProbe]> = [
    ["PlatformIO", caps.platformio],
    ["Python", caps.python],
    ["PyTorch", caps.pytorch],
    ["Compiler", caps.cc],
    ["esptool", caps.esptool],
  ];
  return (
    <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
      {entries.map(([label, probe]) => (
        <div key={label} className="panel-flat px-3 py-2">
          <div className="flex items-center justify-between gap-2">
            <span className="text-[11px] font-medium text-ink-dim">{label}</span>
            <span className={cx("h-1.5 w-1.5 rounded-full", probe.available ? "bg-ok" : "bg-warn")} />
          </div>
          <div className="mt-1 truncate text-[10.5px] text-muted" title={probe.version ?? probe.install_hint}>
            {probe.available ? probe.version ?? "detected" : "not found"}
          </div>
        </div>
      ))}
    </div>
  );
}

export function Metric({
  label,
  value,
  sub,
  tone,
}: {
  label: string;
  value: ReactNode;
  sub?: ReactNode;
  tone?: "ok" | "warn" | "bad" | "accent";
}) {
  const color =
    tone === "ok" ? "text-ok" : tone === "warn" ? "text-warn" : tone === "bad" ? "text-bad" : tone === "accent" ? "text-accent" : "text-ink";
  return (
    <div className="panel-flat px-3 py-2.5">
      <div className="label">{label}</div>
      <div className={cx("mono-num mt-1.5 text-[17px] font-semibold leading-none", color)}>{value}</div>
      {sub && <div className="mt-1 text-[11px] leading-snug text-muted">{sub}</div>}
    </div>
  );
}

export function Requirement({ ok, label, detail }: { ok: boolean; label: string; detail: string }) {
  return (
    <div className="flex items-start gap-2 text-[11.5px]">
      <span className={cx("mt-0.5 shrink-0", ok ? "text-ok" : "text-warn")}>
        {ok ? <Check size={12} /> : <CircleSlash size={12} />}
      </span>
      <span className="min-w-0">
        <span className="text-ink-dim">{label}</span>
        <span className="text-muted"> — {detail}</span>
      </span>
    </div>
  );
}

export function InlineSpinner({ label }: { label: string }) {
  return (
    <span className="flex items-center gap-2 text-[12px] text-muted">
      <Loader2 size={13} className="animate-spin" /> {label}
    </span>
  );
}

export function WarningLine({ children }: { children: ReactNode }) {
  return (
    <div className="flex items-start gap-2 text-[11.5px] text-warn">
      <AlertTriangle size={12} className="mt-0.5 shrink-0" /> <span className="text-ink-dim">{children}</span>
    </div>
  );
}

export function FormatBadge({ format }: { format: string }) {
  const tone = format === "int4" ? "info" : format === "int8" ? "accent" : format === "f16" ? "neutral" : "neutral";
  return <Badge tone={tone}>{format}</Badge>;
}

export function DownloadArtifact({
  projectId,
  name,
  bytes,
  sha256,
}: {
  projectId: string;
  name: string;
  bytes: number;
  sha256: string;
}) {
  return (
    <Button size="sm" onClick={() => window.open(`/api/micro/projects/${projectId}/files/${name}`, "_blank")}>
      {name} · {bytes} B
    </Button>
  );
}
