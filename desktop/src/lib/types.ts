// Mirrors of the `brute` Rust engine's serde types, as returned by the
// Tauri command surface (desktop/src-tauri/src/commands/). Kept in sync
// by hand — see docs/desktop-architecture.md. Every "confidence" /
// "provenance" field below is real engine output, never invented here.

export type Confidence = "measured" | "detected" | "inferred" | "unavailable";

export interface HardwareField<T> {
  value: T | null;
  confidence: Confidence;
  source: string;
}

export type Provenance = "measured" | "detected" | "inferred" | "catalog" | "unknown";

export interface Valued<T> {
  value: T | null;
  provenance: Provenance;
  note: string;
}

export type GpuVendor = "nvidia" | "amd" | "intel" | "other";

export interface GpuAdapter {
  name: string;
  vendor: GpuVendor;
  dedicated_vram_bytes: number | null;
  shared_system_memory_bytes: number | null;
  driver_version: string | null;
}

export interface CpuReport {
  vendor: HardwareField<string>;
  brand: HardwareField<string>;
  physical_cores: HardwareField<number>;
  logical_cores: HardwareField<number>;
  instruction_sets: HardwareField<string[]>;
}

export interface MemoryReport {
  total_bytes: HardwareField<number>;
  available_bytes: HardwareField<number>;
}

export interface GpuReport {
  adapters: HardwareField<GpuAdapter[]>;
  cuda_available: HardwareField<boolean>;
  vulkan_available: HardwareField<boolean>;
}

export interface OsReport {
  product_name: HardwareField<string>;
  display_version: HardwareField<string>;
  build_number: HardwareField<string>;
  process_architecture: HardwareField<string>;
  native_architecture: HardwareField<string>;
}

export interface StorageReport {
  path_queried: string;
  free_bytes: HardwareField<number>;
  total_bytes: HardwareField<number>;
}

export type AcLineStatus = "offline" | "online" | "unknown";
export type ChassisClass = "laptop" | "desktop";

export interface PowerReport {
  ac_line_status: HardwareField<AcLineStatus>;
  battery_percent: HardwareField<number>;
  battery_present: HardwareField<boolean>;
  chassis_class: HardwareField<ChassisClass>;
}

export interface BackendAvailability {
  cpu: boolean;
  cuda: HardwareField<boolean>;
  vulkan: HardwareField<boolean>;
}

export interface ConfidenceSummary {
  measured_count: number;
  detected_count: number;
  inferred_count: number;
  unavailable_count: number;
}

export interface HardwareCapabilityProfile {
  schema_version: string;
  machine_id: string;
  captured_at_rfc3339: string;
  os: OsReport;
  cpu: CpuReport;
  memory: MemoryReport;
  gpus: GpuAdapter[];
  backends: BackendAvailability;
  storage: StorageReport | null;
  power: PowerReport;
  calibration_record_count: number;
  confidence_summary: ConfidenceSummary;
}

export type Backend = "cpu" | "cuda" | "vulkan";

export type BackendStatus =
  | "verified"
  | "launch_failed"
  | "model_load_failed"
  | "benchmark_failed"
  | "detected_only"
  | "unavailable"
  | "unknown";

export interface BackendVerification {
  backend: Backend;
  detected: boolean;
  binary_available: boolean;
  launch_verified: boolean;
  model_load_verified: boolean;
  benchmark_verified: boolean;
  status: BackendStatus;
  failure_reason: string | null;
  verification_tokens_per_second: number | null;
  reported_gpu_info: string | null;
}

// --- Catalog / fit / recommendation ---------------------------------

export type TaskCategory = "general_chat" | "coding" | "arabic_chat" | "reasoning";

// Mirrors the Rust `#[serde(tag = "status", rename_all = "snake_case")]`
// internally-tagged enum exactly: always an object with a `status` field,
// never a bare string - confirmed against the real wire format in
// data/catalog/dev-catalog.json (e.g. {"status":"known","identifier":"apache-2.0"}).
export type License = { status: "known"; identifier: string } | { status: "unknown" };
export type CommercialUse = "allowed" | "restricted" | "unknown";

export interface ModelBuild {
  catalog_id: string;
  family: string;
  display_name: string;
  publisher: string;
  official_source_url: string;
  official_repository_id: string;
  filename: string;
  architecture: string;
  parameter_count: number;
  quantization: string;
  file_size_bytes: number;
  estimated_disk_bytes: number | null;
  estimated_runtime_memory_bytes: number | null;
  min_recommended_ram_bytes: number;
  min_recommended_vram_bytes: number | null;
  supported_backends: Backend[];
  context_sizes: number[];
  task_categories: TaskCategory[];
  short_description: string;
  strength: string;
  limitation: string;
  license: License;
  commercial_use: CommercialUse;
  gated_access: boolean | null;
  metadata_provenance: string;
  last_reviewed: string;
}

export interface Catalog {
  notice: string | null;
  schema_version: string | null;
  builds: ModelBuild[];
}

export type Priority =
  | "fastest"
  | "balanced"
  | "highest_quality"
  | "lowest_memory"
  | "longest_context"
  | "coding"
  | "arabic_general_chat"
  | "privacy_offline";

export type FitState = "excellent" | "good" | "constrained" | "experimental" | "not_recommended" | "unknown";

export interface FitResult {
  state: FitState;
  reasons: string[];
  headroom_ratio: number | null;
  rules_version: string;
}

export type EstimateQuality = "precise_formula" | "coarse_approximation";

export interface MemoryEstimate {
  formula_version: string;
  quality: EstimateQuality;
  model_weights_bytes: Valued<number>;
  kv_cache_bytes_low: Valued<number>;
  kv_cache_bytes_high: Valued<number>;
  runtime_overhead_bytes_low: number;
  runtime_overhead_bytes_high: number;
  os_safety_reserve_bytes: number;
  estimated_total_ram_bytes_low: number;
  estimated_total_ram_bytes_high: number;
  estimated_vram_bytes_low: number | null;
  estimated_vram_bytes_high: number | null;
}

export type CalibrationProximity = "exact" | "close" | "distant";

export interface CalibrationRecord {
  recorded_at_rfc3339: string;
  machine_id: string;
  machine_profile_schema_version: string;
  catalog_id: string | null;
  architecture: string;
  quantization: string;
  parameter_count: number;
  backend: Backend;
  threads: number;
  [key: string]: unknown;
}

export interface CalibrationMatch {
  record: CalibrationRecord;
  proximity: CalibrationProximity;
  distance_notes: string[];
}

export interface CalibrationStore {
  records: CalibrationRecord[];
}

export interface BuildEvaluation {
  build: ModelBuild;
  estimate: MemoryEstimate;
  fit: FitResult;
  calibration_match: CalibrationMatch | null;
  component_scores: Record<string, number>;
  score: number;
}

export interface TechnicalExplanation {
  detected_resources: string;
  memory_calculation: string;
  calibration_used: string | null;
  backend_assumptions: string;
  context_and_batch_assumptions: string;
  fit_thresholds: string;
  confidence_calculation: string;
}

export interface Explanation {
  simple: string;
  technical: TechnicalExplanation;
}

export interface Recommendation {
  recommended: BuildEvaluation;
  safer_fallback: BuildEvaluation | null;
  stronger_optional: BuildEvaluation | null;
  explanation: Explanation;
}

export interface RecommendationDto {
  recommendation: Recommendation | null;
  ranking_formula_version: string;
}

export interface ExplainFitDto {
  evaluation: BuildEvaluation;
  explanation: Explanation;
}

// --- Trusted local model library -------------------------------------

export type FileStatus = "unchanged" | "missing" | "modified" | "replaced" | "inaccessible" | "moved";
export type TrustStatus =
  | "catalog_metadata_matched"
  | "local_unverified_source"
  | "modified_since_verification"
  | "corrupt"
  | "unsupported"
  | "missing"
  | "unknown";
export type CheckState = "verified" | "failed" | "unavailable" | "not_tested";
export type OverallIntegrity = "verified" | "suspect" | "corrupt" | "unknown";
export type CatalogMatchConfidence = "exact" | "strong" | "probable" | "weak" | "none";

export interface CatalogMatchResult {
  catalog_id: string | null;
  confidence: CatalogMatchConfidence;
  notes: string[];
}

export interface GgufVerification {
  header: CheckState;
  structure: CheckState;
  sha256: CheckState;
  runtime_load: CheckState;
  benchmark: CheckState;
  catalog_match: CatalogMatchConfidence;
  overall_integrity: OverallIntegrity;
  trust: TrustStatus;
}

export interface QuarantineInfo {
  reason: string;
  quarantined_at_rfc3339: string;
}

export interface LibraryEntry {
  library_id: string;
  schema_version: string;
  sha256: string;
  file_size_bytes: number;
  gguf_version: number;
  tensor_count: number;
  kv_count: number;
  architecture: string | null;
  quantization: string | null;
  parameter_count: number | null;
  current_path: string;
  original_import_path: string | null;
  imported_at_rfc3339: string;
  last_verified_at_rfc3339: string | null;
  file_modified_at_rfc3339: string | null;
  file_status: FileStatus;
  trust: TrustStatus;
  last_verification: GgufVerification | null;
  catalog_match: CatalogMatchResult;
  alias: string | null;
  notes: string | null;
  quarantine: QuarantineInfo | null;
  managed_copy: boolean;
}

export interface ImportOutcome {
  library_id: string;
  was_new: boolean;
  duplicate_of: string[];
  verification: GgufVerification;
  catalog_match: CatalogMatchResult;
}

export type DiscoveredKind = "gguf_candidate" | "unsupported_format" | "inaccessible";

export interface DiscoveredFile {
  path: string;
  size_bytes: number | null;
  kind: DiscoveredKind;
}

export interface ScanResult {
  root: string;
  discovered: DiscoveredFile[];
  directories_visited: number;
  inaccessible_directories: number;
  truncated_by_file_count: boolean;
  truncated_by_total_size: boolean;
  truncated_by_time: boolean;
  cancelled: boolean;
  wall_time_secs: number;
}

export interface ImportDirectoryOutcome {
  scan: ScanResult;
  imported: [string, { Ok: ImportOutcome } | { Err: string }][];
}

export interface ScanOptionsDto {
  recursive: boolean;
  max_depth?: number | null;
  max_files?: number | null;
  max_total_bytes?: number | null;
  max_duration_secs?: number | null;
}

export interface VerifyOutcomeDto {
  library_id: string;
  verification: GgufVerification | null;
  skipped_reason: string | null;
}

export interface RefreshOutcomeDto {
  library_id: string;
  file_status: FileStatus;
}

export interface DuplicateMember {
  library_id: string;
  current_path: string;
}

export interface DuplicateGroup {
  sha256: string;
  members: DuplicateMember[];
}

export interface LargestEntry {
  library_id: string;
  file_size_bytes: number;
}

export interface StorageSummary {
  total_entries: number;
  total_bytes: number;
  largest: LargestEntry[];
  [key: string]: unknown;
}

export interface LocateOutcome {
  verification: GgufVerification;
  reported_status: FileStatus;
}

export interface StaleAssociationNote {
  library_id: string;
  detail: string;
}

export interface PrivacyConcern {
  library_id: string;
  field: string;
  detail: string;
}

export interface AuditReport {
  healthy: string[];
  missing: string[];
  modified: string[];
  corrupt: string[];
  duplicate_groups: DuplicateGroup[];
  stale_profiles: StaleAssociationNote[];
  stale_calibrations: StaleAssociationNote[];
  unknown_provenance: string[];
  unsupported: string[];
  privacy_concerns: PrivacyConcern[];
  schema_migration_needed: string[];
  recommendations: string[];
}

export interface AssociationsDto {
  runtime_profiles: RuntimeProfile[];
  calibration_record_count: number;
}

// --- Tuning / runtime profiles ----------------------------------------

export type RankingPriority =
  | "balanced"
  | "fastest_generation"
  | "fastest_prompt_processing"
  | "lowest_memory"
  | "longest_context"
  | "maximum_stability"
  | "laptop_friendly";

export type TuningStabilityStatus = "stable" | "marginal" | "unstable";
export type RankingConfidence = "high" | "medium" | "low";

export interface Candidate {
  id: string;
  dimension: string;
  backend: Backend;
  threads: number;
  gpu_layers: number;
  context_size: number;
  batch_size: number;
  predicted_vram_bytes: number | null;
  predicted_ram_bytes: number | null;
}

export interface PrunedCandidate {
  id: string;
  dimension: string;
  reason: string;
}

export interface TuningDefaults {
  threads: number;
  gpu_layers: number;
  context_size: number;
  batch_size: number;
}

export interface TuningPlan {
  formula_version: string;
  model_sha256: string;
  machine_profile_schema_version: string;
  defaults: TuningDefaults;
  candidates: Candidate[];
  pruned: PrunedCandidate[];
  truncated: boolean;
}

export interface TunePlanDto {
  plan: TuningPlan;
  backend_verifications: BackendVerification[];
}

export interface StabilityAssessment {
  status: TuningStabilityStatus;
  formula_version: string;
  repetitions_requested: number;
  repetitions_succeeded: number;
  generation_coefficient_of_variation: number | null;
  prompt_coefficient_of_variation: number | null;
  reason: string;
}

export interface RepetitionSample {
  succeeded: boolean;
  timed_out: boolean;
  cancelled: boolean;
  crashed: boolean;
  generation_tokens_per_second: number | null;
  prompt_tokens_per_second: number | null;
}

export interface RepetitionAttempt {
  attempt_number: number;
  is_retry: boolean;
  sample: RepetitionSample | null;
  skip_reason: string | null;
}

export interface CandidateResult {
  candidate_id: string;
  attempts: RepetitionAttempt[];
  stability: StabilityAssessment;
  skipped: boolean;
}

export interface CandidateMeasurements {
  candidate_id: string;
  completed: boolean;
  stability: TuningStabilityStatus;
  backend: Backend;
  mean_generation_tokens_per_second: number | null;
  mean_prompt_tokens_per_second: number | null;
  context_size: number;
  batch_size: number;
  gpu_layers: number;
  threads: number;
  predicted_ram_bytes: number | null;
  predicted_vram_bytes: number | null;
}

export interface RankedCandidate {
  candidate_id: string;
  rank: number;
  measurements: CandidateMeasurements;
}

export interface RejectedCandidate {
  candidate_id: string;
  reason: string;
}

export interface RankingResult {
  formula_version: string;
  priority: RankingPriority;
  winner: RankedCandidate | null;
  runner_up: RankedCandidate | null;
  safer_fallback: RankedCandidate | null;
  rejected_faster_candidates: RejectedCandidate[];
  confidence: RankingConfidence;
  unknown_values: string[];
  ordered: RankedCandidate[];
  model_sha256: string;
  machine_profile_schema_version: string;
}

export interface TuneRunDto {
  backend_verifications: BackendVerification[];
  summary_cancelled: boolean;
  wall_time_secs: number;
  candidate_results: CandidateResult[];
  ranking: RankingResult;
  saved_profile_id: string | null;
}

export interface TuneProgressEvent {
  total_candidates: number;
  completed_candidates: number;
  current_candidate_id: string | null;
}

export interface RuntimeProfile {
  schema_version: string;
  profile_id: string;
  tuning_date: string;
  model_sha256: string;
  model_architecture: string | null;
  model_quantization: string | null;
  model_parameter_count: number | null;
  machine_profile_schema_version: string;
  machine_id: string;
  backend: Backend;
  llama_cli_sha256: string | null;
  llama_bench_sha256: string | null;
  threads: number;
  gpu_layers: number;
  context_size: number;
  batch_size: number;
  mean_generation_tokens_per_second: number | null;
  mean_prompt_tokens_per_second: number | null;
  predicted_ram_bytes: number | null;
  predicted_vram_bytes: number | null;
  stability: TuningStabilityStatus;
  stability_formula_version: string;
  ranking_formula_version: string;
  tuning_formula_version: string;
  confidence: RankingConfidence;
  source_repetitions_succeeded: number;
  source_repetitions_requested: number;
}

export type ApplyStatus = "verified" | "incompatible_profile" | "verification_failed";

export interface LaunchPreview {
  profile_id: string;
  backend: Backend;
  threads: number;
  gpu_layers: number;
  context_size: number;
  batch_size: number;
  binary_dir: string;
  model_path: string;
}

export interface ApplyResult {
  status: ApplyStatus;
  preview: LaunchPreview;
  compatibility_issues: string[];
  confirmed_backend_matches: boolean | null;
  confirmed_threads: number | null;
  confirmed_gpu_layers: number | null;
  verification_tokens_per_second: number | null;
  rollback_recommended: boolean;
  detail: string;
}

// --- Local run workspace ----------------------------------------------

/** Mirrors `commands::run::RunPhase` - every transition is driven by a
 * genuine engine signal (a real binary hash check, the first byte of
 * real subprocess output, a real cancellation request), never a timer. */
export type RunPhase =
  | "preparing"
  | "validating_runtime"
  | "loading_model"
  | "generating"
  | "stopping"
  | "completed"
  | "failed"
  | "cancelled";

export interface LocalRunOutcome {
  succeeded: boolean;
  timed_out: boolean;
  cancelled: boolean;
  generation_tokens_per_second: number | null;
  prompt_tokens_per_second: number | null;
  elapsed_secs: number;
  error: string | null;
}

// --- Runtime auto-resolution --------------------------------------------

export type RuntimeSource = "bundled" | "user_override" | "discovered" | "not_found";

export interface RuntimeResolution {
  source: RuntimeSource;
  binary_dir: string | null;
  cli_verified: boolean;
  bench_verified: boolean;
  detail: string;
}

// --- Model auto-discovery -------------------------------------------------

export interface CommonLocation {
  label: string;
  path: string;
  exists: boolean;
}

export interface DiscoveryResult {
  label: string;
  path: string;
  scan: ScanResult;
}

// --- Local Chat conversation history ---------------------------------------
// Mirrors brute::conversations exactly - a plain (non-tagged) struct on
// the Rust side, so these fields map directly with no discriminant games.

export type MessageRole = "user" | "assistant";

export interface ConversationMessage {
  message_id: string;
  role: MessageRole;
  content: string;
  created_at_rfc3339: string;
}

export interface Conversation {
  conversation_id: string;
  schema_version: string;
  title: string;
  created_at_rfc3339: string;
  updated_at_rfc3339: string;
  library_id: string | null;
  profile_id: string | null;
  language: string | null;
  messages: ConversationMessage[];
}

export interface ConversationSummary {
  conversation_id: string;
  title: string;
  created_at_rfc3339: string;
  updated_at_rfc3339: string;
  message_count: number;
  library_id: string | null;
}
