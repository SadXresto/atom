import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
} from "react";
import { AlertTriangle, CheckCircle2, ChevronRight, Copy, Info, Loader2, X } from "lucide-react";

export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}

/* ---------------------------------- toasts --------------------------------- */

type Tone = "ok" | "error" | "info";
interface ToastItem {
  id: string;
  message: string;
  tone: Tone;
}
let toastListeners: Array<(item: ToastItem) => void> = [];

export function toast(message: string, tone: Tone = "info") {
  const item: ToastItem = { id: Math.random().toString(36).slice(2), message, tone };
  toastListeners.forEach((listener) => listener(item));
}

export function ToastHost() {
  const [items, setItems] = useState<ToastItem[]>([]);
  useEffect(() => {
    const listener = (item: ToastItem) => {
      setItems((prev) => [...prev, item]);
      window.setTimeout(() => setItems((prev) => prev.filter((i) => i.id !== item.id)), 5000);
    };
    toastListeners.push(listener);
    return () => {
      toastListeners = toastListeners.filter((l) => l !== listener);
    };
  }, []);

  return (
    <div className="pointer-events-none fixed bottom-5 right-5 z-50 flex w-[360px] flex-col gap-2">
      {items.map((item) => (
        <div
          key={item.id}
          className={cx(
            "pointer-events-auto flex items-start gap-2.5 rounded-xl border px-3.5 py-3 text-[13px] shadow-2xl backdrop-blur fade-in",
            item.tone === "ok" && "border-ok/30 bg-[#0f1a16]/95 text-[#c8f5df]",
            item.tone === "error" && "border-bad/35 bg-[#1d1012]/95 text-[#ffc9c9]",
            item.tone === "info" && "border-line-strong bg-raised/95 text-ink-dim",
          )}
        >
          {item.tone === "ok" ? (
            <CheckCircle2 size={15} className="mt-0.5 shrink-0 text-ok" />
          ) : item.tone === "error" ? (
            <AlertTriangle size={15} className="mt-0.5 shrink-0 text-bad" />
          ) : (
            <Info size={15} className="mt-0.5 shrink-0 text-accent" />
          )}
          <span className="leading-relaxed">{item.message}</span>
        </div>
      ))}
    </div>
  );
}

/* --------------------------------- buttons --------------------------------- */

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "ghost" | "danger";
  size?: "sm" | "md";
  busy?: boolean;
  icon?: ReactNode;
};

export function Button({
  variant = "ghost",
  size = "md",
  busy,
  icon,
  className,
  children,
  disabled,
  ...rest
}: ButtonProps) {
  return (
    <button
      {...rest}
      disabled={disabled || busy}
      className={cx(
        "btn",
        variant === "primary" && "btn-primary",
        variant === "ghost" && "btn-ghost",
        variant === "danger" && "btn-danger",
        size === "sm" && "px-2.5 py-1.5 text-[12px]",
        className,
      )}
    >
      {busy ? <Loader2 size={14} className="animate-spin" /> : icon}
      {children}
    </button>
  );
}

export function Spinner({ size = 16 }: { size?: number }) {
  return <Loader2 size={size} className="animate-spin text-muted" />;
}

export function CopyButton({ text, label = "Copy" }: { text: string; label?: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <Button
      size="sm"
      icon={<Copy size={12} />}
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(text);
          setCopied(true);
          window.setTimeout(() => setCopied(false), 1400);
        } catch {
          toast("Clipboard is unavailable in this browser context", "error");
        }
      }}
    >
      {copied ? "Copied" : label}
    </Button>
  );
}

/* ---------------------------------- layout --------------------------------- */

export function Panel({
  children,
  className,
  title,
  subtitle,
  actions,
  icon,
  padded = true,
}: {
  children?: ReactNode;
  className?: string;
  title?: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  icon?: ReactNode;
  padded?: boolean;
}) {
  return (
    <section className={cx("panel", className)}>
      {(title || actions) && (
        <header className="flex items-start justify-between gap-3 border-b border-line px-4 py-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-[13px] font-semibold text-ink">
              {icon && <span className="text-muted">{icon}</span>}
              <span className="truncate">{title}</span>
            </div>
            {subtitle && <p className="mt-0.5 text-[12px] leading-relaxed text-muted">{subtitle}</p>}
          </div>
          {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
        </header>
      )}
      <div className={padded ? "p-4" : undefined}>{children}</div>
    </section>
  );
}

export function PageHeader({
  title,
  description,
  actions,
  eyebrow,
}: {
  title: string;
  description?: ReactNode;
  actions?: ReactNode;
  eyebrow?: string;
}) {
  return (
    <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0">
        {eyebrow && <div className="label mb-1.5">{eyebrow}</div>}
        <h1 className="text-[22px] font-semibold tracking-[-0.01em] text-ink">{title}</h1>
        {description && <p className="mt-1 max-w-3xl text-[13px] leading-relaxed text-muted">{description}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

export function Stat({
  label,
  value,
  sub,
  tone = "default",
  icon,
  mono = true,
  hint,
}: {
  label: string;
  value: ReactNode;
  sub?: ReactNode;
  tone?: "default" | "ok" | "warn" | "bad" | "accent";
  icon?: ReactNode;
  mono?: boolean;
  hint?: string;
}) {
  const color =
    tone === "ok"
      ? "text-ok"
      : tone === "warn"
        ? "text-warn"
        : tone === "bad"
          ? "text-bad"
          : tone === "accent"
            ? "text-accent"
            : "text-ink";
  return (
    <div className="panel-flat px-3.5 py-3" title={hint}>
      <div className="flex items-center justify-between gap-2">
        <span className="label">{label}</span>
        {icon && <span className="text-muted">{icon}</span>}
      </div>
      <div className={cx("mt-1.5 text-[20px] font-semibold leading-none tracking-[-0.01em]", color, mono && "mono-num")}>
        {value}
      </div>
      {sub && <div className="mt-1.5 text-[11.5px] leading-snug text-muted">{sub}</div>}
    </div>
  );
}

export function Badge({
  children,
  tone = "neutral",
  className,
}: {
  children: ReactNode;
  tone?: "neutral" | "ok" | "warn" | "bad" | "accent" | "info";
  className?: string;
}) {
  const map = {
    neutral: "border-line text-ink-dim bg-white/[0.03]",
    ok: "border-ok/30 text-ok bg-ok/10",
    warn: "border-warn/30 text-warn bg-warn/10",
    bad: "border-bad/30 text-bad bg-bad/10",
    accent: "border-accent/35 text-accent bg-accent/10",
    info: "border-accent-2/25 text-accent-2 bg-accent-2/10",
  } as const;
  return <span className={cx("chip", map[tone], className)}>{children}</span>;
}

export function Progress({
  value,
  tone = "accent",
  height = 6,
}: {
  value: number;
  tone?: "accent" | "ok" | "warn" | "bad";
  height?: number;
}) {
  const pct = Math.max(0, Math.min(1, value));
  const bg =
    tone === "ok"
      ? "bg-ok"
      : tone === "warn"
        ? "bg-warn"
        : tone === "bad"
          ? "bg-bad"
          : "bg-gradient-to-r from-accent to-accent-2";
  return (
    <div className="w-full overflow-hidden rounded-full bg-white/[0.06]" style={{ height }}>
      <div
        className={cx("h-full rounded-full transition-[width] duration-500 ease-out", bg)}
        style={{ width: `${pct * 100}%` }}
      />
    </div>
  );
}

/* ---------------------------------- inputs --------------------------------- */

export function Field({
  label,
  hint,
  children,
  className,
  htmlFor,
}: {
  label: ReactNode;
  hint?: ReactNode;
  children: ReactNode;
  className?: string;
  htmlFor?: string;
}) {
  return (
    <label htmlFor={htmlFor} className={cx("block", className)}>
      <span className="mb-1.5 flex items-center justify-between gap-2">
        <span className="text-[12px] font-medium text-ink-dim">{label}</span>
        {hint && <span className="text-[11px] text-muted">{hint}</span>}
      </span>
      {children}
    </label>
  );
}

export function TextInput({ className, ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...rest} className={cx("input", className)} />;
}

export function Select({
  className,
  children,
  ...rest
}: SelectHTMLAttributes<HTMLSelectElement> & { children: ReactNode }) {
  return (
    <select {...rest} className={cx("input appearance-none pr-8", className)}>
      {children}
    </select>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  description,
  disabled,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: ReactNode;
  description?: ReactNode;
  disabled?: boolean;
}) {
  return (
    <div className="flex items-start justify-between gap-4 py-2">
      <div className="min-w-0">
        <div className="text-[12.5px] font-medium text-ink-dim">{label}</div>
        {description && <div className="mt-0.5 text-[11.5px] leading-relaxed text-muted">{description}</div>}
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={cx(
          "relative mt-0.5 h-[20px] w-[36px] shrink-0 rounded-full border transition-colors",
          checked ? "border-accent/60 bg-accent/30" : "border-line-strong bg-white/[0.04]",
          disabled && "opacity-50",
        )}
      >
        <span
          className={cx(
            "absolute top-[2px] h-[14px] w-[14px] rounded-full transition-all",
            checked ? "left-[18px] bg-accent" : "left-[2px] bg-[#5d6678]",
          )}
        />
      </button>
    </div>
  );
}

export function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
  size = "md",
}: {
  options: Array<{ value: T; label: string; count?: number }>;
  value: T;
  onChange: (next: T) => void;
  size?: "sm" | "md";
}) {
  return (
    <div className="inline-flex items-center gap-1 rounded-lg border border-line bg-black/20 p-1">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          onClick={() => onChange(option.value)}
          className={cx(
            "rounded-md font-medium transition-colors",
            size === "sm" ? "px-2 py-1 text-[11.5px]" : "px-2.5 py-1.5 text-[12.5px]",
            value === option.value
              ? "bg-white/[0.08] text-ink shadow-sm"
              : "text-muted hover:text-ink-dim",
          )}
        >
          {option.label}
          {option.count !== undefined && (
            <span className="ml-1.5 text-[10.5px] text-muted mono-num">{option.count}</span>
          )}
        </button>
      ))}
    </div>
  );
}

/* ---------------------------------- modal ---------------------------------- */

export function Modal({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  width = "max-w-lg",
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  width?: string;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-black/70 p-4 pt-[10vh] backdrop-blur-sm">
      <div
        role="dialog"
        aria-modal="true"
        className={cx("panel w-full fade-in", width)}
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-start justify-between gap-4 border-b border-line px-4 py-3">
          <div>
            <h2 className="text-[14px] font-semibold text-ink">{title}</h2>
            {description && <p className="mt-0.5 text-[12px] leading-relaxed text-muted">{description}</p>}
          </div>
          <button onClick={onClose} className="rounded p-1 text-muted hover:bg-white/5 hover:text-ink" aria-label="Close">
            <X size={15} />
          </button>
        </header>
        {children && <div className="px-4 py-4">{children}</div>}
        {footer && <footer className="flex justify-end gap-2 border-t border-line px-4 py-3">{footer}</footer>}
      </div>
    </div>
  );
}

export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  body,
  confirmLabel = "Confirm",
  confirmWord,
  danger,
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: string;
  body: ReactNode;
  confirmLabel?: string;
  confirmWord?: string;
  danger?: boolean;
}) {
  const [typed, setTyped] = useState("");
  useEffect(() => {
    if (open) setTyped("");
  }, [open]);
  const ready = !confirmWord || typed.trim() === confirmWord;
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant={danger ? "danger" : "primary"} disabled={!ready} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </>
      }
    >
      <div className="space-y-3 text-[13px] leading-relaxed text-ink-dim">
        {body}
        {confirmWord && (
          <Field label={<>Type <span className="mono-num text-ink">{confirmWord}</span> to confirm</>}>
            <TextInput value={typed} onChange={(e) => setTyped(e.target.value)} placeholder={confirmWord} />
          </Field>
        )}
      </div>
    </Modal>
  );
}

/* -------------------------------- misc bits -------------------------------- */

export function EmptyState({
  icon,
  title,
  description,
  action,
  compact,
}: {
  icon?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  compact?: boolean;
}) {
  return (
    <div className={cx("flex flex-col items-center justify-center text-center", compact ? "py-8" : "py-14")}>
      {icon && (
        <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-xl border border-line bg-white/[0.03] text-muted">
          {icon}
        </div>
      )}
      <div className="text-[13.5px] font-medium text-ink">{title}</div>
      {description && <p className="mt-1 max-w-md text-[12.5px] leading-relaxed text-muted">{description}</p>}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

export function KV({ label, value, mono, hint }: { label: ReactNode; value: ReactNode; mono?: boolean; hint?: string }) {
  return (
    <div className="flex items-baseline justify-between gap-4 border-b border-line/60 py-1.5 last:border-0" title={hint}>
      <span className="shrink-0 text-[11.5px] text-muted">{label}</span>
      <span className={cx("min-w-0 truncate text-right text-[12px] text-ink-dim", mono && "mono-num")}>{value}</span>
    </div>
  );
}

export function Banner({
  tone = "warn",
  title,
  children,
  action,
}: {
  tone?: "warn" | "bad" | "info" | "ok";
  title?: ReactNode;
  children?: ReactNode;
  action?: ReactNode;
}) {
  const map = {
    warn: "border-warn/25 bg-warn/[0.06]",
    bad: "border-bad/25 bg-bad/[0.06]",
    info: "border-accent/25 bg-accent/[0.05]",
    ok: "border-ok/25 bg-ok/[0.05]",
  } as const;
  const iconColor = { warn: "text-warn", bad: "text-bad", info: "text-accent", ok: "text-ok" } as const;
  return (
    <div className={cx("flex items-start gap-3 rounded-xl border px-3.5 py-3", map[tone])}>
      <span className={cx("mt-0.5 shrink-0", iconColor[tone])}>
        {tone === "ok" ? <CheckCircle2 size={15} /> : tone === "info" ? <Info size={15} /> : <AlertTriangle size={15} />}
      </span>
      <div className="min-w-0 flex-1">
        {title && <div className="text-[12.5px] font-medium text-ink">{title}</div>}
        {children && <div className="mt-0.5 text-[12px] leading-relaxed text-ink-dim">{children}</div>}
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}

export function DisclosureRow({
  title,
  meta,
  children,
  defaultOpen,
  right,
}: {
  title: ReactNode;
  meta?: ReactNode;
  children?: ReactNode;
  defaultOpen?: boolean;
  right?: ReactNode;
}) {
  const [open, setOpen] = useState(Boolean(defaultOpen));
  return (
    <div className="border-b border-line last:border-0">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-3 px-1 py-2.5 text-left hover:bg-white/[0.015]"
      >
        <ChevronRight size={14} className={cx("shrink-0 text-muted transition-transform", open && "rotate-90")} />
        <span className="min-w-0 flex-1 text-[12.5px] text-ink-dim">{title}</span>
        {meta && <span className="shrink-0 text-[11.5px] text-muted">{meta}</span>}
        {right}
      </button>
      {open && children && <div className="px-6 pb-3">{children}</div>}
    </div>
  );
}

/* ------------------------------ theme helpers ------------------------------ */

const DensityContext = createContext<"comfortable" | "compact">("comfortable");
export const useDensity = () => useContext(DensityContext);
export function DensityProvider({ children }: { children: ReactNode }) {
  const value = useMemo(() => "comfortable" as const, []);
  return <DensityContext.Provider value={value}>{children}</DensityContext.Provider>;
}
