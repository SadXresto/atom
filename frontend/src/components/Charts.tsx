import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { bytes } from "../lib/format";
import type { Snapshot } from "../lib/types";

const axisStyle = { fill: "#6f7789", fontSize: 10.5 };
const gridColor = "rgba(255,255,255,0.045)";

/** Compact binary units for axis ticks, so a label can never read "00M". */
export function compactBytes(value: number): string {
  if (!Number.isFinite(value)) return "\u2014";
  const abs = Math.abs(value);
  if (abs >= 1024 ** 4) return `${(value / 1024 ** 4).toFixed(1)} TiB`;
  if (abs >= 1024 ** 3) return `${(value / 1024 ** 3).toFixed(1)} GiB`;
  if (abs >= 1024 ** 2) return `${Math.round(value / 1024 ** 2)} MiB`;
  if (abs >= 1024) return `${Math.round(value / 1024)} KiB`;
  return `${Math.round(value)} B`;
}

const tooltipStyle = {
  background: "#0d1015",
  border: "1px solid #242a36",
  borderRadius: 10,
  fontSize: 11.5,
  color: "#e9ecf3",
  boxShadow: "0 18px 40px -24px rgba(0,0,0,0.9)",
};

export function MemoryChart({ history }: { history: Snapshot[] }) {
  const data = history.map((sample) => ({
    t: new Date(sample.ts).toLocaleTimeString(undefined, { hour12: false }).slice(3),
    system: sample.used_memory,
    atom: sample.process_rss,
    model: sample.model_process_rss ?? 0,
  }));
  return (
    <ResponsiveContainer width="100%" height={210}>
      <AreaChart data={data} margin={{ top: 8, right: 14, left: 2, bottom: 0 }}>
        <defs>
          <linearGradient id="sysFill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#7f9dff" stopOpacity={0.35} />
            <stop offset="100%" stopColor="#7f9dff" stopOpacity={0.02} />
          </linearGradient>
          <linearGradient id="modelFill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#4fd1c5" stopOpacity={0.4} />
            <stop offset="100%" stopColor="#4fd1c5" stopOpacity={0.02} />
          </linearGradient>
        </defs>
        <CartesianGrid stroke={gridColor} vertical={false} />
        <XAxis
          dataKey="t"
          tick={axisStyle}
          axisLine={false}
          tickLine={false}
          minTickGap={44}
          tickMargin={6}
          interval="preserveStartEnd"
        />
        <YAxis
          tick={axisStyle}
          axisLine={false}
          tickLine={false}
          width={62}
          tickMargin={4}
          allowDecimals
          domain={[0, "auto"]}
          tickFormatter={(v: number) => compactBytes(v)}
        />
        <Tooltip
          contentStyle={tooltipStyle}
          formatter={(value: number, name: string) => [compactBytes(value), name]}
          labelStyle={{ color: "#8a93a5" }}
        />
        <Legend
          verticalAlign="top"
          height={26}
          iconType="circle"
          iconSize={7}
          wrapperStyle={{ fontSize: 11, color: "#8a93a5" }}
        />
        <Area
          type="monotone"
          dataKey="system"
          name="System used"
          stroke="#7f9dff"
          strokeWidth={1.6}
          fill="url(#sysFill)"
          isAnimationActive={false}
        />
        <Area
          type="monotone"
          dataKey="model"
          name="llama-server RSS"
          stroke="#4fd1c5"
          strokeWidth={1.6}
          fill="url(#modelFill)"
          isAnimationActive={false}
        />
        <Area
          type="monotone"
          dataKey="atom"
          name="ATOM process"
          stroke="#efb42f"
          strokeWidth={1.2}
          fill="none"
          isAnimationActive={false}
        />
      </AreaChart>
    </ResponsiveContainer>
  );
}

export function CpuChart({ history }: { history: Snapshot[] }) {
  const data = history.map((sample) => ({
    t: new Date(sample.ts).toLocaleTimeString(undefined, { hour12: false }).slice(3),
    cpu: sample.system_cpu_percent,
  }));
  return (
    <ResponsiveContainer width="100%" height={150}>
      <AreaChart data={data} margin={{ top: 8, right: 14, left: 2, bottom: 0 }}>
        <defs>
          <linearGradient id="cpuFill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#4fd1c5" stopOpacity={0.32} />
            <stop offset="100%" stopColor="#4fd1c5" stopOpacity={0.02} />
          </linearGradient>
        </defs>
        <CartesianGrid stroke={gridColor} vertical={false} />
        <XAxis
          dataKey="t"
          tick={axisStyle}
          axisLine={false}
          tickLine={false}
          minTickGap={44}
          tickMargin={6}
          interval="preserveStartEnd"
        />
        <YAxis
          tick={axisStyle}
          axisLine={false}
          tickLine={false}
          width={52}
          tickMargin={4}
          domain={[0, 100]}
          ticks={[0, 25, 50, 75, 100]}
          tickFormatter={(v: number) => `${Math.round(v)}%`}
        />
        <Tooltip
          contentStyle={tooltipStyle}
          formatter={(v: number) => [`${v.toFixed(1)} %`, "System CPU"]}
          labelFormatter={(label: string) => `at ${label}`}
        />
        <Area
          type="monotone"
          dataKey="cpu"
          stroke="#4fd1c5"
          strokeWidth={1.5}
          fill="url(#cpuFill)"
          isAnimationActive={false}
        />
      </AreaChart>
    </ResponsiveContainer>
  );
}

export function SuiteBars({
  suites,
  labels,
}: {
  suites: Array<{ title: string; values: Array<number | null> }>;
  labels: string[];
}) {
  const palette = ["#7f9dff", "#4fd1c5", "#efb42f"];
  const data = suites.map((suite) => {
    const row: Record<string, string | number | null> = { title: suite.title };
    labels.forEach((label, i) => {
      row[label] = suite.values[i] === null ? null : Math.round((suite.values[i] as number) * 100);
    });
    return row;
  });
  return (
    <ResponsiveContainer width="100%" height={240}>
      <BarChart data={data} margin={{ top: 8, right: 14, left: 2, bottom: 0 }} barGap={3}>
        <CartesianGrid stroke={gridColor} vertical={false} />
        <XAxis dataKey="title" tick={{ ...axisStyle, fontSize: 10 }} axisLine={false} tickLine={false} interval={0} />
        <YAxis
          tick={axisStyle}
          axisLine={false}
          tickLine={false}
          width={52}
          tickMargin={4}
          domain={[0, 100]}
          ticks={[0, 25, 50, 75, 100]}
          tickFormatter={(v: number) => `${Math.round(v)}%`}
        />
        <Tooltip
          contentStyle={tooltipStyle}
          formatter={(value: number, name: string) => [`${value}% passed`, name]}
          cursor={{ fill: "rgba(255,255,255,0.03)" }}
        />
        <Legend wrapperStyle={{ fontSize: 11, color: "#8a93a5" }} iconType="circle" iconSize={7} />
        {labels.map((label, i) => (
          <Bar key={label} dataKey={label} fill={palette[i % palette.length]} radius={[4, 4, 0, 0]} maxBarSize={26}>
            {data.map((_, index) => (
              <Cell key={index} fill={palette[i % palette.length]} />
            ))}
          </Bar>
        ))}
      </BarChart>
    </ResponsiveContainer>
  );
}

export function SizeBars({ items }: { items: Array<{ label: string; value: number; color?: string }> }) {
  const max = Math.max(1, ...items.map((i) => i.value));
  return (
    <div className="space-y-2.5">
      {items.map((item) => (
        <div key={item.label}>
          <div className="mb-1 flex items-baseline justify-between gap-2 text-[11.5px]">
            <span className="truncate text-ink-dim">{item.label}</span>
            <span className="mono-num shrink-0 text-muted">{bytes(item.value)}</span>
          </div>
          <div className="h-[6px] overflow-hidden rounded-full bg-white/[0.05]">
            <div
              className="h-full rounded-full"
              style={{
                width: `${Math.max(1.5, (item.value / max) * 100)}%`,
                background: item.color ?? "linear-gradient(90deg,#7f9dff,#4fd1c5)",
              }}
            />
          </div>
        </div>
      ))}
    </div>
  );
}

export function LossChart({
  curve,
}: {
  curve: Array<{ epoch: number; train_loss: number; val_loss: number }>;
}) {
  const data = curve.map((point) => ({
    epoch: `e${point.epoch}`,
    train: Number(point.train_loss.toFixed(4)),
    val: Number(point.val_loss.toFixed(4)),
  }));
  if (data.length === 0) {
    return (
      <div className="flex h-[200px] items-center justify-center text-[12px] text-muted">
        No epochs recorded yet
      </div>
    );
  }
  return (
    <ResponsiveContainer width="100%" height={220}>
      <LineChart data={data} margin={{ top: 8, right: 14, left: 2, bottom: 0 }}>
        <CartesianGrid stroke={gridColor} vertical={false} />
        <XAxis dataKey="epoch" tick={axisStyle} axisLine={false} tickLine={false} />
        <YAxis
          tick={axisStyle}
          axisLine={false}
          tickLine={false}
          width={58}
          tickMargin={4}
          tickFormatter={(v: number) => v.toFixed(2)}
        />
        <Tooltip
          contentStyle={tooltipStyle}
          formatter={(value: number, name: string) => [value.toFixed(4), name === "train" ? "training loss" : "validation loss"]}
        />
        <Legend wrapperStyle={{ fontSize: 11, color: "#8a93a5" }} iconType="circle" iconSize={7} />
        <Line type="monotone" dataKey="train" stroke="#7f9dff" strokeWidth={1.8} dot={false} isAnimationActive={false} name="train" />
        <Line type="monotone" dataKey="val" stroke="#4fd1c5" strokeWidth={1.8} dot={false} isAnimationActive={false} name="val" />
      </LineChart>
    </ResponsiveContainer>
  );
}
