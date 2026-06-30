// TypeScript mirrors of syllepsis-core Rust types. Keep in sync with the Rust structs.
// Future: replace with tauri-specta generated bindings.

export type ObjectType =
  | 'note' | 'commentary' | 'table' | 'picture' | 'drawing';

export type PriorKind =
  | 'new_paragraph'
  | 'same_paragraph'
  | 'indented_new_paragraph'
  | 'bullet_point'
  | 'numbered_list';

export type PriorRef =
  | { note: string }
  | { category: string };

export interface PriorEdge {
  target: PriorRef;
  kind: PriorKind;
}

export type ClassificationKind =
  | 'note' | 'qa' | 'reference' | 'quote' | 'code' | 'todo' | 'idea'
  | 'hypothesis' | 'factual_claim' | 'rule_or_requirement' | 'principle'
  | 'preference' | 'procedure' | 'context' | 'analysis_or_interpretation'
  | 'narrative';

export type Basis =
  | 'science_and_data' | 'regulation_or_standard' | 'logic_and_reasoning'
  | 'tradition_and_culture' | 'established_lore_or_fiction' | 'lived_experience'
  | 'personal_preference' | 'none';

export type Checkability =
  | 'objectively_checkable' | 'partly_judgment_based' | 'subjective_or_personal' | 'none';

export type Stability = 'settled' | 'evolving' | 'tentative';
export type Priority = 'standard' | 'important' | 'core';
export type LockMode = 'none' | 'unlock_delay' | 'fact_check_gate';
export type NoteStatus = 'open' | 'active' | 'needs_clarification' | 'deferred' | 'cancelled' | 'done';
export type NoteVisibility = 'active' | 'archived' | 'trash';
export type CommentaryKind = 'proposal' | 'fact_check' | 'critique' | 'comment' | 'footnote';
export type CommentaryStatus = 'locked' | 'open' | 'merged' | 'dismissed' | 'pinned';
export type CommentarySource = 'ai' | 'user';
export type CommentaryTargetField = 'body' | 'summary' | 'categories';

export interface Classification {
  kind: ClassificationKind;
  basis: Basis;
  checkability: Checkability;
  stability: Stability;
  priority: Priority;
  starred: boolean;
  stylistic_elements: string[];
}

export interface FlexDate {
  date?: string;
  relative_to?: string;
  relative_days?: number;
  reminder?: boolean;
}

export interface DateMetadata {
  created: string;
  updated: string;
  scheduled?: FlexDate;
  started?: FlexDate;
  due?: FlexDate;
  completed?: FlexDate;
}

export interface Authorship {
  created_by?: string;
  edited_by?: string[];
  ownership?: string;
  ai_generated?: boolean;
}

export interface ForkInfo {
  forked_from: string;
  forked_at: string;
}

export interface Lifecycle {
  /** Not shown in the main UI / default views / exports. */
  hidden?: boolean;
  /** Excluded from search + RAG retrieval. */
  exclude_from_search?: boolean;
  /** Added to .gitignore and excluded from the static-site publish. */
  exclude_from_publish?: boolean;
  lock?: LockMode;
  archived?: boolean;
  vanish_at?: string;
  marked_for_deletion_at?: string;
}

export interface PackMembership {
  packs?: string[];
  pack_version?: string;
  locally_modified?: boolean;
}

export interface Kanban {
  assignee?: string;
  magnitude?: number;
}

export interface Metadata {
  status?: NoteStatus;
  classification: Classification;
  dates: DateMetadata;
  authorship: Authorship;
  fork?: ForkInfo;
  lifecycle?: Lifecycle;
  packs: PackMembership;
  kanban: Kanban;
}

export interface CommentaryMetadata {
  parent_note_id: string;
  kind: CommentaryKind;
  status: CommentaryStatus;
  source: CommentarySource;
  target_field?: CommentaryTargetField | null;
  job_id?: string | null;
  task?: string | null;
  provider?: string | null;
  model?: string | null;
  base_body_sha256?: string | null;
  base_body?: string | null;
  crdt_backend?: string | null;
  base_crdt_snapshot_b64?: string | null;
  fact_check_passed?: boolean | null;
  approves_commentary_id?: string | null;
}

export interface NoteDto {
  id: string;
  type: ObjectType;
  title: string;
  summary: string;
  body: string;
  categories: string[];
  prior?: PriorEdge;
  location?: string;
  asset?: AssetMetadata;
  commentary?: CommentaryMetadata | null;
  sorted: boolean;
  metadata: Metadata;
}

export interface CommentarySummary {
  id: string;
  title: string;
  body: string;
  metadata: CommentaryMetadata;
  created: string;
  updated: string;
}

export interface ApplyCommentaryOptions {
  force_replace?: boolean;
  fact_check_passed?: boolean;
}

export type NoteScreenMode = 'read' | 'edit' | 'source';

export interface NoteNeighborSummary {
  id: string;
  title: string;
  summary: string;
}

export interface NoteNeighbors {
  previous?: NoteNeighborSummary | null;
  next?: NoteNeighborSummary | null;
}

export type NoteTokenCountMethod = 'embedding_tokenizer' | 'shared_tokenizer';

export interface NoteTokenCount {
  count: number;
  method: NoteTokenCountMethod;
  warning: boolean;
}

export interface NoteEmbeddingDetails {
  status: string;
  generated_at_unix_ms?: number | null;
  model_id?: string | null;
  dimensions?: number | null;
  summary_vector?: number[] | null;
  full_note_vector?: number[] | null;
}

export interface MergeNotesRequest {
  target_note_id: string;
  source_note_ids: string[];
}

export interface SplitNoteRequest {
  note_id: string;
  split_at: number;
  second_title?: string | null;
}

export interface SplitNoteResult {
  first: NoteDto;
  second: NoteDto;
}

export interface QueuedLlmJobRequest {
  target_note_id: string;
  task: LlmTask;
  model_override?: ModelRef | null;
  style_card_id?: string | null;
  style_overrides?: string | null;
  summary_variant?: SummaryVariant;
  rewrite_mode?: RewriteMode;
  store_result_as_commentary: boolean;
  for_proposal_id?: string | null;
}

export type QueuedLlmJobStatus = 'queued' | 'running' | 'complete' | 'failed';

export interface QueuedLlmJobResult {
  job_id: string;
  status: QueuedLlmJobStatus;
  target_note_id: string;
  task: LlmTask;
  proposal?: Proposal | null;
  commentary_id?: string | null;
  error?: string | null;
}

export interface CreateNoteOptions {
  vanishing?: boolean;
  vanish_days?: number;
  classification?: ClassificationKind;
}

export interface AssetMetadata {
  uuid: string;
  media_type: string;
  intrinsic_dimensions: [number, number];
  original_filename: string;
}

export interface Category {
  name: string;
  long_name: string;
  heading_level: number;
  icon?: string;
  parent?: string;
  location?: string;
  region?: SpatialRegion;
  /** Notes in this category are not shown in the main UI / default views / exports. */
  hidden?: boolean;
  /** Notes in this category are excluded from search + RAG retrieval. */
  exclude_from_search?: boolean;
  /** This category and its notes are added to .gitignore and excluded from the publish. */
  exclude_from_publish?: boolean;
}

// ── Spatial worlds & overlays (mirrors syllepsis_core::model::world + ::spatial) ──

export type WorldKind = 'geo' | 'image';

export interface World {
  id: string;
  display_name: string;
  kind: WorldKind;
  backdrop?: string;
  /** Intrinsic backdrop pixel size as [width, height] (image worlds). */
  intrinsic_dimensions?: [number, number];
  tile_source?: string;
}

export interface CreateImageWorldRequest {
  display_name: string;
  backdrop_asset_uuid: string;
}

export interface WorldDeletionImpact {
  note_references: number;
  category_references: number;
  lookup_references: number;
}

export type SpatialRegion =
  | { shape: 'svg_element'; element_id: string }
  | { shape: 'bounding_box'; x: number; y: number; width: number; height: number }
  | { shape: 'polygon'; points: [number, number][] };

export type WorldPoint =
  | { kind: 'geo'; lat: number; lon: number }
  | { kind: 'plane'; x: number; y: number };

export interface ResolvedLocation {
  world: string;
  point: WorldPoint;
}

export interface LookupEntry {
  name: string;
  world: string;
  first: number;
  second: number;
}

export type SpatialTarget =
  | { kind: 'note'; id: string; title: string }
  | { kind: 'category'; name: string };

export interface Pin {
  target: SpatialTarget;
  point: WorldPoint;
}

export interface OverlayRegion {
  category: string;
  region: SpatialRegion;
  anchor: WorldPoint;
}

export interface Overlay {
  world: World;
  pins: Pin[];
  regions: OverlayRegion[];
}

export interface RenderedNote {
  id: string;
  summary: string;
  body: string;
  join: PriorKind;
  list_depth: number;
  indented: boolean;
  numbered: boolean;
}

export type RenderItem =
  | { kind: 'heading'; level: number; text: string; category: string }
  | ({ kind: 'note' } & RenderedNote);

export interface BookInfo {
  name: string;
  path: string;
  open_warning: BookOpenWarningInfo | null;
}

export interface TrackedBookInfo {
  name: string;
  path: string;
  available: boolean;
  status?: string;
  git: TrackedBookGitInfo;
  cloud: TrackedBookCloudInfo;
}

export interface TrackedBookGitInfo {
  is_repository: boolean;
  branch?: string | null;
}

export interface TrackedBookCloudInfo {
  active_provider?: string | null;
  active_provider_display_name?: string | null;
  known_provider_ids: string[];
}

export interface BookOpenWarningInfo {
  missing_reserved_files: string[];
  should_offer_create_here: boolean;
}

// ── Cross-book search (mirrors CrossBookNote in search.rs) ──

export interface CrossBookNote {
  book_name: string;
  book_path: string;
  note_id: string;
  title: string;
  summary: string;
}

// ── Search & embeddings (mirrors syllepsis_core::search::results + filter) ──

export interface SearchFilter {
  visibility: NoteVisibility;
  categories: string[];
  updated_after: string | null;
  min_body_len: number | null;
  max_body_len: number | null;
  object_types: ObjectType[];
  classifications: ClassificationKind[];
  starred_only: boolean;
}

export function emptyFilter(): SearchFilter {
  return {
    categories: [],
    visibility: 'active',
    updated_after: null,
    min_body_len: null,
    max_body_len: null,
    object_types: [],
    classifications: [],
    starred_only: false,
  };
}

export interface SearchHit {
  note_id: string;
  title: string;
  summary: string;
  snippet: string;
  categories: string[];
  score: number;
  ranking_signals: SearchRankingSignals;
  object_type: ObjectType;
  classification: ClassificationKind;
  /** ISO timestamp of last update. */
  updated: string;
  starred: boolean;
  /** Body length in Unicode characters. */
  body_len: number;
  status?: NoteStatus;
  archived: boolean;
  marked_for_deletion_at?: string;
}

export interface SearchRankingSignals {
  exact: number;
  bm25: number;
  vector: number;
  total: number;
  /** Raw cosine similarity of the best-matching chunk to the query embedding. */
  vector_similarity: number;
}

export interface FacetCount {
  category: string;
  count: number;
}

export interface SearchResults {
  hits: SearchHit[];
  facets: FacetCount[];
}

export interface RelatedNote {
  note_id: string;
  title: string;
  summary: string;
  categories: string[];
  similarity: number;
  shares_category: boolean;
}

export interface DuplicatePair {
  a_id: string;
  a_title: string;
  b_id: string;
  b_title: string;
  similarity: number;
}

export interface BlindSpot {
  note_id: string;
  title: string;
  nearest_similarity: number;
}

export interface EmptyNote {
  note_id: string;
  title: string;
}

export interface EmbeddingDiagnostics {
  duplicates: DuplicatePair[];
  blind_spots: BlindSpot[];
  empty_notes: EmptyNote[];
}

export type GraphMode = 'categories' | 'pillars' | 'communities' | 'density' | 'timeline' | 'kanban';

export type TimelineDateField =
  | 'created'
  | 'updated'
  | 'scheduled'
  | 'started'
  | 'due'
  | 'completed';
export type TimelineGranularity = 'auto' | 'hour' | 'day' | 'month' | 'year';
export type TimelineColorBy = 'category' | 'cluster';
export type KanbanColorBy = 'classification' | 'category' | 'importance';

export interface GraphAnalysisRequest {
  mode: GraphMode;
  automatic_cluster_defaults: boolean;
  umap_neighbors: number;
  kmeans_k: number;
  louvain_resolution: number;
  hdbscan_min_cluster_size: number;
  timeline_primary_date: TimelineDateField;
  timeline_fallback_date: TimelineDateField | null;
  timeline_range_end_date: TimelineDateField | null;
  timeline_granularity: TimelineGranularity;
  timeline_color_by: TimelineColorBy;
}

export interface GraphAnalysisNode {
  id: string;
  type: ObjectType;
  title: string;
  summary: string;
  categories: string[];
  status?: NoteStatus;
  classification: ClassificationKind;
  priority: Priority;
  starred: boolean;
  created: string;
  updated: string;
  started?: string;
  completed?: string;
  x: number;
  y: number;
  cluster_id?: number;
  outlier: boolean;
  no_semantic_signal: boolean;
  timeline_date?: GraphTimelineNodeDate;
  timeline_range?: GraphTimelineNodeRange;
}

export interface GraphTimelineNodeDate {
  at_ms: number;
  source_field: TimelineDateField;
  used_fallback: boolean;
  date_only: boolean;
}

export interface GraphTimelineNodeRange {
  end_date: GraphTimelineNodeDate;
  end_x: number;
  end_before_start: boolean;
}

export interface GraphCluster {
  id: number;
  label: string;
  node_count: number;
}

export interface GraphSemanticEdge {
  source: string;
  target: string;
  similarity: number;
}

export interface GraphPriorEdge {
  source: string;
  target: string;
}

export interface GraphProviderMetadata {
  id: string;
  semantic: boolean;
}

export interface GraphAnalysisSummary {
  note_count: number;
  embedded_note_count: number;
  cluster_count: number;
  outlier_count: number;
  no_signal_count: number;
  semantic_edge_candidate_count: number;
}

export interface GraphTimelineTick {
  at_ms: number;
  label: string;
  x: number;
}

export interface GraphTimelineMeta {
  start_ms: number;
  end_ms: number;
  focus_start_x: number;
  focus_end_x: number;
  granularity: TimelineGranularity;
  ticks: GraphTimelineTick[];
  undated_count: number;
  bucket_count: number;
}

export interface GraphAnalysisResult {
  mode: GraphMode;
  nodes: GraphAnalysisNode[];
  clusters: GraphCluster[];
  semantic_edges: GraphSemanticEdge[];
  prior_edges: GraphPriorEdge[];
  provider: GraphProviderMetadata;
  summary: GraphAnalysisSummary;
  timeline?: GraphTimelineMeta;
}

// ── LLM (mirrors syllepsis_core::llm and syllepsis_core::app::llm) ──

export type LlmTask =
  | 'summarize'
  | 'fact_check'
  | 'devils_advocate'
  | 'grammar'
  | 'category_suggest'
  | 'rewrite'
  | 'generate_from_summary';

export type SummaryVariant = 'plain' | 'mnemonic' | 'acrostic';
export type RewriteMode = 'standard' | 'simplify';

export type ProposalStatus = 'pending' | 'accepted' | 'rejected';

export interface ModelRef {
  provider: string;
  model: string;
}

export interface LlmStatus {
  provider: string;
  live: boolean;
  enabled: boolean;
  auto_accept: boolean;
}

export type LlmExecutionMode = 'disabled' | 'local' | 'cloud' | 'unavailable';

export interface LlmRouteStatus {
  task: LlmTask;
  provider: string;
  model: string;
  execution_mode: LlmExecutionMode;
  available: boolean;
}

export interface CloudLlmProviderDescriptor {
  provider: string;
  display_name: string;
  base_url_required: boolean;
}

export interface CloudLlmProviderSettings {
  provider: string;
  api_key?: string | null;
  base_url?: string | null;
}

export interface CloudLlmModel {
  id: string;
}

export interface CloudLlmConnectionTestResult {
  provider: string;
  display_name: string;
  model_count: number;
  models: CloudLlmModel[];
  authentication_status: 'verified' | 'not_required' | 'not_tested' | 'inconclusive';
}

export interface BuildInfo {
  version: string;
  build_date: string;
}

// Book operational config — mirrors syllepsis_core::config::Config, persisted to _config.yaml.
// Updaters replace a whole sub-config, so always send the complete object read from getBookConfig.
export interface MarkdownConfig {
  dialect_version: string;
}

export interface SummaryConfig {
  max_chars: number;
  max_fraction_of_body: number;
}

export interface CleanupConfig {
  default_vanish_days: number;
  deletion_delay_days: number;
  todo_archive_days: number;
}

export interface PrivacyConfig {
  unlock_delay_hours: number;
  confirmation_delay_hours: number;
}

export interface EmbeddingConfig {
  chunk_token_limit: number;
  chunk_overlap_tokens: number;
  dimensions: number;
  model_id: string;
  matryoshka_dims: number | null;
}

export interface LocalAiDevicePolicy {
  generate_note_embeddings: boolean;
  pause_note_embeddings_on_battery: boolean;
  note_embedding_debounce_seconds: number;
  model_idle_unload_seconds: number;
}

export interface EmbeddingCoverage {
  total_notes: number;
  fresh_notes: number;
  stale_notes: number;
  missing_notes: number;
  incompatible_notes: number;
  blocked_notes: number;
}

export interface LocalAiFailure {
  occurred_at: string;
  job: string;
  message: string;
}

export interface LocalAiWorkerStatus {
  current_job: string | null;
  pending_llm_jobs: number;
  pending_query_jobs: number;
  pending_note_jobs: number;
  blocked_note_jobs: number;
  note_block_reason: string | null;
  power_source: 'ac' | 'battery' | 'unknown';
  policy: LocalAiDevicePolicy;
  recent_failures: LocalAiFailure[];
}

export interface LocalAiStatus {
  worker: LocalAiWorkerStatus;
  embedding_coverage: EmbeddingCoverage;
  embedding_model_id: string;
  embedding_model_cached: boolean;
}

export interface SearchConfig {
  rrf_k: number;
  category_upweight: number;
  bm25_k1: number;
  bm25_b: number;
  result_limit: number;
  related_limit: number;
  duplicate_similarity: number;
  blind_spot_similarity: number;
}

export interface LlmRouting {
  summarize: ModelRef;
  fact_check: ModelRef;
  devils_advocate: ModelRef;
  grammar: ModelRef;
  category_suggest: ModelRef;
  rewrite: ModelRef;
}

export interface LlmConfig {
  enabled: boolean;
  provider: string;
  local_model: string;
  max_new_tokens: number;
  auto_accept: boolean;
  routing: LlmRouting;
}

export interface SyncConfig {
  enabled: boolean;
  crdt_backend: string;
  conflict_marker: string;
  external_edit_skew_secs: number;
  author: string;
}

export interface GitChangedFile {
  path: string;
  status: string;
  stage_by_default: boolean;
}

export interface GitStatusDto {
  available: boolean;
  version?: string;
  is_repository: boolean;
  branch?: string;
  changed_files: GitChangedFile[];
  error?: string;
}

export interface GitCommandReport {
  command: string;
  stdout: string;
  stderr: string;
  hint?: string;
}

export interface SyncActivityEvent {
  happened_at: string;
  source: string;
  kind: string;
  path?: string;
  detail: string;
}

export interface SyncActivitySummary {
  external_updates_24h: number;
  external_updates_7d: number;
  external_note_updates_24h: number;
  latest_external_update_at?: string;
  conflict_copies_7d: number;
  latest_conflict_path?: string;
  latest_conflict_at?: string;
  remote_loro_merges_7d: number;
  latest_remote_loro_merge_note?: string;
  latest_remote_loro_merge_at?: string;
}

export interface OperationalGitSummary {
  available: boolean;
  is_repository: boolean;
  branch?: string;
  changed_file_count: number;
  commit_safe_note_change_count: number;
  error?: string;
}

export interface OperationalCloudSummary {
  provider_count: number;
  connected_provider_count: number;
  connected_provider_names: string[];
  error?: string;
}

export interface OperationalCrdtSummary {
  backend: string;
  sync_enabled: boolean;
  note_count: number;
  sidecar_count: number;
  loro_sidecar_coverage_percent: number;
}

export interface OperationalActivitySummary {
  activity: SyncActivitySummary;
  git: OperationalGitSummary;
  cloud: OperationalCloudSummary;
  crdt: OperationalCrdtSummary;
}

export interface NoteSyncActivity {
  kind: string;
  happened_at: string;
  detail: string;
}

export interface CloudSyncProviderDescriptor {
  provider: string;
  display_name: string;
  auth_url_base: string;
}

export interface CloudSyncProviderStatus {
  provider: string;
  display_name: string;
  connected: boolean;
  requires_loro: boolean;
  active_for_current_book: boolean;
}

export interface CloudSyncConnectStart {
  provider: string;
  auth_url: string;
  redirect_uri: string;
  state: string;
}

export interface CloudBookSummary {
  book_id: string;
  name: string;
  updated_at: string;
  remote_root: string;
  layout: string;
}

export interface ManagedCloudReport {
  uploaded_patches: string[];
  downloaded_patches: string[];
  uploaded_snapshots: string[];
  reconstructed_notes: string[];
  uploaded_embeddings: string[];
  downloaded_embeddings: string[];
  skipped_notes: number;
}

export interface SyncReport {
  pushed: string[];
  pulled: string[];
  merged: string[];
  conflicted: string[];
  deleted_local: string[];
  deleted_remote: string[];
  skipped: number;
}

/** Payload of the `cloud-sync-finished` event (a backgrounded "Sync now" completing). */
export interface CloudSyncFinished {
  provider: string;
  report?: SyncReport;
  error?: string;
}

export interface DeleteBookCloudCleanupOutcome {
  provider: string;
  attempted: boolean;
  connected: boolean;
  deleted_object_count: number;
  error?: string;
}

export interface DeleteCurrentBookReport {
  book_name: string;
  book_path: string;
  cloud_cleanup: DeleteBookCloudCleanupOutcome[];
}

export interface BookConfig {
  markdown: MarkdownConfig;
  summary: SummaryConfig;
  cleanup: CleanupConfig;
  privacy: PrivacyConfig;
  embedding: EmbeddingConfig;
  search: SearchConfig;
  llm: LlmConfig;
  sync: SyncConfig;
}

export interface Proposal {
  id: string;
  target: string;
  task: LlmTask;
  provider: string;
  model: string;
  live: boolean;
  content: string;
  status: ProposalStatus;
  created_at: string;
}

export interface CloudLlmPrompt {
  target_note_id: string;
  task: LlmTask;
  provider: string;
  model: string;
  system: string;
  user: string;
  output_contract: string;
}

export interface CloudLlmCompletion {
  target_note_id: string;
  task: LlmTask;
  provider: string;
  model: string;
  content: string;
}

export interface ModelManifestFile {
  repo_path: string;
  role: string;
  sha256?: string;
  size_bytes?: number;
}

export interface ModelManifest {
  id: string;
  display_name: string;
  repo: string;
  revision: string;
  kind: string;
  quantization: string;
  files: ModelManifestFile[];
  hidden_size: number;
  max_context_tokens: number;
  min_ram_mb: number;
  preferred_execution_providers: string[];
  pooling?: string;
  query_instruction?: string;
}

export interface ModelDownloadFileReport {
  file_name: string;
  integrity: 'verified' | 'unverified' | { mismatch: { expected: string; actual: string } };
}

export interface ModelDownloadReport {
  model_id: string;
  downloaded_files: ModelDownloadFileReport[];
}

export type ModelFileCacheState =
  | 'missing'
  | 'wrong_size'
  | 'present'
  | 'verified'
  | 'unverified'
  | 'mismatch';

export interface ModelFileCacheStatus {
  file_name: string;
  repo_path: string;
  role: string;
  expected_size_bytes?: number;
  actual_size_bytes?: number;
  sha256_configured: boolean;
  state: ModelFileCacheState;
  mismatch_expected?: string;
  mismatch_actual?: string;
}

export interface ModelCacheStatus {
  model_id: string;
  display_name: string;
  kind: string;
  cached: boolean;
  loadable: boolean;
  files: ModelFileCacheStatus[];
}

// ── Privacy & lifecycle (mirrors syllepsis_core::app::lifecycle) ──

export interface NoteRef {
  id: string;
  title: string;
}

export interface LockedNote {
  id: string;
  title: string;
  mode: LockMode;
}

export interface PendingDeletion {
  id: string;
  title: string;
  marked_at: string;
  purge_at: string;
}

export interface PolicyOverview {
  hidden_notes: NoteRef[];
  search_excluded_notes: NoteRef[];
  publish_excluded_notes: NoteRef[];
  archived_notes: NoteRef[];
  locked_notes: LockedNote[];
  pending_deletion: PendingDeletion[];
  hidden_categories: string[];
  search_excluded_categories: string[];
  publish_excluded_categories: string[];
  unlock_delay_hours: number;
}

// ── Knowledge packs (mirrors syllepsis_core::app::pack and ::pack) ──

export type ExportKind = 'pack' | 'book';

export interface PackManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  export_kind: ExportKind;
}

export interface ExportSpec {
  id: string;
  name: string;
  version: string;
  description: string;
  categories: string[];
  note_ids: string[];
  export_all: boolean;
  include_commentary?: boolean;
}

export type ImportStatus = 'new' | 'update' | 'locally_modified';

export interface ImportNotePreview {
  id: string;
  title: string;
  status: ImportStatus;
}

export interface CategoryMapping {
  incoming: string;
  suggested_local: string | null;
}

export interface ImportPreview {
  manifest: PackManifest;
  notes: ImportNotePreview[];
  category_suggestions: CategoryMapping[];
}

export type NoteResolution = 'overwrite' | 'merge' | 'commentary' | 'duplicate' | 'skip';

export interface ImportOptions {
  selected_note_ids: string[];
  category_map: Record<string, string>;
  resolutions?: Record<string, NoteResolution>;
}

export interface ImportReport {
  imported: string[];
  skipped_locally_modified: string[];
  created_categories: string[];
  overwritten: string[];
  merged: string[];
  commentary_created: string[];
  duplicated: string[];
}

// ── Text import (mirrors syllepsis_core::app::text_import) ──

export type TextImportSplitMode = 'one_note' | 'non_empty_line' | 'paragraph' | 'smart';
export type TextImportBlockKind = 'paragraph' | 'list' | 'table' | 'code';
export type TextImportPriorPreviewTarget = 'none' | 'previous_imported_note' | 'category' | 'existing_note';

export interface TextImportOptions {
  split_mode: TextImportSplitMode;
  detect_headings: boolean;
  detect_lists: boolean;
  detect_tables: boolean;
  detect_code_blocks: boolean;
  convert_indented_lists: boolean;
}

export interface TextImportPriorPreview {
  target: TextImportPriorPreviewTarget;
  target_label?: string | null;
  kind: PriorKind;
}

export interface TextImportPreviewItem {
  index: number;
  title: string;
  body: string;
  block_kind: TextImportBlockKind;
  category_context?: string | null;
  intended_prior?: TextImportPriorPreview | null;
  warnings: string[];
}

export interface TextImportCategoryPreview {
  name: string;
  long_name: string;
  heading_level: number;
}

export interface TextImportPreview {
  items: TextImportPreviewItem[];
  categories: TextImportCategoryPreview[];
  warnings: string[];
}

export type TextImportPlacement =
  | { kind: 'unsorted' }
  | { kind: 'category'; category: string }
  | { kind: 'after_note'; note_id: string };

export interface TextImportCommitRequest {
  items: TextImportPreviewItem[];
  categories: TextImportCategoryPreview[];
  placement: TextImportPlacement;
}

export interface TextImportReport {
  imported: string[];
  created_categories: string[];
  first_note_id?: string | null;
}

// ── Plugins (mirrors syllepsis_core::app::plugin::PluginDescriptor) ──

export type PluginKind = 'import_source' | 'code_block_renderer';
export type PluginSource = 'builtin' | 'user';

export interface PluginDescriptor {
  id: string;
  name: string;
  version: string;
  description: string;
  kind: PluginKind;
  languages: string[];
  import_extensions: string[];
  source: PluginSource;
  enabled: boolean;
}

// ── Book statistics (mirrors syllepsis_core::app::commands::BookStats) ──

export interface BookStats {
  total_notes: number;
  sorted_notes: number;
  unsorted_notes: number;
  hidden_notes: number;
  archived_notes: number;
  starred_notes: number;
  notes_by_type: Record<string, number>;
  notes_by_category: Record<string, number>;
  total_categories: number;
  notes_with_location: number;
}

// ── Style cards (mirrors syllepsis_core::model::style_card) ──

export type StyleVerbosity = 'succinct' | 'standard' | 'expansive';
export type StylePerspective =
  | 'first_person_singular'
  | 'first_person_plural'
  | 'first_person_soliloquy'
  | 'second_person'
  | 'third_person_objective'
  | 'third_person_omniscient'
  | 'third_person_limited';
export type StyleReadingLevel = 'elementary' | 'accessible' | 'advanced' | 'expert';
export type StyleVoice = 'active' | 'passive';

export interface StylePattern {
  text: string;
}

export interface StyleExemplar {
  text: string;
}

export interface StyleCard {
  id: string;
  version: number;
  name: string;
  short_description: string;
  verbosity: StyleVerbosity;
  perspective: StylePerspective;
  reading_level: StyleReadingLevel;
  voice: StyleVoice;
  patterns: StylePattern[];
  exemplars: StyleExemplar[];
  source_urls: string[];
}

// ── Category embedding stats ──

export interface CategoryEmbeddingStats {
  total_notes: number;
  embedded_notes: number;
  has_vector: boolean;
}

// ── Publishing & serving (mirrors syllepsis_core::app::publish) ──

export interface PublishReport {
  index_path: string;
  published_notes: number;
  excluded_private: number;
}

export interface GitignoreReport {
  excluded_paths: string[];
}

// ── Local search API ──

export interface SearchApiStatus {
  enabled: boolean;
  port: number;
  token: string | null;
  rest_url: string;
  mcp_url: string;
}
