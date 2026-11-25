import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  ArrowDownUp,
  Ban,
  Bot,
  Download,
  Gauge,
  MemoryStick,
  MessageSquarePlus,
  Play,
  Power,
  PowerOff,
  Send,
  Settings2,
  Trash2,
  User,
  Zap,
} from "lucide-react";
import { api, consumeSse, downloadText, fetchText } from "../lib/api";
import { bytes, clamp, dateTime, relativeTime } from "../lib/format";
import { useApi } from "../lib/hooks";
import type {
  Conversation,
  ConversationSummary,
  LoadOutcome,
  MemoryEstimate,
  ModelRecord,
  RuntimeStatus,
} from "../lib/types";
import { ModelSelect } from "../components/ModelSelect";
import {
  Badge,
  Banner,
  Button,
  CopyButton,
  EmptyState,
  Field,
  KV,
  Modal,
  PageHeader,
  Panel,
  Progress,
  Select,
  TextInput,
  Toggle,
  cx,
  toast,
} from "../components/ui";

interface ChatMetrics {
  ttft_ms: number;
  tokens_per_second: number;
  completion_tokens: number;
  total_ms: number;
}

const KV_TYPES = ["f16", "f32", "q8_0", "q4_0", "q4_1", "q5_0", "q5_1", "iq4_nl"];

export default function Inference() {
  const navigate = useNavigate();
  const { data: runtimeData, reload: reloadRuntime } = useApi<{ runtime: RuntimeStatus; tuning: string[] }>(
    "/runtime",
    5000,
  );
  const { data: modelsData } = useApi<{ models: ModelRecord[] }>("/models", 0);
  const { data: chatData, reload: reloadChats } = useApi<{ conversations: ConversationSummary[] }>(
    "/conversations",
    8000,
  );

  const runtime = runtimeData?.runtime;
  const models = modelsData?.models ?? [];
  const conversations = chatData?.conversations ?? [];

  const [modelId, setModelId] = useState<string | null>(null);
  const [controlsOpen, setControlsOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [loadWarnings, setLoadWarnings] = useState<string[]>([]);
  const [estimate, setEstimate] = useState<MemoryEstimate | null>(null);

  const [overrides, setOverrides] = useState({
    context_length: 4096,
    threads: 4,
    batch_size: 512,
    gpu_layers: 0,
    use_mmap: true,
    use_mlock: false,
    flash_attention: true,
    kv_cache_type_k: "f16",
    kv_cache_type_v: "f16",
    memory_budget_mb: 0,
  });

  const [conversationId, setConversationId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Array<{ role: string; content: string }>>([]);
  const [partial, setPartial] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [metrics, setMetrics] = useState<ChatMetrics | null>(null);
  const [input, setInput] = useState("");
  const [gen, setGen] = useState({ max_tokens: 512, temperature: 0.7, top_p: 0.95, seed: "" as string });
  const abortRef = useRef<AbortController | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const selectedModel = models.find((m) => m.id === modelId) ?? null;

  useEffect(() => {
    if (!modelId && models.length) setModelId(models[0].id);
  }, [models, modelId]);

  useEffect(() => {
    textareaRef.current?.focus();
  }, [runtime?.loaded, conversationId]);

  useEffect(() => {
    if (!scrollRef.current) return;
    scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [messages, partial]);

  const localEstimate = useMemo(() => {
    if (!selectedModel) return null;
    const kvPerToken = selectedModel.summary.kv_bytes_per_token ?? 0;
    const factor = overrides.kv_cache_type_k === "f16" ? 1 : overrides.kv_cache_type_k === "q8_0" ? 0.53 : 0.28;
    const kv = Math.round(kvPerToken * factor * overrides.context_length);
    const overhead = 320 * 1024 * 1024;
    return {
      weights: selectedModel.summary.weights_bytes,
      kv,
      overhead,
      total: selectedModel.summary.weights_bytes + kv + overhead,
    };
  }, [selectedModel, overrides.context_length, overrides.kv_cache_type_k]);

  const load = async () => {
    if (!modelId) return;
    setLoading(true);
    setLoadWarnings([]);
    try {
      const body: Record<string, unknown> = { model_id: modelId, ...overrides };
      if (!overrides.memory_budget_mb) delete body.memory_budget_mb;
      const outcome = await api<LoadOutcome>("/runtime/load", { method: "POST", body: JSON.stringify(body) });
      setEstimate(outcome.estimate);
      setLoadWarnings(outcome.warnings);
      toast(`Loaded ${outcome.status.model_label} in ${outcome.status.load_ms} ms`, "ok");
      void reloadRuntime();
    } catch (e) {
      toast((e as Error).message, "error");
    } finally {
      setLoading(false);
    }
  };

  const unload = async () => {
    try {
      await api("/runtime/unload", { method: "POST" });
      toast("Model unloaded", "ok");
      void reloadRuntime();
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const release = async () => {
    try {
      const result = await api<{ released: string }>("/runtime/release", { method: "POST" });
      toast(result.released, "ok");
      void reloadRuntime();
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const newChat = () => {
    if (streaming) return;
    setConversationId(null);
    setMessages([]);
    setPartial("");
    setMetrics(null);
  };

  const openConversation = async (id: string) => {
    if (streaming) return;
    try {
      const result = await api<{ conversation: Conversation }>(`/conversations/${id}`);
      setConversationId(id);
      setMessages(result.conversation.messages);
      setMetrics(result.conversation.metrics.at(-1) ?? null);
      setPartial("");
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const send = async () => {
    const text = input.trim();
    if (!text || streaming) return;
    if (!runtime?.loaded) {
      toast("Load a model first", "error");
      return;
    }
    const nextMessages = [...messages, { role: "user", content: text }];
    setMessages(nextMessages);
    setInput("");
    setPartial("");
    setMetrics(null);
    setStreaming(true);

    const controller = new AbortController();
    abortRef.current = controller;
    let accumulator = "";

    try {
      const res = await fetch("/api/chat", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          conversation_id: conversationId,
          messages: nextMessages,
          max_tokens: gen.max_tokens,
          temperature: gen.temperature,
          top_p: gen.top_p,
          seed: gen.seed === "" ? null : Number(gen.seed),
        }),
        signal: controller.signal,
      });
      await consumeSse(res, (event, data) => {
        if (event !== "chat" || !data || typeof data !== "object") return;
        const payload = data as Record<string, any>;
        if (payload.type === "delta") {
          accumulator += payload.text ?? "";
          setPartial(accumulator);
        } else if (payload.type === "done") {
          setMetrics({
            ttft_ms: payload.ttft_ms,
            tokens_per_second: payload.tokens_per_second,
            completion_tokens: payload.completion_tokens,
            total_ms: payload.total_ms,
          });
        } else if (payload.type === "cancelled") {
          toast("Generation cancelled", "info");
        } else if (payload.type === "error") {
          toast(payload.message ?? "Inference failed", "error");
        }
      });

      const finalText = accumulator;
      if (finalText) {
        setMessages([...nextMessages, { role: "assistant", content: finalText }]);
      }
      setPartial("");
      void reloadChats();
      // Resolve the server-created conversation id so follow-up turns append.
      if (!conversationId) {
        try {
          const list = await api<{ conversations: ConversationSummary[] }>("/conversations");
          const latest = list.conversations[0];
          if (latest) setConversationId(latest.id);
          void reloadChats();
        } catch {
          /* non-fatal */
        }
      }
    } catch (e) {
      if ((e as Error).name !== "AbortError") toast((e as Error).message, "error");
    } finally {
      setStreaming(false);
      abortRef.current = null;
    }
  };

  const stop = async () => {
    try {
      await api("/chat/cancel", { method: "POST" });
      abortRef.current?.abort();
    } catch {
      abortRef.current?.abort();
    }
    setStreaming(false);
  };

  const exportChat = async () => {
    if (!conversationId) {
      toast("Send a message first so the conversation is saved", "info");
      return;
    }
    try {
      const markdown = await fetchText(`/conversations/${conversationId}/export`);
      downloadText(`atom-chat-${conversationId.slice(0, 8)}.md`, markdown);
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  const deleteChat = async (id: string) => {
    try {
      await api(`/conversations/${id}`, { method: "DELETE" });
      if (conversationId === id) newChat();
      void reloadChats();
      toast("Conversation deleted", "ok");
    } catch (e) {
      toast((e as Error).message, "error");
    }
  };

  return (
    <div className="fade-in">
      <PageHeader
        eyebrow="Inference Workspace"
        title="Run and test a model locally"
        description="ATOM starts llama-server with your tuning, streams tokens as they are produced, and reports measured latency and throughput. This is a testing workspace, not the whole product."
        actions={
          <>
            <Button size="sm" icon={<Settings2 size={13} />} onClick={() => setControlsOpen(true)}>
              Runtime controls
            </Button>
            {runtime?.loaded ? (
              <Button size="sm" variant="danger" icon={<PowerOff size={13} />} onClick={unload}>
                Unload
              </Button>
            ) : (
              <Button size="sm" variant="primary" icon={<Power size={13} />} onClick={load} busy={loading} disabled={!modelId}>
                Load model
              </Button>
            )}
          </>
        }
      />

      <div className="mb-4 grid gap-3 lg:grid-cols-[1.3fr_1fr]">
        <Panel
          title={runtime?.loaded ? runtime.model_label ?? "Loaded model" : "Runtime"}
          subtitle={
            runtime?.loaded
              ? `pid ${runtime.pid} · ${runtime.base_url} · loaded in ${runtime.load_ms} ms`
              : "No llama-server process. Loading a model starts one with the settings you choose."
          }
          icon={<Zap size={14} />}
          actions={
            <Badge tone={runtime?.loaded ? (runtime.alive ? "ok" : "bad") : "neutral"}>
              {runtime?.loaded ? (runtime.alive ? "serving" : "exited") : "idle"}
            </Badge>
          }
        >
          {runtime?.loaded ? (
            <div className="space-y-2">
              <div className="flex flex-wrap gap-1.5">
                <Badge tone="info">ctx {runtime.options?.context_length.toLocaleString()}</Badge>
                <Badge>{runtime.options?.threads} threads</Badge>
                <Badge>batch {runtime.options?.batch_size}</Badge>
                <Badge>kv {runtime.options?.kv_cache_type_k}/{runtime.options?.kv_cache_type_v}</Badge>
                <Badge>{runtime.options?.use_mmap ? "mmap" : "no-mmap"}</Badge>
                {runtime.options?.flash_attention && <Badge>flash-attn</Badge>}
                <Badge>{runtime.options?.gpu_layers ?? 0} GPU layers</Badge>
              </div>
              <div className="grid gap-x-6 sm:grid-cols-2">
                <KV label="Model RSS" value={runtime.rss_bytes ? bytes(runtime.rss_bytes) : "sampling…"} mono />
                <KV label="Load time" value={`${runtime.load_ms} ms`} mono />
                <KV label="Started" value={dateTime(runtime.started_at)} />
                <KV label="Backend build" value={runtime.backend_version ?? "not reported"} mono />
              </div>
              <div className="flex gap-2">
                <Button size="sm" onClick={() => setControlsOpen(true)} icon={<Settings2 size={12} />}>
                  Tune
                </Button>
                <Button size="sm" onClick={release} icon={<ArrowDownUp size={12} />}>
                  Release resources
                </Button>
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <ModelSelect models={models} value={modelId} onChange={setModelId} />
              {localEstimate && (
                <div className="rounded-lg border border-line bg-black/20 px-3 py-2">
                  <div className="flex items-center justify-between text-[11.5px]">
                    <span className="text-muted">Estimated runtime memory</span>
                    <span className="mono-num text-ink">{bytes(localEstimate.total)}</span>
                  </div>
                  <div className="mt-1 text-[10.5px] text-muted">
                    weights {bytes(localEstimate.weights)} + KV {bytes(localEstimate.kv)} + overhead{" "}
                    {bytes(localEstimate.overhead)}
                  </div>
                </div>
              )}
              <div className="flex gap-2">
                <Button variant="primary" onClick={load} busy={loading} disabled={!modelId} icon={<Power size={13} />}>
                  Load with current settings
                </Button>
                <Button onClick={() => setControlsOpen(true)} icon={<Settings2 size={13} />}>
                  Configure
                </Button>
              </div>
            </div>
          )}
          {(loadWarnings.length > 0 || estimate) && runtime?.loaded && (
            <div className="mt-3 space-y-2">
              {estimate && (
                <div className="rounded-lg border border-line bg-black/20 px-3 py-2 text-[11.5px] text-muted">
                  <span className="text-ink-dim">Memory estimate:</span> {estimate.formula}
                </div>
              )}
              {loadWarnings.map((warning, i) => (
                <Banner key={i} tone="warn" title="Load warning">
                  {warning}
                </Banner>
              ))}
            </div>
          )}
        </Panel>

        <Panel title="Last generation" icon={<Gauge size={14} />} subtitle="Measured by ATOM from the actual token stream.">
          {metrics ? (
            <div className="grid grid-cols-2 gap-2">
              <Metric label="tokens/s" value={metrics.tokens_per_second.toFixed(1)} />
              <Metric label="time to first token" value={`${metrics.ttft_ms} ms`} />
              <Metric label="tokens generated" value={String(metrics.completion_tokens)} />
              <Metric label="total time" value={`${(metrics.total_ms / 1000).toFixed(2)} s`} />
            </div>
          ) : (
            <EmptyState compact icon={<Gauge size={16} />} title="No generation yet" description="Send a prompt to measure." />
          )}
        </Panel>
      </div>

      <div className="grid gap-4 lg:grid-cols-[220px_1fr]">
        <Panel title="Conversations" icon={<MessageSquarePlus size={14} />} actions={
          <Button size="sm" onClick={newChat} icon={<MessageSquarePlus size={12} />}>
            New
          </Button>
        }>
          {conversations.length === 0 ? (
            <div className="py-4 text-center text-[11.5px] text-muted">Saved conversations appear here.</div>
          ) : (
            <div className="space-y-1">
              {conversations.map((conversation) => (
                <div
                  key={conversation.id}
                  className={cx(
                    "group flex items-center gap-1 rounded-lg border px-2 py-1.5",
                    conversationId === conversation.id
                      ? "border-accent/35 bg-accent/[0.07]"
                      : "border-transparent hover:border-line hover:bg-white/[0.02]",
                  )}
                >
                  <button onClick={() => void openConversation(conversation.id)} className="min-w-0 flex-1 text-left">
                    <div className="truncate text-[11.5px] text-ink-dim">{conversation.title}</div>
                    <div className="truncate text-[10px] text-muted">
                      {conversation.messages} msgs · {relativeTime(conversation.updated_at)}
                    </div>
                  </button>
                  <button
                    onClick={() => void deleteChat(conversation.id)}
                    className="shrink-0 rounded p-1 text-muted opacity-0 transition-opacity hover:text-bad group-hover:opacity-100"
                    aria-label="Delete conversation"
                  >
                    <Trash2 size={11} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </Panel>

        <section className="panel flex min-h-[520px] flex-col">
          <header className="flex flex-wrap items-center justify-between gap-2 border-b border-line px-4 py-2.5">
            <div className="flex items-center gap-2">
              <Bot size={14} className="text-accent" />
              <span className="text-[12.5px] font-medium text-ink">
                {runtime?.loaded ? runtime.model_label : "No model loaded"}
              </span>
              {runtime?.loaded && <Badge tone="ok">localhost</Badge>}
            </div>
            <div className="flex items-center gap-2">
              <Button size="sm" onClick={exportChat} icon={<Download size={12} />}>
                Export
              </Button>
            </div>
          </header>

          <div ref={scrollRef} className="min-h-0 flex-1 space-y-3 overflow-y-auto px-4 py-4">
            {messages.length === 0 && !partial && (
              <EmptyState
                icon={<Bot size={18} />}
                title={runtime?.loaded ? "Start a conversation" : "Load a model to begin"}
                description={
                  runtime?.loaded
                    ? "Prompts and responses stay local. Metrics are recorded per turn in the conversation."
                    : "The inference workspace needs a loaded llama.cpp model. Configure runtime settings first."
                }
                action={
                  runtime?.loaded ? undefined : (
                    <Button variant="primary" onClick={load} disabled={!modelId} busy={loading} icon={<Power size={13} />}>
                      Load {selectedModel?.summary.label_quant ?? "model"}
                    </Button>
                  )
                }
              />
            )}
            {messages.map((message, index) => (
              <MessageBubble key={index} role={message.role} content={message.content} />
            ))}
            {partial && <MessageBubble role="assistant" content={partial} streaming />}
          </div>

          <footer className="border-t border-line px-3 py-2.5">
            <div className="flex items-end gap-2">
              <textarea
                ref={textareaRef}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    void send();
                  }
                }}
                rows={2}
                disabled={!runtime?.loaded || streaming}
                placeholder={runtime?.loaded ? "Send a prompt… (Enter to send, Shift+Enter for newline)" : "Load a model first"}
                className="input max-h-40 min-h-[44px] flex-1 resize-y py-2 text-[12.5px] disabled:opacity-50"
              />
              {streaming ? (
                <Button variant="danger" onClick={stop} icon={<Ban size={13} />}>
                  Stop
                </Button>
              ) : (
                <Button variant="primary" onClick={send} disabled={!runtime?.loaded || !input.trim()} icon={<Send size={13} />}>
                  Send
                </Button>
              )}
            </div>
            <div className="mt-1.5 flex flex-wrap items-center gap-2 text-[11px] text-muted">
              <span className="mono-num">max {gen.max_tokens}</span>
              <span>·</span>
              <span className="mono-num">temp {gen.temperature}</span>
              <span>·</span>
              <span className="mono-num">top-p {gen.top_p}</span>
              <button onClick={() => setControlsOpen(true)} className="ml-auto flex items-center gap-1 hover:text-ink-dim">
                <Settings2 size={11} /> generation settings
              </button>
            </div>
          </footer>
        </section>
      </div>

      <Modal
        open={controlsOpen}
        onClose={() => setControlsOpen(false)}
        title="Runtime and generation settings"
        description="These are live-optimization knobs. They change how llama.cpp runs; the model file is never modified."
        width="max-w-2xl"
        footer={
          <div className="flex w-full items-center justify-between gap-2">
            <Button onClick={release} size="sm" icon={<ArrowDownUp size={12} />}>
              Release resources
            </Button>
            <div className="flex gap-2">
              <Button onClick={() => setControlsOpen(false)}>Close</Button>
              <Button variant="primary" onClick={load} busy={loading} disabled={!modelId || Boolean(runtime?.loaded)}>
                {runtime?.loaded ? "Unload first to apply" : "Load with these settings"}
              </Button>
            </div>
          </div>
        }
      >
        <div className="space-y-3">
          <Field label="Model">
            <ModelSelect models={models} value={modelId} onChange={setModelId} />
          </Field>

          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Context length" hint="tokens">
              <TextInput
                type="number"
                min={128}
                step={256}
                value={overrides.context_length}
                onChange={(e) => setOverrides({ ...overrides, context_length: clamp(Number(e.target.value) || 4096, 128, 1_000_000) })}
              />
            </Field>
            <Field label="Threads" hint="CPU workers">
              <TextInput
                type="number"
                min={1}
                max={512}
                value={overrides.threads}
                onChange={(e) => setOverrides({ ...overrides, threads: clamp(Number(e.target.value) || 4, 1, 512) })}
              />
            </Field>
            <Field label="Batch size">
              <TextInput
                type="number"
                min={32}
                step={32}
                value={overrides.batch_size}
                onChange={(e) => setOverrides({ ...overrides, batch_size: Number(e.target.value) || 512 })}
              />
            </Field>
            <Field label="GPU layers" hint="0 on CPU-only builds">
              <TextInput
                type="number"
                min={0}
                value={overrides.gpu_layers}
                onChange={(e) => setOverrides({ ...overrides, gpu_layers: Number(e.target.value) || 0 })}
              />
            </Field>
            <Field label="KV cache K">
              <Select
                value={overrides.kv_cache_type_k}
                onChange={(e) => setOverrides({ ...overrides, kv_cache_type_k: e.target.value })}
              >
                {KV_TYPES.map((v) => (
                  <option key={v} value={v}>{v}</option>
                ))}
              </Select>
            </Field>
            <Field label="KV cache V">
              <Select
                value={overrides.kv_cache_type_v}
                onChange={(e) => setOverrides({ ...overrides, kv_cache_type_v: e.target.value })}
              >
                {KV_TYPES.map((v) => (
                  <option key={v} value={v}>{v}</option>
                ))}
              </Select>
            </Field>
            <Field label="Memory budget" hint="MiB, 0 = no limit">
              <TextInput
                type="number"
                min={0}
                value={overrides.memory_budget_mb}
                onChange={(e) => setOverrides({ ...overrides, memory_budget_mb: Number(e.target.value) || 0 })}
              />
            </Field>
            <Field label="Max tokens per reply">
              <TextInput
                type="number"
                min={16}
                value={gen.max_tokens}
                onChange={(e) => setGen({ ...gen, max_tokens: Number(e.target.value) || 256 })}
              />
            </Field>
            <Field label="Temperature">
              <TextInput
                type="number"
                step="0.05"
                min={0}
                max={2}
                value={gen.temperature}
                onChange={(e) => setGen({ ...gen, temperature: Number(e.target.value) })}
              />
            </Field>
            <Field label="Top-p">
              <TextInput
                type="number"
                step="0.05"
                min={0}
                max={1}
                value={gen.top_p}
                onChange={(e) => setGen({ ...gen, top_p: Number(e.target.value) })}
              />
            </Field>
            <Field label="Seed" hint="blank = random">
              <TextInput value={gen.seed} onChange={(e) => setGen({ ...gen, seed: e.target.value })} placeholder="random" />
            </Field>
          </div>

          <div className="border-t border-line pt-1">
            <Toggle
              checked={overrides.use_mmap}
              onChange={(v) => setOverrides({ ...overrides, use_mmap: v })}
              label="Memory-map weights"
              description="Reversible. Pages weights from disk; RSS then reflects page-cache residency, not a private copy."
            />
            <Toggle
              checked={overrides.use_mlock}
              onChange={(v) => setOverrides({ ...overrides, use_mlock: v })}
              label="Lock weights in RAM (mlock)"
              description="Prevents paging at the cost of resident memory. Fails if the RLIMIT is too low."
            />
            <Toggle
              checked={overrides.flash_attention}
              onChange={(v) => setOverrides({ ...overrides, flash_attention: v })}
              label="Flash attention"
              description="Passes -fa to llama-server when the build supports it."
            />
          </div>

          {localEstimate && (
            <div className="rounded-lg border border-line bg-black/20 px-3 py-2">
              <div className="flex items-center justify-between text-[11.5px]">
                <span className="flex items-center gap-1.5 text-muted">
                  <MemoryStick size={12} /> Estimated runtime memory
                </span>
                <span className="mono-num text-ink">{bytes(localEstimate.total)}</span>
              </div>
              <Progress value={Math.min(1, localEstimate.total / Math.max(1, (selectedModel?.size_bytes ?? 1) * 3))} height={4} />
              <div className="mt-1 text-[10.5px] leading-relaxed text-muted">
                weights + KV cache at {overrides.context_length.toLocaleString()} tokens + ~320 MiB runtime overhead. Not a
                promise: llama.cpp allocates additional compute buffers at load time.
              </div>
            </div>
          )}
        </div>
      </Modal>

      {!runtime?.loaded && models.length === 0 && (
        <div className="mt-4">
          <Panel>
            <EmptyState
              icon={<Bot size={18} />}
              title="No models available"
              description="Import a GGUF model to start the inference workspace."
              action={
                <Button variant="primary" onClick={() => navigate("/library?import=1")}>
                  Import model
                </Button>
              }
            />
          </Panel>
        </div>
      )}
    </div>
  );
}

function MessageBubble({ role, content, streaming }: { role: string; content: string; streaming?: boolean }) {
  const isUser = role === "user";
  return (
    <div className={cx("flex gap-3", isUser && "justify-end")}>
      {!isUser && (
        <span className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-accent/25 bg-accent/10 text-accent">
          <Bot size={13} />
        </span>
      )}
      <div className={cx("group min-w-0", isUser ? "max-w-[80%]" : "flex-1")}>
        <div
          className={cx(
            "whitespace-pre-wrap rounded-xl border px-3 py-2 text-[12.5px] leading-relaxed",
            isUser
              ? "border-accent/20 bg-accent/[0.08] text-ink"
              : "border-line bg-black/25 text-ink-dim",
          )}
        >
          {content}
          {streaming && <span className="ml-0.5 inline-block h-3 w-1.5 animate-pulse bg-accent align-middle" />}
        </div>
        <div className={cx("mt-1 flex items-center gap-2", isUser && "justify-end")}>
          <span className="text-[10px] uppercase tracking-wide text-muted">
            {isUser ? <User size={9} className="mr-0.5 inline" /> : null}
            {role}
          </span>
          {!isUser && !streaming && <CopyButton text={content} label="Copy" />}
        </div>
      </div>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-line bg-black/20 px-2.5 py-2">
      <div className="label">{label}</div>
      <div className="mono-num mt-0.5 text-[16px] text-ink">{value}</div>
    </div>
  );
}
