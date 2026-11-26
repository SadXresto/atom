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
  kind:
    | "import"
    | "quantize"
    | "benchmark"
    | "verification"
    | "train"
    | "export"
    | "deploy"
    | "distillation";
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
  dataset_roots: string[];
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
  onboarding_completed: boolean;
  platformio_path: string | null;
  python_path: string | null;
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

/* -------------------------------- ATOM Micro ------------------------------- */

export interface MicroTarget {
  id: string;
  name: string;
  vendor: string;
  mcu: string;
  cpu_mhz: number;
  cores: number;
  sram_bytes: number;
  flash_bytes: number;
  psram_bytes: number;
  fp32_fpu: boolean;
  accelerator: string;
  toolchain: string;
  platformio_board: string | null;
  notes: string;
  firmware_bytes: number;
  runtime_ram_bytes: number;
}

export interface MicroArchitecture {
  id: string;
  name: string;
  family: string;
  summary: string;
  context: number;
  hidden: number;
  layers: number;
  trainable_here: boolean;
  requires: string | null;
  default_epochs: number;
  default_lr: number;
  default_batch: number;
  windows_per_epoch: number;
  parameters: number;
  description: string;
  best_for: string;
}

export interface ByteShare {
  byte: number;
  display: string;
  count: number;
  share: number;
}

export interface DatasetStats {
  documents: number;
  bytes: number;
  chars: number;
  words: number;
  lines: number;
  avg_document_chars: number;
  max_document_chars: number;
  duplicate_documents: number;
  unique_bytes: number;
  train_chars: number;
  val_chars: number;
  val_ratio: number;
  entropy_bits_per_byte: number;
  top_bytes: ByteShare[];
}

export interface PreviewItem {
  index: number;
  chars: number;
  excerpt: string;
}

export interface DatasetInfo {
  name: string;
  source_path: string;
  stored_path: string;
  format: string;
  imported_at: string;
  split_seed: number;
  stats: DatasetStats;
  preview: PreviewItem[];
}

export interface ArchSelection {
  spec_id: string;
  name: string;
  family: string;
  context: number;
  hidden: number;
  epochs: number;
  learning_rate: number;
  batch: number;
  seed: number;
  windows_per_epoch: number;
}

export interface EpochPoint {
  epoch: number;
  train_loss: number;
  val_loss: number;
  val_perplexity: number;
  top1: number;
  seconds: number;
}

export interface SampleGeneration {
  prompt: string;
  output: string;
}

export interface TrainReport {
  context: number;
  hidden: number;
  params: number;
  epochs: number;
  windows: number;
  val_windows: number;
  duration_ms: number;
  first_loss: number;
  train_loss: number;
  val_loss: number;
  perplexity: number;
  top1: number;
  top5: number;
  curve: EpochPoint[];
  samples: SampleGeneration[];
}

export interface EvalReport {
  windows: number;
  loss: number;
  perplexity: number;
  top1: number;
  top5: number;
  duration_ms: number;
  samples: SampleGeneration[];
  note: string;
}

export interface TrainingSummary {
  run_id: string;
  job_id: string;
  model_path: string;
  model_bytes: number;
  started_at: string;
  finished_at: string;
  report: TrainReport;
}

export interface QuantReport {
  format: string;
  params: number;
  weight_bytes: number;
  f32_bytes: number;
  ratio: number;
  bits_per_weight: number;
  val_loss: number;
  perplexity: number;
  top1: number;
  delta_loss: number;
  duration_ms: number;
  note: string;
}

export interface QuantSummary {
  format: string;
  path: string;
  bytes: number;
  report: QuantReport;
  created_at: string;
}

export interface MemoryPlan {
  target_id: string;
  target_name: string;
  quant_format: string;
  params: number;
  weight_bytes: number;
  activation_bytes: number;
  runtime_ram_bytes: number;
  total_ram_bytes: number;
  sram_bytes: number;
  psram_bytes: number;
  firmware_bytes: number;
  total_flash_bytes: number;
  flash_bytes: number;
  ram_used_percent: number;
  flash_used_percent: number;
  fits_ram: boolean;
  fits_flash: boolean;
  verdict: string;
  formula: string;
  estimate: boolean;
}

export interface ExportFile {
  name: string;
  bytes: number;
  sha256: string;
}

export interface ExportReport {
  dir: string;
  created_at: string;
  files: ExportFile[];
  manifest_path: string;
  manifest_sha256: string;
  quant_format: string;
  compile_check: string | null;
  total_bytes: number;
}

export interface FirmwareReport {
  dir: string;
  target_id: string;
  board: string;
  generated_at: string;
  built: boolean;
  build_tool: string;
  artifact_path: string | null;
  log_tail: string[];
  error: string | null;
  deployed: boolean;
  deployed_at: string | null;
}

export interface DeviceBenchmark {
  id: string;
  device: string;
  firmware: string;
  label: string;
  tokens_per_second: number;
  latency_ms: number;
  ram_bytes: number;
  flash_bytes: number;
  notes: string;
  measured_at: string | null;
  imported_at: string;
  source: string;
}

export interface DistillReport {
  teacher_model_id: string;
  teacher_label: string;
  prompts: number;
  teacher_tokens: number;
  duration_ms: number;
  agreement: number;
  report: TrainReport;
  dataset: DatasetInfo;
  model_path: string;
  created_at: string;
  method: string;
}

/* --------------------------- ATOM Micro classifier ------------------------- */

export interface CsvColumn {
  name: string;
  kind: "numeric" | "categorical" | string;
  missing: number;
  invalid: number;
  unique: number;
  min: number | null;
  max: number | null;
  mean: number | null;
  examples: string[];
}

export interface CsvProfile {
  path: string;
  bytes: number;
  rows: number;
  columns: CsvColumn[];
  warnings: string[];
}

export interface ClassShare {
  label: string;
  count: number;
  share: number;
}

export interface ClassifierSplit {
  train: number;
  validation: number;
  test: number;
}

export interface ClassifierDataset {
  profile: CsvProfile;
  spec: {
    features: string[];
    target: string;
    val_ratio: number;
    test_ratio: number;
    seed: number;
  };
  stored_path: string;
  design_features: number;
  classes: ClassShare[];
  split: ClassifierSplit;
  duplicate_rows: number;
  duplicate_groups: number;
  missing_values: number;
  notes: string[];
}

export interface ClassifierEpoch {
  epoch: number;
  train_loss: number;
  val_loss: number;
  val_accuracy: number;
  seconds: number;
}

export interface ClassMetrics {
  label: string;
  support: number;
  predicted: number;
  precision: number;
  recall: number;
  f1: number;
}

export interface ClassifierMetrics {
  samples: number;
  accuracy: number;
  macro_precision: number;
  macro_recall: number;
  macro_f1: number;
  weighted_f1: number;
  log_loss: number;
  per_class: ClassMetrics[];
  confusion: number[][];
  note: string;
}

export interface ClassifierReport {
  labels: string[];
  design_features: number;
  parameters: number;
  weight_bytes: { float32_bytes: number; int8_estimate_bytes: number; note: string };
  settings: { epochs: number; learning_rate: number; batch: number; l2: number; seed: number };
  epochs: number;
  duration_ms: number;
  train: ClassifierMetrics;
  validation: ClassifierMetrics;
  test: ClassifierMetrics;
  curve: ClassifierEpoch[];
  notes: string[];
}

export interface ClassifierTraining {
  job_id: string;
  model_path: string;
  model_bytes: number;
  started_at: string;
  finished_at: string;
  report: ClassifierReport;
}

export interface ClassifierExport {
  dir: string;
  created_at: string;
  files: Array<{ name: string; bytes: number; sha256: string }>;
  manifest_path: string;
  manifest_sha256: string;
  compile_check: string | null;
  validation: string;
  total_bytes: number;
}

export interface ClassifierRecord {
  dataset: ClassifierDataset;
  training: ClassifierTraining | null;
  export: ClassifierExport | null;
}

export interface MicroProject {
  id: string;
  name: string;
  description: string;
  domain: string;
  target_id: string;
  created_at: string;
  updated_at: string;
  dataset: DatasetInfo | null;
  architecture: ArchSelection | null;
  training: TrainingSummary | null;
  evaluation: EvalReport | null;
  quantization: QuantSummary | null;
  memory: MemoryPlan | null;
  export: ExportReport | null;
  firmware: FirmwareReport | null;
  distillation: DistillReport | null;
  benchmarks: DeviceBenchmark[];
  classification: ClassifierRecord | null;
  next_step: number;
  completed_steps: number[];
}

export interface ProjectSummary {
  id: string;
  name: string;
  domain: string;
  target_id: string;
  updated_at: string;
  params: number | null;
  dataset_documents: number | null;
  val_perplexity: number | null;
  quant_format: string | null;
  firmware_built: boolean;
  next_step: number;
  completed_steps: number;
}

export interface MicroOverviewData {
  projects: ProjectSummary[];
  counts: { projects: number; trained: number; quantized: number; built: number };
  storage_bytes: number;
  started_at: string;
}

export interface ToolProbe {
  name: string;
  path: string | null;
  version: string | null;
  required_for: string;
  install_hint: string;
  available: boolean;
}

export interface MicroCapabilities {
  platformio: ToolProbe;
  python: ToolProbe;
  pytorch: ToolProbe;
  esptool: ToolProbe;
  cc: ToolProbe;
  checked_at: string;
  note: string;
}

export interface BenchmarkSummary {
  device: string;
  firmware: string;
  best_tokens_per_second: number;
  mean_tokens_per_second: number;
  mean_latency_ms: number;
  peak_ram_bytes: number;
  runs: number;
}

export interface MicroProjectResponse {
  project: MicroProject;
  capabilities: MicroCapabilities;
  benchmark_summary: BenchmarkSummary | null;
}

export interface MicroInferResponse {
  output: string;
  tokens: number;
  total_ms: number;
  bytes_per_second: number;
  model_path: string;
  params: number;
  format: string;
}
