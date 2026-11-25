import { bytes, params } from "../lib/format";
import type { ModelRecord } from "../lib/types";
import { Select } from "./ui";

export function ModelSelect({
  models,
  value,
  onChange,
  placeholder = "Select a model…",
  filter,
  className,
}: {
  models: ModelRecord[];
  value: string | null;
  onChange: (id: string) => void;
  placeholder?: string;
  filter?: (model: ModelRecord) => boolean;
  className?: string;
}) {
  const options = models.filter((m) => (filter ? filter(m) : true));
  return (
    <Select value={value ?? ""} onChange={(e) => onChange(e.target.value)} className={className}>
      <option value="" disabled>
        {options.length ? placeholder : "No models in the library"}
      </option>
      {options.map((model) => (
        <option key={model.id} value={model.id}>
          {model.label} · {model.summary.label_quant} · {bytes(model.size_bytes)}
        </option>
      ))}
    </Select>
  );
}

export function ModelLine({ model }: { model: ModelRecord }) {
  return (
    <div className="flex flex-wrap items-center gap-2 text-[11.5px] text-muted">
      <span className="text-ink-dim">{model.summary.label_quant}</span>
      <span className="text-line-strong">•</span>
      <span className="mono-num">{bytes(model.size_bytes)}</span>
      <span className="text-line-strong">•</span>
      <span className="mono-num">{params(model.summary.parameter_count)} params</span>
      {model.summary.context_length && (
        <>
          <span className="text-line-strong">•</span>
          <span className="mono-num">{model.summary.context_length.toLocaleString()} ctx</span>
        </>
      )}
    </div>
  );
}
