export type Verification =
  | { state: "verified"; checked_at: string }
  | { state: "unverified" }
  | { state: "failed"; reason: string; checked_at: string };

export type ModelKind =
  | { type: "original" }
  | {
      type: "compressed";
      source_model_id: string;
      source_sha256: string | null;
      format: string;
      job_id: string;
      output_sha256: string;
      source_size_bytes: number;
    };

export interface TensorTypeShare {
  id: number;
  name: string;
  elements: number;
  share: number;
}

export interface Compatibility {
  quantizable: boolean;
  requires_requantize_flag: boolean;
  source_quant: string;
  reason: string;
}

export interface ModelSummary {
  architecture: string | null;
  name: string | null;
  license: string | null;
  author: string | null;
  tokenizer: string | null;
  file_type: number | null;
  declared_quant: string | null;
  label_quant: string;
  dominant_type_id: number;
  dominant_quant: string;
  tensor_count: number;
  parameter_count: number;
  weights_bytes: number;
  context_length: number | null;
  embedding_length: number | null;
  block_count: number | null;
  head_count: number | null;
  head_count_kv: number | null;
  vocab_size: number | null;
  kv_bytes_per_token: number | null;
  gguf_version: number;
  alignment: number;
  compatibility: Compatibility;
  tensor_types: TensorTypeShare[];
  memory_note: string;
}

export interface ModelRecord {
  id: string;
  label: string;
  file_name: string;
  path: string;
  size_bytes: number;
  sha256: string | null;
  verification: Verification;
  imported_at: string;
  kind: ModelKind;
  summary: ModelSummary;
  notes: string;
}

export interface Snapshot {
  ts: string;
  total_memory: number;
  used_memory: number;
  available_memory: number;
  system_cpu_percent: number;
  process_rss: number;
  process_cpu_percent: number;
  model_process_pid: number | null;
  model_process_rss: number | null;
  load_average: [number, number, number];
}

export interface ToolInfo {
  name: string;
  path: string;
  version: string | null;
}

export interface BackendStatus {
  server: ToolInfo | null;
  quantize: ToolInfo | null;
  cli: ToolInfo | null;
  inference_ready: boolean;
  compression_ready: boolean;
  checked_at: string;
  message: string;
}

export interface ToolStatus {
  available: boolean;
  path: string | null;
  version: string | null;
  formats_from_tool: boolean;
  supported_types: string[];
  allow_requantize: boolean;
  checked_at: string;
  message: string;
}

export interface FormatOption {
  name: string;
  label: string;
  tradeoff: string;
  quality_tier: string;
  bits_per_weight: number;
  estimated_bytes: number;
  estimated_savings_bytes: number;
  estimated_savings_percent: number;
  supported_by_tool: boolean;
  supported_for_source: boolean;
  note: string;
}

export type JobStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled";

export interface JobView {
  id: string;
  kind: "import" | "quantize" | "benchmark" | "verification";
  title: string;
  status: JobStatus;
  stage: string;
  progress: number;
  params: any;
  result: any;
  error: string | null;
  created_at: string;
  started_at: string | null;
  finished_at: string | null;
  log_count: number;
}

export interface JobLog {
  seq: number;
  ts: string;
  stream: string;
  message: string;
}

export interface LoadOptions {
  model_path: string;
  model_label: string;
  context_length: number;
  threads: number;
  batch_size: number;
  gpu_layers: number;
  use_mmap: boolean;
  use_mlock: boolean;
  flash_attention: boolean;
  kv_cache_type_k: string;
  kv_cache_type_v: string;
  memory_budget_mb: number | null;
}

export interface RuntimeStatus {
  loaded: boolean;
  model_id: string | null;
  model_label: string | null;
  model_path: string | null;
  pid: number | null;
  base_url: string | null;
  load_ms: number | null;
  started_at: string | null;
  backend_version: string | null;
  options: LoadOptions | null;
  alive: boolean | null;
  rss_bytes: number | null;
}

export interface MemoryEstimate {
  weights_bytes: number;
  kv_cache_bytes: number | null;
  overhead_bytes: number;
  total_bytes: number;
  system_available_bytes: number;
  formula: string;
}

export interface LoadOutcome {
  status: RuntimeStatus;
  warnings: string[];
  estimate: MemoryEstimate;
}

export interface CaseResult {
  case_id: string;
  suite: string;
  title: string;
  prompt: string;
  output: string;
  passed: boolean;
  detail: string;
  latency_ms: number;
  tokens_per_second: number;
  completion_tokens: number;
  error: string | null;
}

export interface SuiteResult {
  suite: string;
  title: string;
  passed: number;
  total: number;
  pass_rate: number;
  kind: string;
}

export interface ConsistencyResult {
  prompt: string;
  samples: string[];
  agreement: number;
  detail: string;
}

export interface ModelRun {
  model_id: string;
  label: string;
  quant: string;
  kind: string;
  file_size: number;
  load_ms: number;
  peak_rss_bytes: number;
  rss_after_load_bytes: number;
  avg_ttft_ms: number;
  avg_tokens_per_second: number;
  total_generation_ms: number;
  suites: SuiteResult[];
  cases: CaseResult[];
  consistency: ConsistencyResult | null;
  quality_index: number;
  error: string | null;
}

export interface RunSettings {
  max_tokens: number;
  temperature: number;
  top_p: number;
  seed: number | null;
  context_length: number;
  threads: number;
  batch_size: number;
  kv_cache_type_k: string;
  kv_cache_type_v: string;
  gpu_layers: number;
  use_mmap: boolean;
  repeats: number;
}

export interface Environment {
  os: string;
  arch: string;
  cpu_count: number;
  total_memory: number;
  atom_version: string;
  backend_version: string | null;
  started_at: string;
}

export interface Comparison {
  id: string;
  created_at: string;
  title: string;
  settings: RunSettings;
  environment: Environment;
  runs: ModelRun[];
  job_id: string | null;
  notes: string;
}

export interface ComparisonSummary {
  id: string;
  created_at: string;
  title: string;
  models: string[];
  baseline_quant: string;
  candidate_quant: string;
  size_saved_percent: number | null;
  speed_change_percent: number | null;
  quality_delta: number | null;
  job_id: string | null;
}

export interface SuiteInfo {
  id: string;
  title: string;
  description: string;
  kind: string;
}

export interface ChatMessage {
  role: "system" | "user" | "assistant" | string;
  content: string;
}

export interface ConversationSummary {
  id: string;
  title: string;
  model_label: string;
  messages: number;
  updated_at: string;
  metrics: ChatMetric | null;
}

export interface ChatMetric {
  ttft_ms: number;
  tokens_per_second: number;
  completion_tokens: number;
  total_ms: number;
  at: string;
}

export interface Conversation {
  id: string;
  title: string;
  model_id: string | null;
  model_label: string;
  messages: ChatMessage[];
  metrics: ChatMetric[];
  created_at: string;
  updated_at: string;
}

export interface Config {
  model_roots: string[];
  models_dir: string;
  outputs_dir: string;
  reports_dir: string;
  data_root: string;
  llama_quantize_path: string | null;
  llama_server_path: string | null;
  llama_cli_path: string | null;
  threads: number;
  context_length: number;
  gpu_layers: number;
  batch_size: number;
  use_mmap: boolean;
  use_mlock: boolean;
  flash_attention: boolean;
  kv_cache_type_k: string;
  kv_cache_type_v: string;
  memory_budget_mb: number | null;
  verify_checksums: boolean;
  network_enabled: boolean;
  api_port: number;
}

export interface Overview {
  runtime: RuntimeStatus;
  system: Snapshot;
  backend: BackendStatus;
  tool: ToolStatus;
  counts: { models: number; originals: number; compressed: number; jobs: number; comparisons: number };
  disk: { library_bytes: number; compressed_bytes: number };
  last_inference:
    | { model_label: string; tokens_per_second: number; ttft_ms: number; completion_tokens: number; finished_at: string }
    | null;
  recent_jobs: JobView[];
  recent_comparisons: ComparisonSummary[];
  started_at: string;
}
