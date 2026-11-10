/** Tipos espelhando o que o `c2-core` serializa. */

export interface ModelSummary {
  name: string;
  path: string | null;
  host: string;
  port: number;
  ctx_size: number;
  gpu_layers: number;
  threads: number;
  disable_thinking: boolean;
  provider: string;
  base_url: string | null;
  api_key: boolean;
}

export interface IndexSummary {
  max_file_bytes: number;
  extensions: number;
  secret_policy: string;
  max_total_bytes_per_run: number;
}

export interface RuntimeSummary {
  healthy: boolean;
  pid: number | null;
  binary: string | null;
  source: string | null;
  version: string | null;
  url: string;
}

export interface EngineState {
  last_scan: number;
  indexed: number;
  unchanged: number;
  removed: number;
  errors: number;
  llm_calls: number;
  prompt_tokens: number;
  completion_tokens: number;
  latency_ms: number;
  last_error: string | null;
}

export interface Skipped {
  binary: number;
  too_large: number;
  unsupported_extension: number;
  sensitive: number;
  other: number;
}

export interface GgufInfo {
  path: string;
  size_bytes: number;
  version: number;
  tensor_count: number;
  architecture: string | null;
  name: string | null;
  quantization: string;
  file_type: number | null;
}

export interface GpuInfo {
  name: string;
  total_mb: number;
  used_mb: number;
  free_mb: number;
}

export type VramFit = "fits" | "tight" | "too_big" | "unknown";

export interface ModelInspection {
  gguf: GgufInfo;
  gpu: GpuInfo | null;
  fit: VramFit;
}

export interface UsageEstimate {
  model_mb: number;
  kv_mb: number;
  overhead_mb: number;
  total_mb: number;
  available_mb: number;
  fit: VramFit;
}

export interface RuntimePlan {
  gguf: GgufInfo;
  gpu: GpuInfo | null;
  ctx_size: number;
  gpu_layers: number;
  estimate: UsageEstimate;
  loaded: boolean;
}

export interface McpToolInfo {
  name: string;
  description: string;
}

export interface ProviderTest {
  ok: boolean;
  provider: string;
  model: string;
  latency_ms: number;
  sample: string | null;
  error: string | null;
}

export interface McpStatus {
  available: boolean;
  binary: string | null;
  protocol_version: string | null;
  server_name: string | null;
  tools: McpToolInfo[];
  error: string | null;
}

export interface RecentProject {
  path: string;
  opened_at: number;
}

export interface StatusReport {
  root: string;
  initialized: boolean;
  config_path: string;
  model: ModelSummary;
  index: IndexSummary;
  engine: { poll_interval_secs: number };
  runtime: RuntimeSummary;
  files: number;
  metadata: number;
  skipped: Skipped;
  state: EngineState;
}

export interface IndexReport {
  indexed: number;
  unchanged: number;
  removed: number;
  errors: number;
  deferred: number;
  relinked: number;
  blocked: number;
  masked: number;
  calls: number;
  prompt_tokens: number;
  completion_tokens: number;
  latency_ms: number;
  cost_usd: number;
  estimated_cost_usd: number;
  cost_capped: boolean;
  last_error: string | null;
}

export interface CostEstimate {
  pending: number;
  estimated_cost_usd: number;
  cap_usd: number;
}

export interface Evidence {
  field: string;
  terms: string[];
}

export interface Snippet {
  line_start: number;
  line_end: number;
  text: string;
}

export interface Hit {
  file: string;
  score: number;
  path_score: number;
  symbol_score: number;
  semantic_score: number;
  embedding_score?: number;
  evidence: Evidence[];
  summary: string;
  snippet: Snippet | null;
}

export interface FileEntry {
  file: string;
  what: string;
  summary: string;
  links: number;
}

export interface MetaLink {
  to: string;
  type: string;
}

export interface AgentNote {
  by: string;
  model: string | null;
  task: string | null;
  at: string;
  hash: string | null;
  kind: string;
  text: string;
}

export interface MetaFile {
  schema_version: number;
  file: string;
  hash: string;
  size: number;
  mtime: number;
  semantic: {
    what: string;
    why: string;
    goal: string;
    summary: string;
    responsibility: string;
  };
  links: MetaLink[];
  agent_notes?: AgentNote[];
}

export interface RelatedResult {
  file: string;
  uses: MetaLink[];
  used_by: MetaLink[];
}

export interface InitResult {
  created: boolean;
  files: number;
}

export interface ReadResult {
  file: string;
  line_start: number;
  line_end: number;
  lines: string[];
}

export interface EmbeddingStatus {
  enabled: boolean;
  healthy: boolean;
  pid: number | null;
  model_path: string | null;
  model_id: string;
  dim: number;
  device: string;
  vectors: number;
  files: number;
  port: number;
  binary: string | null;
  last_error: string | null;
}

export interface Neighbor {
  file: string;
  summary: string;
  score: number;
}

export interface DuplicatePair {
  a: string;
  b: string;
  score: number;
  linked: boolean;
}

export interface GenericFile {
  file: string;
  summary: string;
  what: string;
}

export interface DuplicateReport {
  pairs: DuplicatePair[];
  generic: GenericFile[];
}

export interface LinkSuggestion {
  from: string;
  to: string;
  score: number;
}

export interface CoChange {
  file: string;
  commits: number;
}

export interface NoteEntry {
  file: string;
  domain: string;
  index: number;
  stale: boolean;
  missing_hash: boolean;
  note: AgentNote;
}

export interface NotesInventory {
  total: number;
  stale: number;
  by_kind: Record<string, number>;
  domains: Record<string, number>;
  notes: NoteEntry[];
}

export interface UsageEvent {
  schema_version: number;
  at: string;
  actor: string;
  tool: string;
  file?: string;
  meta_reads: number;
  result_count?: number;
  annotation_kind?: string;
  duration_ms?: number;
  ok: boolean;
}

export interface UsageSummary {
  calls: number;
  meta_reads: number;
  annotations: number;
  code_reads: number;
  preflight_before_code: boolean;
  by_actor: Record<string, number>;
  by_tool: Record<string, number>;
}

export interface ForestTeleology {
  what: string;
  why: string;
  goal: string;
  who: string[];
  success: string[];
  non_goals: string[];
}

export interface ForestDomain {
  path: string;
  what: string;
  why: string;
  goal: string;
  boundaries: string[];
  key_files: string[];
  depends_on: string[];
  evidence: string[];
}

export interface ForestDecision {
  id: string;
  title: string;
  status: string;
  summary: string;
  evidence: string[];
}

export interface ForestRisk {
  title: string;
  severity: string;
  summary: string;
  evidence: string[];
}

export interface ForestDivergence {
  kind: string;
  claim: string;
  docs_ref: string;
  code_ref: string;
  severity: string;
}

export interface ForestData {
  schema_version: number;
  generated_at: string;
  teleology: ForestTeleology;
  architecture: { summary: string; layers: string[]; entrypoints: string[] };
  domains: ForestDomain[];
  decisions: ForestDecision[];
  risks: ForestRisk[];
  divergences: ForestDivergence[];
  sources: { path: string; hash: string; mtime: number }[];
  section_hashes: Record<string, string>;
}
