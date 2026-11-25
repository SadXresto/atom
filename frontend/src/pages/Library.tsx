import { useMemo, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  BadgeCheck,
  Boxes,
  Download,
  FileSearch,
  FileUp,
  Filter,
  HardDrive,
  Layers,
  Loader2,
  RefreshCw,
  Search,
  ShieldCheck,
  Trash2,
  Upload,
  Zap,
} from "lucide-react";
import { api } from "../lib/api";
import { bytes, dateTime, number, params as formatParams, shortHash } from "../lib/format";
import { useApi, useDebounced } from "../lib/hooks";
import type { ModelRecord, Verification } from "../lib/types";
import { JobConsole } from "../components/JobConsole";
import { ModelLine } from "../components/ModelSelect";
import {
  Badge,
  Banner,
  Button,
  ConfirmDialog,
  DisclosureRow,
  EmptyState,
  Field,
  KV,
  Modal,
  PageHeader,
  Panel,
  Progress,
  SegmentedControl,
  Select,
  TextInput,
  cx,
  toast,
} from "../components/ui";

type FilterKind = "all" | "original" | "compressed" | "unverified";

export default function Library() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const { data, reload, loading } = useApi<{ models: ModelRecord[] }>("/models", 0);
  const models = data?.models ?? [];

  const [query, setQuery] = useState("");
  const debounced = useDebounced(query, 150);
  const [filter, setFilter] = useState<FilterKind>("all");
  const [sort, setSort] = useState("recent");
  const [importOpen, setImportOpen] = useState(params.get("import") === "1");
  const [path, setPath] = useState("");
  const [jobId, setJobId] = useState<string | null>(null);
  const [detail, setDetail] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ModelRecord | null>(null);
  const [deletingFile, setDeletingFile] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [dragging, setDragging] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const filtered = useMemo(() => {
    const list = models.filter((model) => {
      const haystack = `${model.label} ${model.file_name} ${model.summary.architecture ?? ""} ${model.summary.label_quant}`.toLowerCase();
      if (debounced && !haystack.includes(debounced.trim().toLowerCase())) return false;
      if (filter === "original") return model.kind.type === "original";
      if (filter === "compressed") return model.kind.type === "compressed";
      if (filter === "unverified") return model.verification.state !== "verified";
      return true;
    });
    return [...list].sort((a, b) => {
      if (sort === "size") return b.size_bytes - a.size_bytes;
      if (sort === "name") return a.label.localeCompare(b.label);
      return b.imported_at.localeCompare(a.imported_at);
    });
  }, [models, debounced, filter, sort]);

  const detailModel = models.find((m) => m.id === detail) ?? null;
  const variantsOfDetail = detailModel
    ? models.filter(
        (m) => m.kind.type === "compressed" && m.kind.source_model_id === detailModel.id,
      )
    : [];
  const compressedKind = detailModel?.kind.type === "compressed" ? detailModel.kind : null;
  const sourceModel = compressedKind ? models.find((m) => m.id === compressedKind.source_model_id) ?? null : null;

  const startImport = async () => {
    if (!path.trim()) return;
    try {
      const result = await api<{ job_id: string }>("/models/import", {
        method: "POST",
        body: JSON.stringify({ path: path.trim() }),
      });
      setJobId(result.job_id);
      setPath("");
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const uploadFiles = async (files: FileList | File[]) => {
    const file = Array.from(files).find((f) => f.name.toLowerCase().endsWith(".gguf"));
    if (!file) {
      toast("Only .gguf files can be imported", "error");
      return;
    }
    setUploading(true);
    try {
      const body = new FormData();
      body.append("file", file);
      const result = await api<{ job_id: string }>("/models/upload", { method: "POST", body });
      setJobId(result.job_id);
      setImportOpen(true);
      toast(`Uploaded ${file.name}; indexing…`, "ok");
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setUploading(false);
    }
  };

  const verify = async (model: ModelRecord) => {
    try {
      const result = await api<{ job_id: string }>(`/models/${model.id}/verify`, { method: "POST" });
      setJobId(result.job_id);
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const doDelete = async () => {
    if (!deleteTarget) return;
    try {
      await api(`/models/${deleteTarget.id}?confirm=${encodeURIComponent(deleteTarget.id)}&delete_file=${deletingFile}`, {
        method: "DELETE",
      });
      toast(
        deletingFile ? `Deleted ${deleteTarget.label} and its file` : `Removed ${deleteTarget.label} from the library`,
        "ok",
      );
      setDeleteTarget(null);
      setDetail(null);
      void reload();
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const totals = models.reduce(
    (acc, model) => {
      acc.all += model.size_bytes;
      if (model.kind.type === "compressed") acc.compressed += model.size_bytes;
      return acc;
    },
    { all: 0, compressed: 0 },
  );

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="Model management"
        title="Model Library"
        description="GGUF files ATOM knows about, including compressed variants it produced. Files are copied into the ATOM workspace on import and never modified afterwards."
        actions={
          <>
            <Button icon={<RefreshCw size={13} />} onClick={() => void reload()} size="sm">
              Refresh
            </Button>
            <Button variant="primary" icon={<Download size={13} />} onClick={() => setImportOpen(true)}>
              Import model
            </Button>
          </>
        }
      />

      <div className="mb-4 grid gap-3 sm:grid-cols-3">
        <MiniCard label="Models" value={String(models.length)} icon={<Boxes size={14} />} />
        <MiniCard label="On disk" value={bytes(totals.all)} icon={<HardDrive size={14} />} />
        <MiniCard label="Compressed variants" value={bytes(totals.compressed)} icon={<Layers size={14} />} />
      </div>

      <div
        onDragOver={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragging(false);
          void uploadFiles(e.dataTransfer.files);
        }}
        className={cx(
          "mb-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-dashed px-4 py-3 transition-colors",
          dragging ? "border-accent/60 bg-accent/[0.06]" : "border-line bg-white/[0.012]",
        )}
      >
        <div className="flex items-center gap-3">
          <span className="flex h-8 w-8 items-center justify-center rounded-lg border border-line bg-white/[0.03] text-accent">
            {uploading ? <Loader2 size={15} className="animate-spin" /> : <FileUp size={15} />}
          </span>
          <div>
            <div className="text-[12.5px] text-ink-dim">Drop a .gguf file here to import</div>
            <div className="text-[11px] text-muted">
              Copied into <span className="mono-num">{models[0]?.path ? models[0].path.split("/").slice(0, -1).join("/") : "the models directory"}</span>{" "}
              and checksummed. Source files are never altered.
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <input
            ref={fileInput}
            type="file"
            accept=".gguf"
            className="hidden"
            onChange={(e) => e.target.files && void uploadFiles(e.target.files)}
          />
          <Button size="sm" onClick={() => fileInput.current?.click()} busy={uploading}>
            Choose file
          </Button>
        </div>
      </div>

      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div className="relative min-w-[220px] flex-1">
          <Search size={13} className="pointer-events-none absolute left-2.5 top-2.5 text-muted" />
          <TextInput
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search by name, architecture or quantization…"
            className="pl-8"
          />
        </div>
        <SegmentedControl
          value={filter}
          onChange={setFilter}
          options={[
            { value: "all", label: "All", count: models.length },
            { value: "original", label: "Original", count: models.filter((m) => m.kind.type === "original").length },
            { value: "compressed", label: "Compressed", count: models.filter((m) => m.kind.type === "compressed").length },
            {
              value: "unverified",
              label: "Unverified",
              count: models.filter((m) => m.verification.state !== "verified").length,
            },
          ]}
        />
        <div className="flex items-center gap-1.5">
          <Filter size={12} className="text-muted" />
          <Select value={sort} onChange={(e) => setSort(e.target.value)} className="w-[132px] py-1.5 text-[12px]">
            <option value="recent">Newest first</option>
            <option value="name">Name</option>
            <option value="size">Largest first</option>
          </Select>
        </div>
      </div>

      {loading && models.length === 0 ? (
        <div className="grid gap-3 md:grid-cols-2">
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className="panel h-[132px] shimmer" />
          ))}
        </div>
      ) : filtered.length === 0 ? (
        <Panel>
          <EmptyState
            icon={<Boxes size={17} />}
            title={models.length ? "No models match this view" : "The library is empty"}
            description={
              models.length
                ? "Adjust the search or filter to see other models."
                : "Import a GGUF file to inspect its metadata, tune its runtime, or compress it. ATOM never downloads models without an explicit, verified source."
            }
            action={
              <Button variant="primary" icon={<Download size={13} />} onClick={() => setImportOpen(true)}>
                Import a GGUF model
              </Button>
            }
          />
        </Panel>
      ) : (
        <div className="grid gap-3 md:grid-cols-2">
          {filtered.map((model) => (
            <ModelCard
              key={model.id}
              model={model}
              variants={models.filter((m) => m.kind.type === "compressed" && m.kind.source_model_id === model.id).length}
              onOpen={() => setDetail(model.id)}
              onLoad={async () => {
                try {
                  await api("/runtime/load", { method: "POST", body: JSON.stringify({ model_id: model.id }) });
                  toast(`${model.label} loaded`, "ok");
                  navigate("/inference");
                } catch (e) {
                  toast((e as Error).message, "error");
                }
              }}
              onQuantize={() => navigate(`/compression?model=${model.id}`)}
            />
          ))}
        </div>
      )}

      {/* Import modal */}
      <Modal
        open={importOpen}
        onClose={() => {
          setImportOpen(false);
          setJobId(null);
          void reload();
        }}
        title="Import a GGUF model"
        description="ATOM copies the file into its models directory, parses the header, and records a SHA-256 checksum. The original file stays where it is."
        width="max-w-xl"
        footer={
          jobId ? (
            <Button
              variant="primary"
              onClick={() => {
                setImportOpen(false);
                setJobId(null);
                void reload();
              }}
            >
              Done
            </Button>
          ) : (
            <>
              <Button onClick={() => setImportOpen(false)}>Cancel</Button>
              <Button variant="primary" onClick={startImport} disabled={!path.trim()}>
                Import from path
              </Button>
            </>
          )
        }
      >
        {jobId ? (
          <JobConsole
            jobId={jobId}
            onFinished={() => {
              void reload();
            }}
          />
        ) : (
          <div className="space-y-3">
            <Field
              label="Absolute path to a .gguf file"
              hint="Desktop app can read any allowed root"
            >
              <TextInput
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder="/Users/you/Models/qwen2.5-7b-instruct-f16.gguf"
                onKeyDown={(e) => {
                  if (e.key === "Enter") void startImport();
                }}
              />
            </Field>
            <Banner tone="info" title="Import rules">
              Paths must resolve inside the model roots configured in Settings. Symlink escapes are rejected.
              Non-GGUF files are refused with the conversion requirement instead of being guessed at.
            </Banner>
            <Button icon={<FileUp size={13} />} onClick={() => fileInput.current?.click()} busy={uploading} className="w-full">
              Or upload a file from this device
            </Button>
          </div>
        )}
      </Modal>

      {/* Verify / job modal */}
      <Modal
        open={Boolean(jobId) && !importOpen}
        onClose={() => {
          setJobId(null);
          void reload();
        }}
        title="Verification job"
        description="Re-hashing the file and comparing it with the recorded checksum."
        width="max-w-xl"
        footer={
          <Button variant="primary" onClick={() => { setJobId(null); void reload(); }}>
            Close
          </Button>
        }
      >
        {jobId && <JobConsole jobId={jobId} onFinished={() => void reload()} />}
      </Modal>

      {/* Detail drawer */}
      <Modal
        open={Boolean(detailModel)}
        onClose={() => setDetail(null)}
        title={detailModel?.label ?? ""}
        description={detailModel?.summary.memory_note}
        width="max-w-3xl"
        footer={
          detailModel && (
            <div className="flex w-full flex-wrap items-center justify-between gap-2">
              <Button
                variant="danger"
                size="sm"
                icon={<Trash2 size={12} />}
                onClick={() => setDeleteTarget(detailModel)}
              >
                Remove
              </Button>
              <div className="flex items-center gap-2">
                <Button size="sm" icon={<ShieldCheck size={12} />} onClick={() => void verify(detailModel)}>
                  Verify checksum
                </Button>
                <Button size="sm" icon={<Layers size={12} />} onClick={() => navigate(`/compression?model=${detailModel.id}`)}>
                  Compress
                </Button>
                <Button
                  size="sm"
                  variant="primary"
                  icon={<Zap size={12} />}
                  onClick={async () => {
                    try {
                      await api("/runtime/load", {
                        method: "POST",
                        body: JSON.stringify({ model_id: detailModel.id }),
                      });
                      toast("Model loaded", "ok");
                      navigate("/inference");
                    } catch (e) {
                      toast((e as Error).message, "error");
                    }
                  }}
                >
                  Load & run
                </Button>
              </div>
            </div>
          )
        }
      >
        {detailModel && (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center gap-2">
              <Badge tone={detailModel.kind.type === "compressed" ? "accent" : "neutral"}>
                {detailModel.kind.type === "compressed" ? "Compressed variant" : "Original"}
              </Badge>
              <Badge>{detailModel.summary.label_quant}</Badge>
              <VerificationBadge verification={detailModel.verification} />
              <Badge>{bytes(detailModel.size_bytes)}</Badge>
              {detailModel.summary.architecture && <Badge>{detailModel.summary.architecture}</Badge>}
            </div>

            {compressedKind && (
              <Banner tone="info" title="Provenance">
                Created by llama-quantize ({compressedKind.format}) from{" "}
                {sourceModel ? (
                  <button className="text-accent underline-offset-2 hover:underline" onClick={() => setDetail(sourceModel.id)}>
                    {sourceModel.label}
                  </button>
                ) : (
                  "a model that is no longer in the library"
                )}. The source file is untouched.
              </Banner>
            )}

            <div className="grid gap-4 sm:grid-cols-2">
              <div>
                <div className="label mb-1.5">File</div>
                <KV label="Name" value={detailModel.file_name} mono />
                <KV label="Size" value={bytes(detailModel.size_bytes)} mono />
                <KV label="Path" value={detailModel.path} mono />
                <KV label="Imported" value={dateTime(detailModel.imported_at)} />
                <KV label="SHA-256" value={shortHash(detailModel.sha256)} mono hint={detailModel.sha256 ?? undefined} />
              </div>
              <div>
                <div className="label mb-1.5">Architecture</div>
                <KV label="Model" value={detailModel.summary.name ?? "—"} />
                <KV label="Parameters" value={`${number(detailModel.summary.parameter_count)} (${formatParams(detailModel.summary.parameter_count)})`} mono />
                <KV label="Context" value={detailModel.summary.context_length?.toLocaleString() ?? "—"} mono />
                <KV label="Layers" value={detailModel.summary.block_count ?? "—"} mono />
                <KV label="Embedding" value={detailModel.summary.embedding_length ?? "—"} mono />
                <KV label="GGUF version" value={`v${detailModel.summary.gguf_version}`} mono />
              </div>
            </div>

            <div>
              <div className="label mb-1.5">Tensor mix</div>
              <div className="space-y-2">
                {detailModel.summary.tensor_types.slice(0, 6).map((type) => (
                  <div key={type.id}>
                    <div className="mb-1 flex items-center justify-between text-[11.5px]">
                      <span className="text-ink-dim">
                        {type.name} <span className="text-muted">({number(type.elements)} weights)</span>
                      </span>
                      <span className="mono-num text-muted">{(type.share * 100).toFixed(1)}%</span>
                    </div>
                    <Progress value={type.share} height={4} />
                  </div>
                ))}
              </div>
              <div className="mt-2 text-[11px] text-muted">
                {detailModel.summary.tensor_count} tensors · alignment {detailModel.summary.alignment} bytes
              </div>
            </div>

            <DisclosureRow title="Quantization compatibility" meta={detailModel.summary.compatibility.quantizable ? "supported" : "not supported"}>
              <p className="text-[12px] leading-relaxed text-ink-dim">{detailModel.summary.compatibility.reason}</p>
            </DisclosureRow>
            <DisclosureRow title="License and source" meta={detailModel.summary.license ?? "not declared"}>
              <div className="space-y-1">
                <KV label="License" value={detailModel.summary.license ?? "not declared in metadata"} />
                <KV label="Author" value={detailModel.summary.author ?? "not declared"} />
                <KV label="Tokenizer" value={detailModel.summary.tokenizer ?? "—"} />
                <KV label="Vocabulary" value={detailModel.summary.vocab_size?.toLocaleString() ?? "—"} mono />
                <KV label="Notes" value={detailModel.notes || "—"} />
              </div>
            </DisclosureRow>

            {variantsOfDetail.length > 0 && (
              <div>
                <div className="label mb-1.5">Compressed variants</div>
                <div className="space-y-1.5">
                  {variantsOfDetail.map((variant) => (
                    <button
                      key={variant.id}
                      onClick={() => setDetail(variant.id)}
                      className="flex w-full items-center justify-between gap-2 rounded-lg border border-line bg-white/[0.02] px-2.5 py-2 text-left text-[12px] hover:border-line-strong"
                    >
                      <span className="truncate text-ink-dim">{variant.label}</span>
                      <span className="mono-num shrink-0 text-muted">{bytes(variant.size_bytes)}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </Modal>

      <ConfirmDialog
        open={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        onConfirm={doDelete}
        danger
        title="Remove model"
        confirmLabel={deletingFile ? "Delete file" : "Remove from library"}
        confirmWord={deleteTarget?.label}
        body={
          <div className="space-y-3">
            <p>
              <span className="text-ink">{deleteTarget?.label}</span> will be removed from the library. Removing the record
              never touches compressed variants' source files.
            </p>
            <label className="flex items-center gap-2 text-[12.5px]">
              <input
                type="checkbox"
                checked={deletingFile}
                onChange={(e) => setDeletingFile(e.target.checked)}
                className="h-3.5 w-3.5 accent-[#7f9dff]"
              />
              Also delete the model file from disk ({deleteTarget ? bytes(deleteTarget.size_bytes) : ""})
            </label>
          </div>
        }
      />
    </div>
  );
}

function ModelCard({
  model,
  variants,
  onOpen,
  onLoad,
  onQuantize,
}: {
  model: ModelRecord;
  variants: number;
  onOpen: () => void;
  onLoad: () => void;
  onQuantize: () => void;
}) {
  return (
    <div className="panel group flex flex-col gap-3 p-3.5 transition-colors hover:border-line-strong">
      <button onClick={onOpen} className="min-w-0 text-left">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <div className="truncate text-[13px] font-medium text-ink">{model.label}</div>
            <ModelLine model={model} />
          </div>
          <div className="flex shrink-0 flex-col items-end gap-1">
            <Badge tone={model.kind.type === "compressed" ? "accent" : "neutral"}>
              {model.kind.type === "compressed" ? "compressed" : "original"}
            </Badge>
            {variants > 0 && <span className="text-[10.5px] text-muted">{variants} variant{variants > 1 ? "s" : ""}</span>}
          </div>
        </div>
      </button>
      <div className="flex flex-wrap items-center gap-1.5">
        <Badge tone={model.summary.label_quant.includes("Q") ? "info" : "neutral"}>{model.summary.label_quant}</Badge>
        <VerificationBadge verification={model.verification} />
        {model.summary.architecture && <Badge>{model.summary.architecture}</Badge>}
        {model.summary.license && <Badge>{model.summary.license}</Badge>}
      </div>
      <div className="mt-auto flex items-center justify-between gap-2">
        <button
          onClick={onOpen}
          className="flex items-center gap-1 text-[11.5px] text-muted hover:text-ink-dim"
        >
          <FileSearch size={12} /> Inspect
        </button>
        <div className="flex items-center gap-1.5">
          <Button size="sm" icon={<Layers size={12} />} onClick={onQuantize}>
            Compress
          </Button>
          <Button size="sm" variant="primary" icon={<Zap size={12} />} onClick={onLoad}>
            Load
          </Button>
        </div>
      </div>
    </div>
  );
}

function VerificationBadge({ verification }: { verification: Verification }) {
  if (verification.state === "verified") {
    return (
      <span title={`Checked ${dateTime(verification.checked_at)}`}>
        <Badge tone="ok">
          <BadgeCheck size={11} /> verified
        </Badge>
      </span>
    );
  }
  if (verification.state === "failed") {
    return (
      <span title={verification.reason}>
        <Badge tone="bad">checksum failed</Badge>
      </span>
    );
  }
  return <Badge tone="warn">unverified</Badge>;
}

function MiniCard({ label, value, icon }: { label: string; value: string; icon: React.ReactNode }) {
  return (
    <div className="panel-flat flex items-center gap-3 px-3.5 py-2.5">
      <span className="flex h-7 w-7 items-center justify-center rounded-lg border border-line bg-white/[0.03] text-muted">
        {icon}
      </span>
      <div>
        <div className="label">{label}</div>
        <div className="mono-num text-[15px] text-ink">{value}</div>
      </div>
    </div>
  );
}
