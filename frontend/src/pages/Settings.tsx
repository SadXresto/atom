import { useEffect, useState } from "react";
import {
  Cpu,
  FolderLock,
  HardDrive,
  Info,
  Keyboard,
  RefreshCw,
  Save,
  ShieldCheck,
  Sliders,
  Terminal,
} from "lucide-react";
import { api } from "../lib/api";
import { useApi } from "../lib/hooks";
import type { BackendStatus, Config, MicroCapabilities, ToolStatus } from "../lib/types";
import {
  Badge,
  Banner,
  Button,
  Field,
  KV,
  PageHeader,
  Panel,
  Select,
  TextInput,
  Toggle,
  toast,
} from "../components/ui";

const KV_TYPES = ["f16", "f32", "q8_0", "q4_0", "q4_1", "q5_0", "q5_1", "iq4_nl"];

export default function SettingsPage() {
  const { data, reload } = useApi<Config>("/settings", 0);
  const { data: backendData, reload: reloadBackend } = useApi<{ backend: BackendStatus; tool: ToolStatus }>(
    "/backend",
    0,
  );
  const { data: caps } = useApi<MicroCapabilities>("/micro/capabilities", 30000);
  const [draft, setDraft] = useState<Config | null>(null);
  const [rootsText, setRootsText] = useState("");
  const [datasetRootsText, setDatasetRootsText] = useState("");
  const [saving, setSaving] = useState(false);
  const [refreshing, setRefreshing] = useState(false);

  useEffect(() => {
    if (data) {
      setDraft(data);
      setRootsText(data.model_roots.join("\n"));
      setDatasetRootsText(data.dataset_roots.join("\n"));
    }
  }, [data]);

  if (!draft) {
    return (
      <div className="fade-in">
        <PageHeader title="Settings" description="Loading configuration…" />
        <Panel>
          <div className="h-40 shimmer rounded-lg" />
        </Panel>
      </div>
    );
  }

  const update = <K extends keyof Config>(key: K, value: Config[K]) => setDraft({ ...draft, [key]: value });

  const save = async () => {
    setSaving(true);
    try {
      const roots = rootsText
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean);
      const datasetRoots = datasetRootsText
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean);
      const payload: Config = { ...draft, model_roots: roots, dataset_roots: datasetRoots };
      const saved = await api<Config>("/settings", { method: "PUT", body: JSON.stringify(payload) });
      setDraft(saved);
      setRootsText(saved.model_roots.join("\n"));
      setDatasetRootsText(saved.dataset_roots.join("\n"));
      toast("Settings saved", "ok");
      void reloadBackend();
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setSaving(false);
    }
  };

  const refresh = async () => {
    setRefreshing(true);
    try {
      await api("/backend/refresh", { method: "POST" });
      await reloadBackend();
      toast("Toolchain re-probed", "ok");
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setRefreshing(false);
    }
  };

  const dirty =
    JSON.stringify(draft) !== JSON.stringify(data) ||
    rootsText !== data?.model_roots.join("\n") ||
    datasetRootsText !== data?.dataset_roots.join("\n");

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="Configuration"
        title="Settings"
        description="Paths, toolchain discovery and runtime defaults. ATOM refuses to read or write outside the approved directories."
        actions={
          <>
            <Button icon={<RefreshCw size={13} />} onClick={refresh} busy={refreshing} size="sm">
              Re-probe toolchain
            </Button>
            <Button variant="primary" icon={<Save size={13} />} onClick={save} busy={saving} disabled={!dirty}>
              {dirty ? "Save changes" : "Saved"}
            </Button>
          </>
        }
      />

      {!backendData?.backend.inference_ready && (
        <Banner tone="warn" title="Inference unavailable">
          {backendData?.backend.message}
        </Banner>
      )}

      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <Panel
          title="llama.cpp toolchain"
          subtitle="Explicit paths win; otherwise ATOM searches PATH for the binary."
          icon={<Terminal size={14} />}
          actions={
            <Badge tone={backendData?.backend.inference_ready && backendData?.backend.compression_ready ? "ok" : "warn"}>
              {backendData?.backend.inference_ready && backendData?.backend.compression_ready
                ? "ready"
                : "incomplete"}
            </Badge>
          }
        >
          <div className="space-y-3">
            <Field label="llama-server" hint="inference & benchmarking">
              <TextInput
                value={draft.llama_server_path ?? ""}
                onChange={(e) => update("llama_server_path", e.target.value || null)}
                placeholder={backendData?.backend.server?.path ?? "/usr/local/bin/llama-server"}
              />
            </Field>
            <Field label="llama-quantize" hint="permanent compression">
              <TextInput
                value={draft.llama_quantize_path ?? ""}
                onChange={(e) => update("llama_quantize_path", e.target.value || null)}
                placeholder={backendData?.backend.quantize?.path ?? "/usr/local/bin/llama-quantize"}
              />
            </Field>
            <Field label="llama-cli" hint="optional, single-shot runs">
              <TextInput
                value={draft.llama_cli_path ?? ""}
                onChange={(e) => update("llama_cli_path", e.target.value || null)}
                placeholder={backendData?.backend.cli?.path ?? "not configured"}
              />
            </Field>
            <div className="rounded-lg border border-line bg-black/20 px-3 py-2">
              <div className="label mb-1">Detected</div>
              <KV label="server" value={backendData?.backend.server?.version ?? backendData?.backend.server?.path ?? "not found"} mono />
              <KV label="quantize" value={backendData?.tool.path ?? "not found"} mono />
              <KV
                label="formats"
                value={
                  backendData?.tool.available
                    ? backendData.tool.formats_from_tool
                      ? `${backendData.tool.supported_types.length} advertised by the binary`
                      : "fallback type names"
                    : "—"
                }
              />
              <KV label="re-quantize" value={backendData?.tool.allow_requantize ? "supported" : "not advertised"} />
            </div>
            <p className="text-[11px] leading-relaxed text-muted">
              ATOM never downloads llama.cpp. Point these paths at a build you trust.
            </p>
          </div>
        </Panel>

        <Panel title="Storage" subtitle="The allowlist for every file operation." icon={<FolderLock size={14} />}>
          <div className="space-y-3">
            <Field label="Data root" hint="read only here">
              <TextInput value={draft.data_root} readOnly className="opacity-70" />
            </Field>
            <Field label="Models directory" hint="imports land here">
              <TextInput value={draft.models_dir} onChange={(e) => update("models_dir", e.target.value)} />
            </Field>
            <Field label="Compressed output directory">
              <TextInput value={draft.outputs_dir} onChange={(e) => update("outputs_dir", e.target.value)} />
            </Field>
            <Field label="Reports directory">
              <TextInput value={draft.reports_dir} onChange={(e) => update("reports_dir", e.target.value)} />
            </Field>
            <Field label="Additional model roots" hint="one absolute path per line">
              <textarea
                value={rootsText}
                onChange={(e) => setRootsText(e.target.value)}
                rows={3}
                className="input resize-y font-mono text-[11.5px]"
              />
            </Field>
            <Field label="Dataset roots" hint="Micro projects may import from here">
              <textarea
                value={datasetRootsText}
                onChange={(e) => setDatasetRootsText(e.target.value)}
                rows={3}
                className="input resize-y font-mono text-[11.5px]"
              />
            </Field>
            <Banner tone="info" title="Why an allowlist">
              Import paths must resolve inside these roots. Symlinks are resolved first, so a link pointing outside the
              allowlist is rejected.
            </Banner>
          </div>
        </Panel>
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <Panel title="Runtime defaults" icon={<Sliders size={14} />} subtitle="Used when a model is loaded without overrides.">
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Threads">
              <TextInput
                type="number"
                min={1}
                max={512}
                value={draft.threads}
                onChange={(e) => update("threads", Number(e.target.value) || 1)}
              />
            </Field>
            <Field label="Context length">
              <TextInput
                type="number"
                min={128}
                step={256}
                value={draft.context_length}
                onChange={(e) => update("context_length", Number(e.target.value) || 4096)}
              />
            </Field>
            <Field label="Batch size">
              <TextInput
                type="number"
                min={32}
                step={32}
                value={draft.batch_size}
                onChange={(e) => update("batch_size", Number(e.target.value) || 512)}
              />
            </Field>
            <Field label="GPU layers">
              <TextInput
                type="number"
                min={0}
                value={draft.gpu_layers}
                onChange={(e) => update("gpu_layers", Number(e.target.value) || 0)}
              />
            </Field>
            <Field label="KV cache K">
              <Select value={draft.kv_cache_type_k} onChange={(e) => update("kv_cache_type_k", e.target.value)}>
                {KV_TYPES.map((v) => (
                  <option key={v} value={v}>{v}</option>
                ))}
              </Select>
            </Field>
            <Field label="KV cache V">
              <Select value={draft.kv_cache_type_v} onChange={(e) => update("kv_cache_type_v", e.target.value)}>
                {KV_TYPES.map((v) => (
                  <option key={v} value={v}>{v}</option>
                ))}
              </Select>
            </Field>
            <Field label="Memory budget" hint="MiB, blank = none">
              <TextInput
                type="number"
                min={0}
                value={draft.memory_budget_mb ?? ""}
                onChange={(e) => update("memory_budget_mb", e.target.value === "" ? null : Number(e.target.value))}
              />
            </Field>
            <Field label="API port" hint="restart required">
              <TextInput
                type="number"
                min={1024}
                max={65535}
                value={draft.api_port}
                onChange={(e) => update("api_port", Number(e.target.value) || 8787)}
              />
            </Field>
          </div>
          <div className="mt-2 border-t border-line pt-1">
            <Toggle
              checked={draft.use_mmap}
              onChange={(v) => update("use_mmap", v)}
              label="Memory-map weights by default"
              description="Lower private memory; weights live in page cache."
            />
            <Toggle
              checked={draft.use_mlock}
              onChange={(v) => update("use_mlock", v)}
              label="mlock weights by default"
              description="Locks pages in RAM. Requires an adequate RLIMIT_MEMLOCK."
            />
            <Toggle
              checked={draft.flash_attention}
              onChange={(v) => update("flash_attention", v)}
              label="Request flash attention"
              description="Passed as -fa; newer builds decide whether it applies."
            />
          </div>
        </Panel>

        <Panel title="Safety and reliability" icon={<ShieldCheck size={14} />}>
          <div className="space-y-2">
            <Toggle
              checked={draft.verify_checksums}
              onChange={(v) => update("verify_checksums", v)}
              label="Record SHA-256 on import"
              description="Hashes the file once so later verification can detect tampering. Large files take time."
            />
            <Toggle
              checked={draft.network_enabled}
              onChange={(v) => update("network_enabled", v)}
              label="Allow network access"
              description="No download feature is implemented yet. Downloading models is planned, and would be opt-in, source-specific and checksum-verified."
              disabled={false}
            />
            {draft.network_enabled && (
              <Banner tone="warn" title="Network flag is on, nothing uses it">
                ATOM has no code path that fetches models. This switch only documents intent for the planned download
                flow; leaving it on does not add network behaviour.
              </Banner>
            )}
            <div className="rounded-lg border border-line bg-black/20 px-3 py-2">
              <div className="label mb-1">Operational guarantees</div>
              <ul className="space-y-1 text-[11.5px] leading-relaxed text-muted">
                <li>External tools run with explicit argv, never through a shell.</li>
                <li>Model metadata is treated as untrusted text, never executed.</li>
                <li>Cancelling a job kills its child process and removes partial output.</li>
                <li>Source models are never overwritten or deleted automatically.</li>
              </ul>
            </div>
            <div className="rounded-lg border border-line bg-black/20 px-3 py-2">
              <div className="flex items-center justify-between">
                <span className="label">Workspace</span>
                <Badge tone="accent">local</Badge>
              </div>
              <KV label="API" value="127.0.0.1 bound via host process" mono />
              <KV label="Checksums" value={draft.verify_checksums ? "recorded on import" : "skipped"} />
            </div>
          </div>
        </Panel>
      </div>

      <div className="mt-4">
        <Panel
          title="ATOM Micro toolchain"
          subtitle="Probed from PATH. Each missing tool disables exactly one step and explains the install."
          icon={<Cpu size={14} />}
          actions={
            <Badge tone={caps?.platformio.available ? "ok" : "warn"}>
              {caps?.platformio.available ? "builds available" : "builds unavailable"}
            </Badge>
          }
        >
          <div className="grid gap-3 lg:grid-cols-2">
            <div className="space-y-3">
              <Field label="PlatformIO path" hint="optional; otherwise PATH">
                <TextInput
                  value={draft.platformio_path ?? ""}
                  onChange={(e) => update("platformio_path", e.target.value || null)}
                  placeholder={caps?.platformio.path ?? "pio"}
                />
              </Field>
              <Field label="Python path" hint="used by the transformer adapter">
                <TextInput
                  value={draft.python_path ?? ""}
                  onChange={(e) => update("python_path", e.target.value || null)}
                  placeholder={caps?.python.path ?? "python3"}
                />
              </Field>
              <div className="rounded-lg border border-line bg-black/20 px-3 py-2">
                <div className="label mb-1">Detected</div>
                <KV label="PlatformIO" value={caps?.platformio.version ?? "not found"} mono />
                <KV label="Python" value={caps?.python.version ?? "not found"} mono />
                <KV label="PyTorch" value={caps?.pytorch.version ?? "not installed"} mono />
                <KV label="C compiler" value={caps?.cc.version ?? "not found"} mono />
                <KV label="esptool" value={caps?.esptool.version ?? "not found"} mono />
              </div>
            </div>
            <div className="space-y-2">
              <Toggle
                checked={draft.onboarding_completed}
                onChange={(v) => update("onboarding_completed", v)}
                label="Walkthrough completed"
                description="Turn this off to see the first-run walkthrough again."
              />
              <div className="rounded-lg border border-line bg-black/20 px-3 py-2 text-[11.5px] leading-relaxed text-muted">
                <div className="label mb-1">What the Micro pipeline runs</div>
                <ul className="space-y-1">
                  <li>Built-in byte MLP training and evaluation: this process, no Python.</li>
                  <li>Transformer architectures: external PyTorch adapter (requires torch).</li>
                  <li>Firmware build and flash: PlatformIO `pio run` with explicit arguments.</li>
                  <li>Host verification of exported C: your `cc`, when present.</li>
                </ul>
              </div>
              {!caps?.pytorch.available && (
                <Banner tone="info" title="PyTorch is optional">
                  Without it the built-in Atom MLP trainers still cover the whole workflow from dataset to firmware.
                </Banner>
              )}
            </div>
          </div>
        </Panel>
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <Panel title="Keyboard shortcuts" icon={<Keyboard size={14} />}>
          <div className="grid gap-x-6 gap-y-1.5 sm:grid-cols-2 text-[12px]">
            {[
              ["⌘K / Ctrl+K", "Command palette"],
              ["g then o", "Overview"],
              ["g then l", "Model Library"],
              ["g then c", "Compression Studio"],
              ["g then p", "Comparison Lab"],
              ["g then i", "Inference"],
              ["g then m", "System Monitor"],
              ["g then b", "Benchmark Center"],
              ["g then r", "Runtime Optimization"],
              ["g then u", "Micro Studio"],
              ["g then t", "Training Workspace"],
              ["g then d", "Distillation Lab"],
              ["g then w", "Hardware Lab"],
              ["g then y", "Deployment Center"],
              ["g then s", "Settings"],
              ["Esc", "Close dialog"],
            ].map(([keys, label]) => (
              <div key={keys} className="flex items-center justify-between gap-2 border-b border-line/60 py-1">
                <span className="text-muted">{label}</span>
                <kbd className="rounded border border-line bg-black/30 px-1.5 py-0.5 font-mono text-[10.5px] text-ink-dim">
                  {keys}
                </kbd>
              </div>
            ))}
          </div>
        </Panel>

        <Panel title="About ATOM" icon={<Info size={14} />}>
          <div className="space-y-1">
            <KV label="Version" value="0.1.0" mono />
            <KV label="Runtime" value="Tauri shell + Rust core + React UI" />
            <KV label="Inference" value="llama.cpp llama-server HTTP API" />
            <KV label="Compression" value="llama.cpp llama-quantize CLI" />
            <KV label="Model format" value="GGUF (v1-v3 headers)" />
            <KV label="Local API" value={`port ${draft.api_port}`} mono />
          </div>
          <div className="mt-3 flex items-start gap-2 rounded-lg border border-line bg-black/20 px-3 py-2 text-[11px] leading-relaxed text-muted">
            <HardDrive size={12} className="mt-0.5 shrink-0" />
            <span>
              ATOM is a local-first tool. It stores configuration, the model library index, benchmark reports and chat
              transcripts under the data root shown above. Model content never leaves this machine.
            </span>
          </div>
          <div className="mt-2 flex items-center gap-2 text-[11px] text-muted">
            <Cpu size={12} /> Built for developers who want measured numbers, not marketing claims.
          </div>
        </Panel>
      </div>
    </div>
  );
}
