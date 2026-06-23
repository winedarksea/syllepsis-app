// TypeScript mirrors of syllepsis-core Rust types. Keep in sync with the Rust structs.
// Future: replace with tauri-specta generated bindings.

export type ObjectType =
  | 'note' | 'quote' | 'reference' | 'todo' | 'qa'
  | 'commentary' | 'table' | 'picture' | 'drawing' | 'code';

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

export type StatementType =
  | 'hypothesis' | 'factual_claim' | 'rule_or_requirement' | 'principle'
  | 'preference' | 'procedure' | 'context' | 'analysis_or_interpretation'
  | 'narrative' | 'idea';

export type Basis =
  | 'science_and_data' | 'regulation_or_standard' | 'logic_and_reasoning'
  | 'tradition_and_culture' | 'established_lore_or_fiction' | 'lived_experience'
  | 'personal_preference' | 'none';

export type Checkability =
  | 'objectively_checkable' | 'partly_judgment_based' | 'subjective_or_personal' | 'none';

export type Stability = 'settled' | 'evolving' | 'tentative';
export type Priority = 'standard' | 'important' | 'core';
export type LockMode = 'none' | 'unlock_delay' | 'fact_check_gate';

export interface Classification {
  statement_type: StatementType;
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
  private?: boolean;
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
  classification: Classification;
  dates: DateMetadata;
  authorship: Authorship;
  fork?: ForkInfo;
  lifecycle: Lifecycle;
  packs: PackMembership;
  kanban: Kanban;
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
  sorted: boolean;
  metadata: Metadata;
}

export interface Category {
  name: string;
  long_name: string;
  heading_level: number;
  icon?: string;
  parent?: string;
  location?: string;
  region?: SpatialRegion;
  private?: boolean;
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

// ── Search & embeddings (mirrors syllepsis_core::search::results) ──

export interface SearchHit {
  note_id: string;
  title: string;
  summary: string;
  snippet: string;
  categories: string[];
  score: number;
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

export interface EmbeddingDiagnostics {
  duplicates: DuplicatePair[];
  blind_spots: BlindSpot[];
}

// ── LLM (mirrors syllepsis_core::llm and syllepsis_core::app::llm) ──

export type LlmTask =
  | 'summarize'
  | 'fact_check'
  | 'devils_advocate'
  | 'grammar'
  | 'category_suggest'
  | 'rewrite';

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

export interface CloudLlmProviderStatus {
  provider: string;
  display_name: string;
  api_key_configured: boolean;
  base_url_configured: boolean;
  base_url_required: boolean;
}

export interface CloudLlmProviderSettings {
  provider: string;
  api_key?: string | null;
  base_url?: string | null;
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
}

export interface ManagedCloudReport {
  uploaded_patches: string[];
  downloaded_patches: string[];
  uploaded_snapshots: string[];
  reconstructed_notes: string[];
  skipped_notes: number;
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
  private_notes: NoteRef[];
  archived_notes: NoteRef[];
  locked_notes: LockedNote[];
  pending_deletion: PendingDeletion[];
  private_categories: string[];
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

export interface ImportOptions {
  selected_note_ids: string[];
  category_map: Record<string, string>;
}

export interface ImportReport {
  imported: string[];
  skipped_locally_modified: string[];
  created_categories: string[];
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
}

// ── Book statistics (mirrors syllepsis_core::app::commands::BookStats) ──

export interface BookStats {
  total_notes: number;
  sorted_notes: number;
  unsorted_notes: number;
  private_notes: number;
  archived_notes: number;
  starred_notes: number;
  notes_by_type: Record<string, number>;
  notes_by_category: Record<string, number>;
  total_categories: number;
  notes_with_location: number;
}

// ── Style cards (mirrors syllepsis_core::model::style_card) ──

export type StyleField = 'technical' | 'instructional' | 'persuasive' | 'narrative' | 'reflective' | 'administrative';
export type StyleTenor = 'intimate' | 'peer' | 'expert_to_peer' | 'expert_to_novice' | 'institutional';
export type StyleMode = 'spoken' | 'conversational_written' | 'edited_written' | 'formal_written';
export type StyleDensity = 'sparse' | 'moderate' | 'dense';
export type StyleTexture = 'plain' | 'polished' | 'vivid' | 'aphoristic' | 'procedural';
export type StyleOrganization = 'conclusion_first' | 'stepwise' | 'narrative' | 'compare_contrast' | 'problem_solution';

export interface StyleExemplar {
  text: string;
  note: string;
}

export interface StyleCard {
  id: string;
  version: number;
  short_description: string;
  field: StyleField;
  tenor: StyleTenor;
  mode: StyleMode;
  density: StyleDensity;
  texture: StyleTexture;
  organization: StyleOrganization;
  exemplars: StyleExemplar[];
  source_urls: string[];
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
