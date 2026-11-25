import { useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  ArrowRight,
  Boxes,
  Check,
  Cpu,
  Microchip,
  Monitor,
  ShieldCheck,
  Sparkles,
  Terminal,
} from "lucide-react";
import { api } from "../lib/api";
import { useApi } from "../lib/hooks";
import type { BackendStatus, Config, MicroCapabilities } from "../lib/types";
import { Badge, Button, cx, toast } from "../components/ui";

const STEPS = ["Environment", "Starting point", "Ready"];

export default function Onboarding() {
  const navigate = useNavigate();
  const { data: backendData } = useApi<{ backend: BackendStatus }>("/backend");
  const { data: caps } = useApi<MicroCapabilities>("/micro/capabilities");
  const { data: config } = useApi<Config>("/settings");
  const [step, setStep] = useState(0);
  const [goal, setGoal] = useState<"library" | "micro" | "monitor">("library");
  const [saving, setSaving] = useState(false);

  const backend = backendData?.backend;

  const finish = async (path: string) => {
    setSaving(true);
    try {
      if (config) {
        await api("/settings", {
          method: "PUT",
          body: JSON.stringify({ ...config, onboarding_completed: true }),
        });
      }
      window.localStorage.setItem("atom.onboarded", "1");
      navigate(path, { replace: true });
    } catch (e) {
      toast((e as Error).message, "error");
      window.localStorage.setItem("atom.onboarded", "1");
      navigate(path, { replace: true });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fade-in mx-auto flex min-h-[calc(100vh-120px)] max-w-5xl flex-col justify-center">
      <div className="mb-8 flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="flex h-11 w-11 items-center justify-center rounded-xl border border-accent/30 bg-gradient-to-br from-accent/25 to-accent-2/10">
            <Cpu size={20} className="text-accent" />
          </div>
          <div>
            <div className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted">Welcome to ATOM</div>
            <h1 className="mt-1 text-[24px] font-semibold tracking-[-0.015em] text-ink">
              A local AI engineering workspace
            </h1>
          </div>
        </div>
        <button onClick={() => void finish("/")} className="text-[12px] text-muted hover:text-ink-dim" disabled={saving}>
          Skip setup
        </button>
      </div>

      <div className="mb-6 flex items-center gap-3">
        {STEPS.map((label, index) => (
          <div key={label} className="flex flex-1 items-center gap-2">
            <div
              className={cx(
                "flex h-6 w-6 shrink-0 items-center justify-center rounded-full border text-[11px] font-semibold",
                index < step && "border-ok/40 bg-ok/10 text-ok",
                index === step && "border-accent/60 bg-accent/15 text-accent",
                index > step && "border-line text-muted",
              )}
            >
              {index < step ? <Check size={12} /> : index + 1}
            </div>
            <span className={cx("text-[12px]", index === step ? "text-ink" : "text-muted")}>{label}</span>
            {index < STEPS.length - 1 && <div className="h-px flex-1 bg-line" />}
          </div>
        ))}
      </div>

      <div className="panel overflow-hidden">
        <div className="grid gap-0 lg:grid-cols-[1.35fr_1fr]">
          <div className="min-w-0 p-6 md:p-8">
            {step === 0 && (
              <div className="fade-in">
                <h2 className="text-[18px] font-semibold text-ink">Check what this machine can run</h2>
                <p className="mt-1.5 max-w-xl text-[13px] leading-relaxed text-muted">
                  ATOM measures everything from your system and the tools it can find. Missing tools disable exactly
                  one capability and say what is required — nothing is simulated.
                </p>
                <div className="mt-6 space-y-2">
                  <ProbeRow
                    icon={<Terminal size={14} />}
                    title="llama.cpp inference"
                    detail="Streaming chat, benchmarking, teacher distillation"
                    ready={backend?.inference_ready}
                    hint="Install llama.cpp and set the llama-server path in Settings."
                  />
                  <ProbeRow
                    icon={<Boxes size={14} />}
                    title="Permanent quantization"
                    detail="llama-quantize for separate compressed GGUF files"
                    ready={backend?.compression_ready}
                    hint="Install llama.cpp and set the llama-quantize path in Settings."
                  />
                  <ProbeRow
                    icon={<Microchip size={14} />}
                    title="Embedded builds"
                    detail="PlatformIO for firmware compilation and flashing"
                    ready={caps?.platformio.available}
                    hint="pip install platformio"
                  />
                  <ProbeRow
                    icon={<Sparkles size={14} />}
                    title="Transformer training"
                    detail="PyTorch adapter for transformer architectures"
                    ready={caps?.pytorch.available}
                    hint="pip install torch — the built-in byte MLP trainers work without it."
                  />
                </div>
                <div className="mt-6 flex items-center gap-2">
                  <Button variant="primary" onClick={() => setStep(1)} icon={<ArrowRight size={13} />}>
                    Continue
                  </Button>
                  <Button onClick={() => void finish("/settings")}>Open tool settings</Button>
                </div>
              </div>
            )}

            {step === 1 && (
              <div className="fade-in">
                <h2 className="text-[18px] font-semibold text-ink">Where do you want to start?</h2>
                <p className="mt-1.5 text-[13px] leading-relaxed text-muted">
                  Everything stays available in the sidebar. This only picks your landing spot.
                </p>
                <div className="mt-6 space-y-2.5">
                  <GoalCard
                    selected={goal === "library"}
                    onSelect={() => setGoal("library")}
                    icon={<Boxes size={16} />}
                    title="Desktop models"
                    body="Import a GGUF, tune how it runs, compress it permanently and compare measured results."
                    meta="Library · Compression · Runtime · Comparison"
                  />
                  <GoalCard
                    selected={goal === "micro"}
                    onSelect={() => setGoal("micro")}
                    icon={<Microchip size={16} />}
                    title="ATOM Micro"
                    body="Create a specialist model for a microcontroller: dataset, training, quantization, export, firmware."
                    meta="Micro Studio · Training · Hardware Lab · Deployment"
                  />
                  <GoalCard
                    selected={goal === "monitor"}
                    onSelect={() => setGoal("monitor")}
                    icon={<Monitor size={16} />}
                    title="Measure this machine"
                    body="Watch real CPU, RAM and per-process memory, then decide what fits before loading a model."
                    meta="System Monitor"
                  />
                </div>
                <div className="mt-6 flex items-center gap-2">
                  <Button onClick={() => setStep(0)}>Back</Button>
                  <Button variant="primary" onClick={() => setStep(2)} icon={<ArrowRight size={13} />}>
                    Continue
                  </Button>
                </div>
              </div>
            )}

            {step === 2 && (
              <div className="fade-in">
                <h2 className="text-[18px] font-semibold text-ink">You are ready</h2>
                <p className="mt-1.5 text-[13px] leading-relaxed text-muted">
                  ATOM stores projects, models and reports under your data root. Model files are only read; nothing is
                  deleted unless you explicitly ask for it.
                </p>
                <div className="mt-5 grid gap-2 text-[12.5px]">
                  <SummaryRow label="Workspace" value={config?.data_root ?? "default data root"} />
                  <SummaryRow label="Library root" value={config?.models_dir ?? "—"} />
                  <SummaryRow label="Micro projects" value="managed per project, with checksums on every export" />
                  <SummaryRow
                    label="Toolchain"
                    value={
                      backend?.inference_ready && backend?.compression_ready
                        ? "llama.cpp detected"
                        : "llama.cpp incomplete — inference/compression show as unavailable"
                    }
                  />
                </div>
                <div className="mt-6 flex flex-wrap items-center gap-2">
                  <Button onClick={() => setStep(1)}>Back</Button>
                  <Button
                    variant="primary"
                    busy={saving}
                    icon={<ArrowRight size={13} />}
                    onClick={() =>
                      void finish(goal === "micro" ? "/micro?new=1" : goal === "monitor" ? "/monitor" : "/")
                    }
                  >
                    Enter workspace
                  </Button>
                </div>
              </div>
            )}
          </div>

          <div className="border-t border-line bg-black/25 p-6 md:p-8 lg:border-l lg:border-t-0">
            <div className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-muted">
              <ShieldCheck size={13} className="text-accent-2" /> Built for trust
            </div>
            <ul className="mt-4 space-y-3 text-[12px] leading-relaxed text-ink-dim">
              <li>Every number on screen is measured from your machine, your model or the tool's own output.</li>
              <li>Estimates are labelled. Quality is never invented.</li>
              <li>Source models are never modified or deleted by compression.</li>
              <li>External tools run with explicit arguments, never through a shell.</li>
              <li>When a capability is missing, ATOM says so and explains what to install.</li>
            </ul>
            <div className="mt-6 rounded-xl border border-line bg-white/[0.02] p-3">
              <div className="label mb-1.5">Keyboard</div>
              <div className="space-y-1.5 text-[11.5px] text-muted">
                <div className="flex items-center justify-between gap-2">
                  <span>Command palette</span>
                  <span className="chip">⌘K</span>
                </div>
                <div className="flex items-center justify-between gap-2">
                  <span>Jump to section</span>
                  <span className="chip">g then o/l/c/r/u…</span>
                </div>
              </div>
            </div>
            <Badge tone="info" className="mt-4">
              No account · no telemetry · local only
            </Badge>
          </div>
        </div>
      </div>
    </div>
  );
}

function ProbeRow({
  icon,
  title,
  detail,
  ready,
  hint,
}: {
  icon: React.ReactNode;
  title: string;
  detail: string;
  ready?: boolean;
  hint: string;
}) {
  return (
    <div className="flex items-start gap-3 rounded-xl border border-line bg-white/[0.015] px-3.5 py-2.5">
      <span className={cx("mt-0.5", ready ? "text-ok" : "text-warn")}>{icon}</span>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 text-[12.5px] font-medium text-ink">
          {title}
          <Badge tone={ready ? "ok" : "warn"}>{ready ? "ready" : "missing"}</Badge>
        </div>
        <div className="mt-0.5 text-[11.5px] text-muted">{ready ? detail : hint}</div>
      </div>
    </div>
  );
}

function GoalCard({
  selected,
  onSelect,
  icon,
  title,
  body,
  meta,
}: {
  selected: boolean;
  onSelect: () => void;
  icon: React.ReactNode;
  title: string;
  body: string;
  meta: string;
}) {
  return (
    <button
      onClick={onSelect}
      className={cx(
        "flex w-full items-start gap-3 rounded-xl border px-3.5 py-3 text-left transition-colors",
        selected ? "border-accent/45 bg-accent/[0.07]" : "border-line bg-white/[0.015] hover:border-line-strong",
      )}
    >
      <span className={cx("mt-0.5 flex h-8 w-8 items-center justify-center rounded-lg border", selected ? "border-accent/40 text-accent" : "border-line text-muted")}>
        {icon}
      </span>
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-2 text-[13px] font-medium text-ink">
          {title}
          {selected && <Check size={13} className="text-accent" />}
        </span>
        <span className="mt-0.5 block text-[12px] leading-relaxed text-muted">{body}</span>
        <span className="mt-1 block text-[10.5px] uppercase tracking-wide text-[#5f6878]">{meta}</span>
      </span>
    </button>
  );
}

function SummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-4 border-b border-line/60 py-1.5 last:border-0">
      <span className="shrink-0 text-[11.5px] text-muted">{label}</span>
      <span className="min-w-0 truncate text-right text-[12px] text-ink-dim">{value}</span>
    </div>
  );
}
