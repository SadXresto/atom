import { useEffect, useMemo, useRef, useState } from "react";
import { NavLink, useLocation, useNavigate } from "react-router-dom";
import {
  Activity,
  Boxes,
  ChevronRight,
  Command,
  Cpu,
  FlaskConical,
  Gauge,
  GitCompareArrows,
  GraduationCap,
  Layers,
  LayoutDashboard,
  MessageSquareText,
  Rocket,
  Search,
  Settings,
  ShieldAlert,
  Smartphone,
  CircuitBoard,
  ChartNoAxesCombined,
} from "lucide-react";
import { useApi } from "../lib/hooks";
import type { BackendStatus, MicroCapabilities } from "../lib/types";
import { cx } from "./ui";

interface NavItem {
  to: string;
  label: string;
  icon: typeof LayoutDashboard;
  key: string;
  end?: boolean;
}

const SECTIONS: Array<{ label: string; items: NavItem[] }> = [
  {
    label: "Workspace",
    items: [
      { to: "/", label: "Dashboard", icon: LayoutDashboard, key: "o", end: true },
      { to: "/library", label: "Model Library", icon: Boxes, key: "l" },
      { to: "/inference", label: "Inference Workspace", icon: MessageSquareText, key: "i" },
      { to: "/monitor", label: "System Monitor", icon: Activity, key: "m" },
    ],
  },
  {
    label: "Optimize",
    items: [
      { to: "/compression", label: "Compression Studio", icon: Layers, key: "c" },
      { to: "/runtime", label: "Runtime Optimization", icon: Gauge, key: "r" },
      { to: "/comparison", label: "Comparison Lab", icon: GitCompareArrows, key: "p" },
      { to: "/benchmarks", label: "Benchmark Center", icon: ChartNoAxesCombined, key: "b" },
    ],
  },
  {
    label: "ATOM Micro",
    items: [
      { to: "/micro", label: "Micro Studio", icon: Smartphone, key: "u" },
      { to: "/training", label: "Training Workspace", icon: GraduationCap, key: "t" },
      { to: "/distillation", label: "Distillation Lab", icon: FlaskConical, key: "d" },
      { to: "/hardware", label: "Hardware Lab", icon: CircuitBoard, key: "w" },
      { to: "/deployment", label: "Deployment Center", icon: Rocket, key: "y" },
    ],
  },
  {
    label: "System",
    items: [{ to: "/settings", label: "Settings", icon: Settings, key: "s" }],
  },
];

const NAV: NavItem[] = SECTIONS.flatMap((section) => section.items);

export function Layout({ children }: { children: React.ReactNode }) {
  const { data } = useApi<{ backend: BackendStatus }>("/backend", 20000);
  const { data: caps } = useApi<MicroCapabilities>("/micro/capabilities", 60000);
  const backend = data?.backend;
  const navigate = useNavigate();
  const location = useLocation();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const gArmed = useRef(false);
  const gTimer = useRef<number | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const items = useMemo(
    () => [
      ...NAV.map((item) => ({ label: item.label, to: item.to, hint: `g ${item.key}` })),
      { label: "Import a GGUF model", to: "/library?import=1", hint: "library" },
      { label: "Quantize a model", to: "/compression", hint: "studio" },
      { label: "Create a Micro project", to: "/micro?new=1", hint: "atom micro" },
      { label: "Train a specialist model", to: "/training", hint: "micro" },
      { label: "Analyse hardware fit", to: "/hardware", hint: "micro" },
      { label: "Run a comparison", to: "/comparison", hint: "bench" },
      { label: "Runtime settings", to: "/settings", hint: "tuning" },
    ],
    [],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((item) => item.label.toLowerCase().includes(q) || item.hint.includes(q));
  }, [items, query]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const typing = target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);

      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
        setQuery("");
        setCursor(0);
        return;
      }
      if (paletteOpen) {
        if (e.key === "Escape") setPaletteOpen(false);
        if (e.key === "ArrowDown") {
          e.preventDefault();
          setCursor((c) => Math.min(filtered.length - 1, c + 1));
        }
        if (e.key === "ArrowUp") {
          e.preventDefault();
          setCursor((c) => Math.max(0, c - 1));
        }
        if (e.key === "Enter" && filtered[cursor]) {
          e.preventDefault();
          navigate(filtered[cursor].to);
          setPaletteOpen(false);
        }
        return;
      }
      if (typing) return;
      if (e.key.toLowerCase() === "g") {
        gArmed.current = true;
        if (gTimer.current) window.clearTimeout(gTimer.current);
        gTimer.current = window.setTimeout(() => {
          gArmed.current = false;
        }, 1200);
        return;
      }
      if (gArmed.current) {
        const match = NAV.find((n) => n.key === e.key.toLowerCase());
        if (match) {
          e.preventDefault();
          navigate(match.to);
        }
        gArmed.current = false;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate, paletteOpen, filtered, cursor]);

  useEffect(() => {
    if (paletteOpen) window.setTimeout(() => inputRef.current?.focus(), 30);
  }, [paletteOpen]);

  const active = NAV.find((n) => (n.end ? location.pathname === n.to : location.pathname.startsWith(n.to)));

  return (
    <div className="relative z-10 flex h-full">
      <aside className="hidden w-[248px] shrink-0 flex-col border-r border-line bg-[#080a0d]/80 px-3 py-4 backdrop-blur lg:flex">
        <button
          onClick={() => navigate("/")}
          className="mb-5 flex items-center gap-2.5 px-1.5 text-left"
        >
          <div className="relative flex h-8 w-8 items-center justify-center rounded-[9px] border border-accent/30 bg-gradient-to-br from-accent/25 to-accent-2/10">
            <Cpu size={15} className="text-accent" />
          </div>
          <div className="min-w-0">
            <div className="text-[14px] font-semibold leading-none tracking-[0.02em] text-ink">ATOM</div>
            <div className="mt-1 text-[10px] uppercase tracking-[0.14em] text-muted">Local AI engineering</div>
          </div>
        </button>

        <nav className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pb-2">
          {SECTIONS.map((section) => (
            <div key={section.label}>
              <div className="mb-1 px-2 text-[9.5px] font-semibold uppercase tracking-[0.14em] text-[#5b6375]">
                {section.label}
              </div>
              <div className="flex flex-col gap-0.5">
                {section.items.map((item) => {
                  const Icon = item.icon;
                  return (
                    <NavLink key={item.to} to={item.to} end={item.end} className="nav-item">
                      <Icon size={15} className="nav-icon shrink-0" />
                      <span className="truncate">{item.label}</span>
                    </NavLink>
                  );
                })}
              </div>
            </div>
          ))}
        </nav>

        <div className="mt-2 rounded-xl border border-line bg-white/[0.015] p-3">
          <div className="label mb-2">Runtime</div>
          <div className="space-y-2 text-[11.5px]">
            <StatusRow label="llama-server" ok={backend?.inference_ready} />
            <StatusRow label="llama-quantize" ok={backend?.compression_ready} />
            <StatusRow label="PlatformIO" ok={caps?.platformio.available} />
          </div>
          <button
            onClick={() => navigate("/settings")}
            className="mt-2.5 flex w-full items-center justify-between gap-1 rounded-md border border-line px-2 py-1 text-[11px] text-muted hover:border-line-strong hover:text-ink-dim"
          >
            Configure toolchain
            <ChevronRight size={11} />
          </button>
        </div>

        <div className="mt-auto flex items-center justify-between gap-2 px-1 pt-3 text-[10.5px] text-muted">
          <button
            onClick={() => setPaletteOpen(true)}
            className="flex items-center gap-1.5 rounded-md border border-line px-1.5 py-1 hover:border-line-strong hover:text-ink-dim"
          >
            <Command size={10} /> K
          </button>
          <span className="mono-num">v0.1.0</span>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex items-center justify-between gap-3 border-b border-line bg-[#080a0d]/70 px-4 py-2.5 backdrop-blur lg:hidden">
          <div className="flex items-center gap-2">
            <Cpu size={16} className="text-accent" />
            <span className="text-[13px] font-semibold">ATOM</span>
          </div>
          <select
            value={active?.to ?? "/"}
            onChange={(e) => navigate(e.target.value)}
            className="input w-auto max-w-[58%] py-1 text-[12px]"
          >
            {SECTIONS.map((section) => (
              <optgroup key={section.label} label={section.label}>
                {section.items.map((item) => (
                  <option key={item.to} value={item.to}>
                    {item.label}
                  </option>
                ))}
              </optgroup>
            ))}
          </select>
        </header>

        <main className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[1280px] px-5 py-6 md:px-7 md:py-7">{children}</div>
        </main>
      </div>

      {paletteOpen && (
        <div
          className="fixed inset-0 z-50 flex items-start justify-center bg-black/60 p-4 pt-[12vh] backdrop-blur-sm"
          onClick={() => setPaletteOpen(false)}
        >
          <div
            className="panel w-full max-w-md overflow-hidden fade-in"
            onClick={(e) => e.stopPropagation()}
            role="dialog"
            aria-label="Command palette"
          >
            <div className="flex items-center gap-2 border-b border-line px-3 py-2.5">
              <Search size={14} className="text-muted" />
              <input
                ref={inputRef}
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setCursor(0);
                }}
                placeholder="Jump to a section or action…"
                className="w-full bg-transparent text-[13px] text-ink outline-none placeholder:text-muted"
              />
              <kbd className="rounded border border-line px-1 py-0.5 text-[10px] text-muted">esc</kbd>
            </div>
            <div className="max-h-[340px] overflow-y-auto p-1.5">
              {filtered.length === 0 && <div className="px-3 py-6 text-center text-[12px] text-muted">No matches</div>}
              {filtered.map((item, index) => (
                <button
                  key={item.to + item.label}
                  onMouseEnter={() => setCursor(index)}
                  onClick={() => {
                    navigate(item.to);
                    setPaletteOpen(false);
                  }}
                  className={cx(
                    "flex w-full items-center justify-between gap-2 rounded-lg px-2.5 py-2 text-left text-[12.5px]",
                    index === cursor ? "bg-accent/10 text-ink" : "text-ink-dim hover:bg-white/[0.03]",
                  )}
                >
                  <span>{item.label}</span>
                  <span className="text-[10.5px] text-muted">{item.hint}</span>
                </button>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function StatusRow({ label, ok }: { label: string; ok?: boolean }) {
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="text-muted">{label}</span>
      <span className={cx("flex items-center gap-1.5", ok ? "text-ok" : "text-warn")}>
        <span className={cx("h-1.5 w-1.5 rounded-full", ok ? "bg-ok" : "bg-warn")} />
        {ok ? "ready" : "missing"}
      </span>
    </div>
  );
}

export function BackendGate({ backend }: { backend?: BackendStatus | null }) {
  if (!backend || (backend.inference_ready && backend.compression_ready)) return null;
  return (
    <div className="mb-4 flex items-start gap-2.5 rounded-xl border border-warn/25 bg-warn/[0.05] px-3.5 py-2.5">
      <ShieldAlert size={15} className="mt-0.5 shrink-0 text-warn" />
      <div className="text-[12px] leading-relaxed text-ink-dim">
        <span className="font-medium text-ink">llama.cpp not fully detected.</span> {backend.message}
      </div>
    </div>
  );
}
