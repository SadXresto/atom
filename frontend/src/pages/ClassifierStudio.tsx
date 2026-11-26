import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  ArrowLeft,
  BarChart3,
  Boxes,
  CheckCircle2,
  Download,
  FileSpreadsheet,
  Gauge,
  Play,
  ScanSearch,
  Sparkles,
  Table2,
  TriangleAlert,
  Upload,
} from "lucide-react";
import { api } from "../lib/api";
import { bytes, dateTime, duration, number, percent } from "../lib/format";
import type {
  ClassifierMetrics,
  ClassifierReport,
  CsvProfile,
  MicroProject,
} from "../lib/types";
import { LossChart } from "../components/Charts";
import { JobConsole } from "../components/JobConsole";
import { useMicroProject } from "../components/MicroBits";
import {
  Badge,
  Banner,
  Button,
  EmptyState,
  Field,
  KV,
  PageHeader,
  Panel,
  Progress,
  Select,
  TextInput,
  cx,
  toast,
} from "../components/ui";

const SAMPLE_CSV = `temperature,humidity,pressure,label
21.4,58,1009,comfortable
27.8,71,1004,warm
16.2,44,1014,cool
...`;

export default function ClassifierStudio() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { data, error, reload } = useMicroProject(id ?? null);
  const project = data?.project;

  const [path, setPath] = useState("");
  const [profile, setProfile] = useState<CsvProfile | null>(null);
  const [target, setTarget] = useState("");
  const [features, setFeatures] = useState<string[]>([]);
  const [valRatio, setValRatio] = useState(0.15);
  const [testRatio, setTestRatio] = useState(0.15);
  const [seed, setSeed] = useState(1337);
  const [busy, setBusy] = useState<string | null>(null);
  const [jobId, setJobId] = useState<string | null>(null);
  const [settings, setSettings] = useState({
    epochs: 60,
    learning_rate: 0.25,
    batch: 32,
    l2: 0.0001,
    seed: 1337,
  });

  const record = project?.classification ?? null;
  const dataset = record?.dataset ?? null;
  const report = record?.training?.report ?? null;

  useEffect(() => {
    if (!dataset) return;
    setTarget(dataset.spec.target);
    setFeatures(dataset.spec.features);
    setValRatio(dataset.spec.val_ratio);
    setTestRatio(dataset.spec.test_ratio);
    setSeed(dataset.spec.seed);
  }, [dataset?.stored_path]);

  const columns = profile?.columns ?? dataset?.profile.columns ?? [];
  const numericColumns = useMemo(() => columns.filter((c) => c.kind === "numeric"), [columns]);
  const categoricalColumns = useMemo(() => columns.filter((c) => c.kind === "categorical"), [columns]);

  const inspect = async () => {
    if (!path.trim()) {
      toast("Enter the path to a CSV file", "error");
      return;
    }
    setBusy("profile");
    try {
      const result = await api<{ profile: CsvProfile }>(
        `/micro/projects/${id}/classification/profile`,
        { method: "POST", body: JSON.stringify({ path: path.trim() }) },
      );
      setProfile(result.profile);
      const guessTarget =
        [...result.profile.columns].reverse().find((c) => c.kind === "categorical" && c.unique >= 2)?.name ??
        result.profile.columns[result.profile.columns.length - 1]?.name ??
        "";
      setTarget(guessTarget);
      setFeatures(
        result.profile.columns
          .filter((c) => c.name !== guessTarget && c.missing < result.profile.rows * 0.5)
          .slice(0, 8)
          .map((c) => c.name),
      );
      toast(`Profiled ${result.profile.rows} rows and ${result.profile.columns.length} columns`, "ok");
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(null);
    }
  };

  const useDataset = async () => {
    if (!target) {
      toast("Choose the target label column", "error");
      return;
    }
    if (!features.length) {
      toast("Choose at least one input feature", "error");
      return;
    }
    setBusy("dataset");
    try {
      await api<{ project: MicroProject }>(`/micro/projects/${id}/classification/dataset`, {
        method: "POST",
        body: JSON.stringify({
          path: (profile?.path ?? path).trim(),
          target,
          features,
          val_ratio: valRatio,
          test_ratio: testRatio,
          seed,
        }),
      });
      toast("Dataset validated and stored in the project", "ok");
      setProfile(null);
      setPath("");
      void reload();
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(null);
    }
  };

  const train = async () => {
    setBusy("train");
    try {
      const result = await api<{ job_id: string }>(
        `/micro/projects/${id}/classification/train`,
        { method: "POST", body: JSON.stringify(settings) },
      );
      setJobId(result.job_id);
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(null);
    }
  };

  const runExport = async () => {
    setBusy("export");
    try {
      await api(`/micro/projects/${id}/classification/export`, { method: "POST" });
      toast("Classifier bundle exported and validated", "ok");
      void reload();
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setBusy(null);
    }
  };

  if (!id) return <EmptyState title="No project selected" />;
  if (error) {
    return (
      <Banner tone="bad" title="Could not load this project">
        {error.message}
      </Banner>
    );
  }
  if (!project) return <div className="h-40" />;

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow={
          <button
            onClick={() => navigate(`/micro/${project.id}`)}
            className="flex items-center gap-1 hover:text-ink-dim"
          >
            <ArrowLeft size={12} /> {project.name}
          </button>
        }
        title="Classification workspace"
        description="Train a tabular classifier on your own CSV. Column types, missing values, class balance and every score below are computed from the file and the trained weights."
        actions={
          <Button
            icon={<Boxes size={13} />}
            onClick={() => navigate(`/micro/${project.id}`)}
            size="sm"
          >
            Generative workflow
          </Button>
        }
      />

      <div className="mb-4 grid gap-3 sm:grid-cols-4">
        <Mini label="Rows" value={dataset ? number(dataset.profile.rows) : "—"} />
        <Mini label="Design features" value={dataset ? number(dataset.design_features) : "—"} />
        <Mini label="Classes" value={dataset ? number(dataset.classes.length) : "—"} />
        <Mini
          label="Test accuracy"
          value={report ? percent(report.test.accuracy * 100, 1) : "—"}
          tone={report ? "ok" : undefined}
        />
      </div>

      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.35fr)]">
        <div className="space-y-4">
          <Panel
            title="1 · CSV dataset"
            subtitle="UTF-8 CSV with a header row. The source file is copied into the project and never modified."
            icon={<FileSpreadsheet size={14} />}
            actions={dataset ? <Badge tone="ok">imported</Badge> : <Badge tone="warn">required</Badge>}
          >
            <div className="flex flex-col gap-2 sm:flex-row">
              <TextInput
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder="~/datasets/sensor-readings.csv"
                className="flex-1"
                onKeyDown={(e) => {
                  if (e.key === "Enter") void inspect();
                }}
              />
              <Button
                variant="primary"
                busy={busy === "profile"}
                onClick={inspect}
                icon={<ScanSearch size={13} />}
              >
                Inspect CSV
              </Button>
            </div>
            <div className="mt-2 text-[11px] leading-relaxed text-muted">
              Paths must resolve inside the dataset roots configured in Settings. Files above 20 MiB or
              50,000 rows are rejected or truncated with an explicit warning.
            </div>

            {dataset ? (
              <div className="mt-4 space-y-3">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge tone="accent">{dataset.spec.target}</Badge>
                  <Badge>{dataset.spec.features.length} features</Badge>
                  <Badge>{dataset.profile.rows} rows</Badge>
                  <Badge>{dataset.profile.columns.length} columns</Badge>
                </div>
                <div className="grid gap-2 sm:grid-cols-3">
                  <Mini label="Train rows" value={number(dataset.split.train)} />
                  <Mini label="Validation" value={number(dataset.split.validation)} />
                  <Mini label="Test" value={number(dataset.split.test)} />
                </div>
                {dataset.duplicate_groups > 0 && (
                  <Banner tone="info" title="Leakage prevention">
                    {dataset.duplicate_rows} duplicate rows ({dataset.duplicate_groups} groups) were kept
                    inside a single split, so the same row cannot appear in training and test.
                  </Banner>
                )}
                {dataset.missing_values > 0 && (
                  <div className="text-[11.5px] leading-relaxed text-muted">
                    {dataset.missing_values} missing numeric values were imputed with the training mean.
                  </div>
                )}
                {dataset.notes.map((note, index) => (
                  <div
                    key={index}
                    className="rounded-lg border border-line bg-black/20 px-2.5 py-2 text-[11px] leading-relaxed text-muted"
                  >
                    {note}
                  </div>
                ))}
                <div className="text-[10.5px] text-muted">
                  A normalized copy (<span className="font-mono">{dataset.stored_path.split(/[\\/]/).pop()}</span>) is
                  stored inside this project; the source CSV is untouched.
                </div>
              </div>
            ) : (
              <div className="mt-3 rounded-lg border border-line bg-black/20 px-3 py-2 text-[11px] leading-relaxed text-muted">
                Expected shape:
                <pre className="mt-1 overflow-x-auto font-mono text-[10.5px] text-ink-dim">{SAMPLE_CSV}</pre>
              </div>
            )}
          </Panel>

          {profile && (
            <Panel
              title="2 · Columns and roles"
              subtitle="Types were inferred from the values, not the header names."
              icon={<Table2 size={14} />}
            >
              <div className="max-h-[280px] overflow-auto rounded-lg border border-line">
                <table className="w-full text-[11.5px]">
                  <thead className="sticky top-0 bg-surface">
                    <tr className="border-b border-line">
                      <th className="table-head px-2 py-1.5 text-left">Column</th>
                      <th className="table-head px-2 py-1.5 text-left">Type</th>
                      <th className="table-head px-2 py-1.5 text-right">Missing</th>
                      <th className="table-head px-2 py-1.5 text-right">Invalid</th>
                      <th className="table-head px-2 py-1.5 text-right">Unique</th>
                      <th className="table-head px-2 py-1.5 text-left">Examples</th>
                    </tr>
                  </thead>
                  <tbody>
                    {profile.columns.map((column) => (
                      <tr key={column.name} className="border-b border-line/60">
                        <td className="px-2 py-1.5 font-mono text-[11px] text-ink-dim">{column.name}</td>
                        <td className="px-2 py-1.5">
                          <Badge tone={column.kind === "numeric" ? "info" : "neutral"}>{column.kind}</Badge>
                        </td>
                        <td
                          className={cx(
                            "mono-num px-2 py-1.5 text-right",
                            column.missing > 0 ? "text-warn" : "text-muted",
                          )}
                        >
                          {column.missing}
                        </td>
                        <td className="mono-num px-2 py-1.5 text-right text-muted">{column.invalid}</td>
                        <td className="mono-num px-2 py-1.5 text-right text-muted">{column.unique}</td>
                        <td className="max-w-[220px] truncate px-2 py-1.5 text-muted">
                          {column.examples.join(", ") || "—"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              <div className="mt-3 grid gap-3 sm:grid-cols-2">
                <Field label="Target label" hint="the class to predict">
                  <Select value={target} onChange={(e) => setTarget(e.target.value)}>
                    <option value="">Choose a column…</option>
                    {profile.columns.map((column) => (
                      <option key={column.name} value={column.name}>
                        {column.name} ({column.unique} values, {column.kind})
                      </option>
                    ))}
                  </Select>
                </Field>
                <div>
                  <div className="label mb-1.5">Input features ({features.length})</div>
                  <div className="max-h-[132px] space-y-1 overflow-y-auto rounded-lg border border-line bg-black/20 p-2">
                    {[...numericColumns, ...categoricalColumns].map((column) => {
                      const disabled = column.name === target;
                      const checked = features.includes(column.name);
                      return (
                        <label
                          key={column.name}
                          className={cx(
                            "flex items-center gap-2 text-[11.5px]",
                            disabled ? "opacity-40" : "cursor-pointer",
                          )}
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            disabled={disabled}
                            onChange={(e) =>
                              setFeatures((prev) =>
                                e.target.checked
                                  ? [...prev, column.name]
                                  : prev.filter((name) => name !== column.name),
                              )
                            }
                            className="h-3.5 w-3.5 accent-[#7f9dff]"
                          />
                          <span className="text-ink-dim">{column.name}</span>
                          <span className="ml-auto text-[10px] text-muted">{column.kind}</span>
                        </label>
                      );
                    })}
                  </div>
                </div>
              </div>

              <div className="mt-3 grid gap-3 sm:grid-cols-4">
                <Field label="Validation ratio">
                  <TextInput
                    type="number"
                    step="0.05"
                    min="0"
                    max="0.5"
                    value={valRatio}
                    onChange={(e) => setValRatio(Number(e.target.value))}
                  />
                </Field>
                <Field label="Test ratio">
                  <TextInput
                    type="number"
                    step="0.05"
                    min="0"
                    max="0.5"
                    value={testRatio}
                    onChange={(e) => setTestRatio(Number(e.target.value))}
                  />
                </Field>
                <Field label="Split seed">
                  <TextInput type="number" value={seed} onChange={(e) => setSeed(Number(e.target.value))} />
                </Field>
                <div className="flex items-end">
                  <Button
                    variant="primary"
                    className="w-full"
                    busy={busy === "dataset"}
                    onClick={useDataset}
                    icon={<CheckCircle2 size={13} />}
                  >
                    Use dataset
                  </Button>
                </div>
              </div>
              {profile.warnings.length > 0 && (
                <div className="mt-3 space-y-1">
                  {profile.warnings.map((warning, index) => (
                    <div key={index} className="flex items-start gap-1.5 text-[11px] text-warn">
                      <TriangleAlert size={12} className="mt-0.5 shrink-0" />
                      {warning}
                    </div>
                  ))}
                </div>
              )}
            </Panel>
          )}

          {dataset && (
            <Panel
              title="3 · Class distribution"
              subtitle="Counts from the imported rows. Class imbalance is visible here and in the metric support column."
              icon={<BarChart3 size={14} />}
            >
              <div className="space-y-2">
                {dataset.classes.map((item) => (
                  <div key={item.label}>
                    <div className="mb-1 flex items-baseline justify-between gap-2 text-[11.5px]">
                      <span className="truncate text-ink-dim">{item.label}</span>
                      <span className="mono-num shrink-0 text-muted">
                        {number(item.count)} · {percent(item.share * 100, 1)}
                      </span>
                    </div>
                    <Progress
                      value={item.share}
                      height={6}
                      tone={item.share < 0.1 ? "warn" : "accent"}
                    />
                  </div>
                ))}
              </div>
            </Panel>
          )}
        </div>

        <div className="space-y-4">
          <Panel
            title="4 · Train the classifier"
            subtitle="Multinomial logistic regression trained with mini-batch SGD in this process. The test split is never used for fitting or early stopping."
            icon={<Sparkles size={14} />}
            actions={report ? <Badge tone="ok">trained</Badge> : <Badge tone="neutral">not trained</Badge>}
          >
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              <Field label="Epochs">
                <TextInput
                  type="number"
                  value={settings.epochs}
                  onChange={(e) => setSettings({ ...settings, epochs: Number(e.target.value) })}
                />
              </Field>
              <Field label="Learning rate">
                <TextInput
                  type="number"
                  step="0.01"
                  value={settings.learning_rate}
                  onChange={(e) => setSettings({ ...settings, learning_rate: Number(e.target.value) })}
                />
              </Field>
              <Field label="Batch size">
                <TextInput
                  type="number"
                  value={settings.batch}
                  onChange={(e) => setSettings({ ...settings, batch: Number(e.target.value) })}
                />
              </Field>
              <Field label="L2 penalty">
                <TextInput
                  type="number"
                  step="0.0001"
                  value={settings.l2}
                  onChange={(e) => setSettings({ ...settings, l2: Number(e.target.value) })}
                />
              </Field>
              <Field label="Seed">
                <TextInput
                  type="number"
                  value={settings.seed}
                  onChange={(e) => setSettings({ ...settings, seed: Number(e.target.value) })}
                />
              </Field>
              <div className="flex items-end">
                <Button
                  variant="primary"
                  className="w-full"
                  busy={busy === "train"}
                  disabled={!dataset}
                  onClick={train}
                  icon={<Play size={13} />}
                >
                  Start training
                </Button>
              </div>
            </div>
            {!dataset && (
              <div className="mt-3">
                <Banner tone="warn" title="Import and validate a dataset first">
                  The trainer consumes the stored split, so the metrics are reproducible from the same
                  seed.
                </Banner>
              </div>
            )}
            {jobId && (
              <div className="mt-3 rounded-xl border border-line bg-black/20 p-3">
                <JobConsole
                  jobId={jobId}
                  onFinished={(job) => {
                    void reload();
                    if (job.status === "succeeded") toast("Classifier trained", "ok");
                    setJobId(null);
                  }}
                />
              </div>
            )}
            {record?.training && (
              <div className="mt-3 grid gap-2 sm:grid-cols-3">
                <Mini label="Checkpoint" value={bytes(record.training.model_bytes)} />
                <Mini label="Parameters" value={number(record.training.report.parameters)} />
                <Mini label="Trained" value={dateTime(record.training.finished_at)} />
              </div>
            )}
          </Panel>

          {report ? (
            <Report report={report} />
          ) : (
            <Panel>
              <EmptyState
                icon={<Gauge size={17} />}
                title="No metrics yet"
                description="Train the classifier to see measured accuracy, macro precision/recall/F1 and the confusion matrix on the held-out test split."
              />
            </Panel>
          )}

          {record?.training && (
            <Panel
              title="5 · Export"
              subtitle="Writes the checkpoint, a C header with the same weights, a self-test, a README and a SHA-256 manifest. The JSON is reloaded and re-scored before success is reported."
              icon={<Download size={14} />}
              actions={
                <Button
                  size="sm"
                  variant="primary"
                  busy={busy === "export"}
                  onClick={runExport}
                  icon={<Upload size={12} />}
                >
                  Export bundle
                </Button>
              }
            >
              {record.export ? (
                <div className="space-y-3">
                  <div className="flex flex-wrap items-center gap-2 text-[11.5px] text-muted">
                    <Badge tone="ok">validated</Badge>
                    <span className="font-mono text-[10.5px]">
                      {record.export.dir.split(/[\\/]/).slice(-2).join("/")}
                    </span>
                    <span>· {bytes(record.export.total_bytes)}</span>
                  </div>
                  <div className="overflow-x-auto">
                    <table className="w-full text-[11.5px]">
                      <thead>
                        <tr className="border-b border-line">
                          <th className="table-head px-2 py-1.5 text-left">File</th>
                          <th className="table-head px-2 py-1.5 text-right">Bytes</th>
                          <th className="table-head px-2 py-1.5 text-left">SHA-256</th>
                        </tr>
                      </thead>
                      <tbody>
                        {record.export.files.map((file) => (
                          <tr key={file.name} className="border-b border-line/60">
                            <td className="px-2 py-1.5 font-mono text-[10.5px] text-ink-dim">{file.name}</td>
                            <td className="mono-num px-2 py-1.5 text-right text-muted">{file.bytes}</td>
                            <td className="px-2 py-1.5 font-mono text-[10px] text-muted">
                              {file.sha256.slice(0, 12)}…
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                  <div className="text-[11px] leading-relaxed text-ink-dim">{record.export.validation}</div>
                  {record.export.compile_check && (
                    <div className="rounded-lg border border-line bg-black/25 px-2.5 py-2 font-mono text-[10.5px] leading-relaxed text-muted">
                      {record.export.compile_check}
                    </div>
                  )}
                </div>
              ) : (
                <div className="text-[11.5px] leading-relaxed text-muted">
                  The export directory is created inside the project. Existing checkpoints are preserved; a
                  new export overwrites only its own bundle.
                </div>
              )}
            </Panel>
          )}
        </div>
      </div>
    </div>
  );
}

function Report({ report }: { report: ClassifierReport }) {
  const [split, setSplit] = useState<"test" | "validation" | "train">("test");
  const metrics: ClassifierMetrics =
    split === "test" ? report.test : split === "validation" ? report.validation : report.train;
  return (
    <Panel
      title="Measured evaluation"
      subtitle="Every number below comes from predictions of the trained weights on the selected split."
      icon={<Gauge size={14} />}
      actions={
        <div className="flex items-center gap-1 rounded-lg border border-line p-0.5">
          {(["test", "validation", "train"] as const).map((key) => (
            <button
              key={key}
              onClick={() => setSplit(key)}
              className={cx(
                "rounded-md px-2 py-1 text-[10.5px] capitalize transition-colors",
                split === key ? "bg-accent/15 text-accent" : "text-muted hover:text-ink-dim",
              )}
            >
              {key}
            </button>
          ))}
        </div>
      }
    >
      <div className="grid gap-2 sm:grid-cols-3 lg:grid-cols-6">
        <Mini label="Accuracy" value={percent(metrics.accuracy * 100, 2)} tone="ok" />
        <Mini label="Macro precision" value={percent(metrics.macro_precision * 100, 2)} />
        <Mini label="Macro recall" value={percent(metrics.macro_recall * 100, 2)} />
        <Mini label="Macro F1" value={percent(metrics.macro_f1 * 100, 2)} />
        <Mini label="Weighted F1" value={percent(metrics.weighted_f1 * 100, 2)} />
        <Mini label="Log loss" value={metrics.log_loss.toFixed(4)} />
      </div>

      <div className="mt-4">
        <div className="label mb-2">Training curve · measured per epoch</div>
        <LossChart
          curve={report.curve.map((point) => ({
            epoch: point.epoch,
            train_loss: point.train_loss,
            val_loss: point.val_loss,
          }))}
        />
        <div className="mt-1 text-[10.5px] text-muted">
          {report.epochs} epochs · {duration(report.duration_ms)} · {number(report.parameters)} parameters ·
          float32 weights {bytes(report.weight_bytes.float32_bytes)} (int8 estimate{" "}
          {bytes(report.weight_bytes.int8_estimate_bytes)})
        </div>
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <div>
          <div className="label mb-2">
            Per-class metrics ({metrics.samples} rows) · values in %
          </div>
          <div className="max-h-[240px] overflow-auto rounded-lg border border-line">
            <table className="w-full text-[11px]">
              <thead className="sticky top-0 bg-surface">
                <tr className="border-b border-line">
                  <th className="table-head px-1.5 py-1.5 text-left">Class</th>
                  <th className="table-head px-1 py-1.5 text-right">n</th>
                  <th className="table-head px-1 py-1.5 text-right" title="precision">P</th>
                  <th className="table-head px-1 py-1.5 text-right" title="recall">R</th>
                  <th className="table-head px-1 py-1.5 text-right" title="F1">F1</th>
                </tr>
              </thead>
              <tbody>
                {metrics.per_class.map((row) => (
                  <tr key={row.label} className="border-b border-line/60">
                    <td className="max-w-[110px] truncate px-1.5 py-1.5 text-ink-dim" title={row.label}>
                      {row.label}
                    </td>
                    <td className="mono-num px-1 py-1.5 text-right text-muted">{row.support}</td>
                    <td className="mono-num px-1 py-1.5 text-right text-muted">
                      {(row.precision * 100).toFixed(0)}
                    </td>
                    <td className="mono-num px-1 py-1.5 text-right text-muted">
                      {(row.recall * 100).toFixed(0)}
                    </td>
                    <td className="mono-num px-1 py-1.5 text-right text-ink-dim">
                      {(row.f1 * 100).toFixed(0)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
        <Confusion labels={report.labels} matrix={metrics.confusion} />
      </div>

      <div className="mt-3 space-y-1">
        {report.notes.map((note, index) => (
          <p key={index} className="text-[10.5px] leading-relaxed text-muted">
            {note}
          </p>
        ))}
      </div>
    </Panel>
  );
}

function Confusion({ labels, matrix }: { labels: string[]; matrix: number[][] }) {
  const max = Math.max(1, ...matrix.flat());
  return (
    <div>
      <div className="label mb-2">Confusion matrix · rows are actual, columns predicted</div>
      <div className="overflow-auto rounded-lg border border-line p-1">
        <table className="w-full border-collapse text-[10.5px]">
          <thead>
            <tr>
              <th className="px-1 py-1" />
              {labels.map((label) => (
                <th key={label} className="max-w-[70px] truncate px-1 py-1 text-muted" title={label}>
                  {label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {matrix.map((row, actual) => (
              <tr key={labels[actual]}>
                <th className="max-w-[90px] truncate px-1 py-1 text-right font-normal text-muted" title={labels[actual]}>
                  {labels[actual]}
                </th>
                {row.map((value, predicted) => {
                  const intensity = value / max;
                  return (
                    <td
                      key={predicted}
                      className={cx(
                        "mono-num px-1 py-1 text-center",
                        actual === predicted ? "text-ink" : "text-ink-dim",
                      )}
                      style={{
                        background:
                          value === 0
                            ? "transparent"
                            : actual === predicted
                              ? `rgba(79,209,197,${0.12 + intensity * 0.42})`
                              : `rgba(239,180,47,${0.1 + intensity * 0.35})`,
                      }}
                    >
                      {value}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function Mini({ label, value, tone }: { label: string; value: string; tone?: "ok" }) {
  return (
    <div className="panel-flat px-2.5 py-2">
      <div className="label">{label}</div>
      <div className={cx("mono-num mt-0.5 text-[13.5px]", tone === "ok" ? "text-ok" : "text-ink")}>
        {value}
      </div>
    </div>
  );
}
