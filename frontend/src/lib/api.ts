// Typed wrappers around tauri invoke. Each function mirrors a #[tauri::command] on the
// Rust side. Replace with tauri-specta generated bindings once binding generation is wired.

import { invoke } from '@tauri-apps/api/core';
import type {
  BookInfo, TrackedBookInfo, Category, NoteDto, ObjectType, PriorEdge, RenderItem,
  NoteNeighbors, NoteTokenCount, NoteEmbeddingDetails, MergeNotesRequest, SplitNoteRequest, SplitNoteResult,
  SearchResults, RelatedNote, EmbeddingDiagnostics, CategoryEmbeddingStats,
  GraphAnalysisRequest, GraphAnalysisResult,
  LlmStatus, LlmRouteStatus, LlmTask, ModelRef, Proposal, QueuedLlmJobRequest, QueuedLlmJobResult, CloudLlmPrompt, CloudLlmCompletion,
  CloudLlmConnectionTestResult, CloudLlmModel, CloudLlmProviderDescriptor, CloudLlmProviderSettings,
  ModelManifest, ModelCacheStatus, ModelDownloadReport,
  BuildInfo, BookConfig, PrivacyConfig, SyncConfig, SearchConfig, CleanupConfig, LlmConfig,
  EmbeddingConfig, LocalAiDevicePolicy, LocalAiStatus,
  GitStatusDto, GitCommandReport, SyncActivityEvent, OperationalActivitySummary, NoteSyncActivity,
  CloudSyncProviderDescriptor, CloudSyncProviderStatus, CloudSyncConnectStart, CloudBookSummary,
  SyncReport, DeleteCurrentBookReport,
  World, Overlay, LookupEntry, ResolvedLocation, CreateImageWorldRequest, WorldDeletionImpact,
  LockMode, PolicyOverview,
  ExportSpec, PackManifest, ImportPreview, ImportOptions, ImportReport,
  PublishReport, GitignoreReport,
  BookStats, StyleCard, CrossBookNote, SearchFilter, CreateNoteOptions, NoteVisibility,
  TextImportOptions, TextImportPreview, TextImportCommitRequest, TextImportReport,
  PluginDescriptor, CommentaryKind, CommentarySummary, ApplyCommentaryOptions,
} from '../types';

export const api = {
  getVersion: () => invoke<string>('get_version'),
  getBuildInfo: () => invoke<BuildInfo>('get_build_info'),

  // Book config / settings. Updaters replace a whole sub-config — always pass the complete
  // object read from getBookConfig (omitted fields fall back to serde defaults, not disk values).
  getBookConfig: () => invoke<BookConfig>('get_book_config'),
  updatePrivacyConfig: (privacy: PrivacyConfig) =>
    invoke<BookConfig>('update_privacy_config', { privacy }),
  updateSyncConfig: (sync: SyncConfig) => invoke<BookConfig>('update_sync_config', { sync }),
  updateSearchConfig: (search: SearchConfig) =>
    invoke<BookConfig>('update_search_config', { search }),
  updateCleanupConfig: (cleanup: CleanupConfig) =>
    invoke<BookConfig>('update_cleanup_config', { cleanup }),
  updateLlmConfig: (llm: LlmConfig) => invoke<BookConfig>('update_llm_config', { llm }),
  updateEmbeddingConfig: (embedding: EmbeddingConfig) =>
    invoke<BookConfig>('update_embedding_config', { embedding }),
  getLocalAiDevicePolicy: () =>
    invoke<LocalAiDevicePolicy>('get_local_ai_device_policy'),
  updateLocalAiDevicePolicy: (policy: LocalAiDevicePolicy) =>
    invoke<LocalAiDevicePolicy>('update_local_ai_device_policy', { policy }),

  openBook: (path: string) => invoke<BookInfo>('open_book', { path }),
  listTrackedBooks: () => invoke<TrackedBookInfo[]>('list_tracked_books'),
  forgetTrackedBook: (path: string) => invoke<void>('forget_tracked_book', { path }),
  createBook: (path: string, name: string, language?: string, location?: string) =>
    invoke<BookInfo>('create_book', {
      path,
      name,
      language: language ?? null,
      location: location ?? null,
    }),
  createBookInParent: (parentPath: string, name: string, language?: string, location?: string) =>
    invoke<BookInfo>('create_book_in_parent', {
      parentPath,
      name,
      language: language ?? null,
      location: location ?? null,
    }),

  bookView: () => invoke<RenderItem[]>('book_view'),
  unsortedNotes: () => invoke<NoteDto[]>('unsorted_notes'),
  getNote: (id: string) => invoke<NoteDto>('get_note', { id }),
  renderNoteMarkdown: (request: { noteId?: string | null; markdown?: string | null }) =>
    invoke<string>('render_note_markdown', { request }),
  noteNeighbors: (noteId: string) => invoke<NoteNeighbors>('note_neighbors', { noteId }),
  listNotes: (visibility?: NoteVisibility) =>
    invoke<NoteDto[]>('list_notes', { visibility: visibility ?? null }),
  createNote: (objectType: ObjectType, title: string, inheritFrom?: string, options?: CreateNoteOptions) =>
    invoke<NoteDto>('create_note', {
      objectType,
      title,
      inheritFrom: inheritFrom ?? null,
      options: options ?? null,
    }),
  updateNote: (note: NoteDto) => invoke<NoteDto>('update_note', { note }),
  setPrior: (id: string, prior: PriorEdge | null) =>
    invoke<NoteDto>('set_prior', { id, prior }),
  forkNote: (id: string) => invoke<NoteDto>('fork_note', { id }),
  exportMarkdown: () => invoke<string>('export_markdown'),
  exportHtml: (path: string) => invoke<void>('export_html', { path }),
  exportMarkdownToFile: (path: string) => invoke<void>('export_markdown_to_file', { path }),
  bookStats: () => invoke<BookStats>('book_stats'),
  importAsset: (sourcePath: string) => invoke<string>('import_asset', { sourcePath }),
  importImageObject: (sourcePath: string, title?: string) =>
    invoke<NoteDto>('import_image_object', { sourcePath, title: title ?? null }),
  assetData: (assetUuid: string) => invoke<string | null>('asset_data', { assetUuid }),
  readTableData: (noteId: string) => invoke<string[][]>('read_table_data', { noteId }),
  saveTableData: (noteId: string, rows: string[][]) =>
    invoke<void>('save_table_data', { noteId, rows }),
  noteTokenCount: (request: { noteId?: string | null; text?: string | null }) =>
    invoke<NoteTokenCount>('note_token_count', { request }),
  noteEmbeddingDetails: (noteId: string) =>
    invoke<NoteEmbeddingDetails>('note_embedding_details', { noteId }),
  mergeNotes: (request: MergeNotesRequest) => invoke<NoteDto>('merge_notes', { request }),
  splitNote: (request: SplitNoteRequest) => invoke<SplitNoteResult>('split_note', { request }),
  listCommentary: (parentNoteId: string, includeResolved = false) =>
    invoke<CommentarySummary[]>('list_commentary', { parentNoteId, includeResolved }),
  getCommentary: (commentaryId: string) =>
    invoke<NoteDto>('get_commentary', { commentaryId }),
  createCommentary: (parentNoteId: string, kind: CommentaryKind, body: string) =>
    invoke<NoteDto>('create_commentary', { parentNoteId, kind, body }),
  updateCommentary: (commentary: NoteDto) =>
    invoke<NoteDto>('update_commentary', { commentary }),
  applyCommentary: (commentaryId: string, options?: ApplyCommentaryOptions) =>
    invoke<NoteDto>('apply_commentary', { commentaryId, options: options ?? null }),
  dismissCommentary: (commentaryId: string) =>
    invoke<NoteDto>('dismiss_commentary', { commentaryId }),
  pinCommentary: (commentaryId: string) =>
    invoke<NoteDto>('pin_commentary', { commentaryId }),

  allCategories: () => invoke<Category[]>('all_categories'),
  createCategory: (category: Category) => invoke<void>('create_category', { category }),
  deleteCategory: (name: string) => invoke<void>('delete_category', { name }),
  categoryEmbeddingStats: (name: string) => invoke<CategoryEmbeddingStats>('category_embedding_stats', { name }),

  // Search & embeddings
  search: (query: string, filter: SearchFilter) =>
    invoke<SearchResults>('search', { query, filter }),
  relatedNotes: (id: string) => invoke<RelatedNote[]>('related_notes', { id }),
  embeddingDiagnostics: () => invoke<EmbeddingDiagnostics>('embedding_diagnostics'),
  localAiStatus: () => invoke<LocalAiStatus>('local_ai_status'),
  enqueueAllStaleEmbeddings: () => invoke<number>('enqueue_all_stale_embeddings'),
  noteEditingFinished: (noteId: string) =>
    invoke<void>('note_editing_finished', { noteId }),
  graphAnalysis: (request: GraphAnalysisRequest) =>
    invoke<GraphAnalysisResult>('graph_analysis', { request }),
  searchAcrossBooks: (query: string) => invoke<CrossBookNote[]>('search_across_books', { query }),

  // LLM
  llmStatus: () => invoke<LlmStatus>('llm_status'),
  llmRouteStatuses: () => invoke<LlmRouteStatus[]>('llm_route_statuses'),
  cloudLlmProviderDescriptors: () =>
    invoke<CloudLlmProviderDescriptor[]>('cloud_llm_provider_descriptors'),
  saveCloudLlmProviderSettings: (settings: CloudLlmProviderSettings) =>
    invoke<void>('save_cloud_llm_provider_settings', { settings }),
  clearCloudLlmProviderSettings: (provider: string) =>
    invoke<void>('clear_cloud_llm_provider_settings', { provider }),
  testCloudLlmProviderConnection: (settings: CloudLlmProviderSettings) =>
    invoke<CloudLlmConnectionTestResult>('test_cloud_llm_provider_connection', { settings }),
  listCloudLlmProviderModels: (provider: string) =>
    invoke<CloudLlmModel[]>('list_cloud_llm_provider_models', { provider }),
  generateCloudProposal: (noteId: string, task: LlmTask, modelOverride?: ModelRef) =>
    invoke<Proposal>('generate_cloud_proposal', {
      noteId,
      task,
      modelOverride: modelOverride ?? null,
    }),
  generateProposal: (noteId: string, task: LlmTask, modelOverride?: ModelRef) =>
    invoke<Proposal>('generate_proposal', {
      noteId,
      task,
      modelOverride: modelOverride ?? null,
    }),
  enqueueLlmJob: (request: QueuedLlmJobRequest) =>
    invoke<QueuedLlmJobResult>('enqueue_llm_job', { request }),
  listLlmJobs: () => invoke<QueuedLlmJobResult[]>('list_llm_jobs'),
  listAllLlmJobs: () => invoke<QueuedLlmJobResult[]>('list_all_llm_jobs'),
  getLlmJob: (jobId: string) => invoke<QueuedLlmJobResult | null>('get_llm_job', { jobId }),
  acceptLlmJobResult: (jobId: string, storeOldAsCommentary = false, factCheckPassed = false) =>
    invoke<NoteDto>('accept_llm_job_result', {
      jobId,
      storeOldAsCommentary,
      factCheckPassed,
    }),
  dismissLlmJobResult: (jobId: string) =>
    invoke<void>('dismiss_llm_job_result', { jobId }),
  prepareCloudPrompt: (noteId: string, task: LlmTask, modelOverride?: ModelRef) =>
    invoke<CloudLlmPrompt>('prepare_cloud_prompt', {
      noteId,
      task,
      modelOverride: modelOverride ?? null,
    }),
  proposalFromCloudCompletion: (completion: CloudLlmCompletion) =>
    invoke<Proposal>('proposal_from_cloud_completion', { completion }),
  acceptProposal: (proposal: Proposal, storeOldAsCommentary = false, factCheckPassed = false) =>
    invoke<NoteDto>('accept_proposal', {
      proposal,
      storeOldAsCommentary,
      factCheckPassed,
    }),
  builtinModelManifests: () => invoke<ModelManifest[]>('builtin_model_manifests'),
  builtinModelCacheStatuses: (verifyHashes = false) =>
    invoke<ModelCacheStatus[]>('builtin_model_cache_statuses', { verifyHashes }),
  downloadBuiltinModel: (modelId: string) =>
    invoke<ModelDownloadReport>('download_builtin_model', { modelId }),

  // Spatial worlds & overlays (Phase 5)
  listWorlds: () => invoke<World[]>('list_worlds'),
  createImageWorld: (request: CreateImageWorldRequest) =>
    invoke<World>('create_image_world', { request }),
  worldDeletionImpact: (id: string) =>
    invoke<WorldDeletionImpact>('world_deletion_impact', { id }),
  deleteWorld: (id: string) => invoke<void>('delete_world', { id }),
  worldOverlay: (worldId: string) => invoke<Overlay>('world_overlay', { worldId }),
  worldBackdrop: (worldId: string) => invoke<string | null>('world_backdrop', { worldId }),
  locationLookup: () => invoke<LookupEntry[]>('location_lookup'),
  setLocationLookupEntry: (entry: LookupEntry) =>
    invoke<void>('set_location_lookup_entry', { entry }),
  resolveLocation: (token: string) => invoke<ResolvedLocation>('resolve_location', { token }),

  // Privacy & lifecycle (Phase 6)
  policyOverview: () => invoke<PolicyOverview>('policy_overview'),
  setNotePrivate: (id: string, isPrivate: boolean) =>
    invoke<NoteDto>('set_note_private', { id, private: isPrivate }),
  setNoteArchived: (id: string, archived: boolean) =>
    invoke<NoteDto>('set_note_archived', { id, archived }),
  setNoteLock: (id: string, mode: LockMode) => invoke<NoteDto>('set_note_lock', { id, mode }),
  setCategoryPrivate: (name: string, isPrivate: boolean) =>
    invoke<void>('set_category_private', { name, private: isPrivate }),
  requestDeletion: (id: string) => invoke<NoteDto>('request_deletion', { id }),
  restoreNote: (id: string) => invoke<NoteDto>('restore_note', { id }),
  purgeExpired: () => invoke<string[]>('purge_expired'),
  deleteImageObjectNow: (id: string) => invoke<void>('delete_image_object_now', { id }),

  // Knowledge packs (Phase 6)
  exportPack: (spec: ExportSpec, path: string) =>
    invoke<PackManifest>('export_pack', { spec, path }),
  readPackManifest: (path: string) => invoke<PackManifest>('read_pack_manifest', { path }),
  previewPack: (path: string) => invoke<ImportPreview>('preview_pack', { path }),
  importPack: (path: string, options: ImportOptions) =>
    invoke<ImportReport>('import_pack', { path, options }),
  importPackAsBook: (packPath: string, parentPath: string, bookName: string) =>
    invoke<BookInfo>('import_pack_as_book', {
      packPath,
      parentPath,
      bookName,
    }),

  // Text import
  readTextImportFile: (path: string) => invoke<string>('read_text_import_file', { path }),
  previewTextImport: (sourceText: string, options: TextImportOptions) =>
    invoke<TextImportPreview>('preview_text_import', { sourceText, options }),
  commitTextImport: (request: TextImportCommitRequest) =>
    invoke<TextImportReport>('commit_text_import', { request }),

  // Plugins (WASM)
  listPlugins: () => invoke<PluginDescriptor[]>('list_plugins'),
  setPluginEnabled: (pluginId: string, enabled: boolean) =>
    invoke<void>('set_plugin_enabled', { pluginId, enabled }),
  installUserPlugin: (sourcePath: string) => invoke<string>('install_user_plugin', { sourcePath }),
  runRenderPlugin: (language: string, code: string) =>
    invoke<string>('run_render_plugin', { language, code }),
  previewPluginImport: (pluginId: string, path: string, options: TextImportOptions) =>
    invoke<TextImportPreview>('preview_plugin_import', { pluginId, path, options }),

  // Publishing & serving (Phase 6)
  publishSite: (outDir: string) => invoke<PublishReport>('publish_site', { outDir }),
  refreshPrivateGitignore: () => invoke<GitignoreReport>('refresh_private_gitignore'),

  // Sync, git, file-watch, managed cloud
  gitStatus: () => invoke<GitStatusDto>('git_status'),
  gitInit: () => invoke<GitCommandReport>('git_init'),
  gitStageCommit: (selectedPaths: string[], message: string) =>
    invoke<GitCommandReport>('git_stage_commit', { selectedPaths, message }),
  gitPush: () => invoke<GitCommandReport>('git_push'),
  gitPull: () => invoke<GitCommandReport>('git_pull'),
  startFileWatch: () => invoke<void>('start_file_watch'),
  stopFileWatch: () => invoke<void>('stop_file_watch'),
  syncActivity: () => invoke<SyncActivityEvent[]>('sync_activity'),
  operationalActivitySummary: () =>
    invoke<OperationalActivitySummary>('operational_activity_summary'),
  noteSyncActivity: (noteId: string) =>
    invoke<NoteSyncActivity | null>('note_sync_activity', { noteId }),
  cloudSyncProviderDescriptors: () =>
    invoke<CloudSyncProviderDescriptor[]>('cloud_sync_provider_descriptors'),
  cloudSyncProviderStatuses: () =>
    invoke<CloudSyncProviderStatus[]>('cloud_sync_provider_statuses'),
  connectCloudSyncProvider: (provider: string) =>
    invoke<CloudSyncConnectStart>('connect_cloud_sync_provider', { provider }),
  disconnectCloudSyncProvider: (provider: string) =>
    invoke<CloudSyncProviderStatus>('disconnect_cloud_sync_provider', { provider }),
  listCloudBooks: (provider: string) =>
    invoke<CloudBookSummary[]>('list_cloud_books', { provider }),
  uploadBookToCloud: (provider: string) =>
    invoke<SyncReport>('upload_book_to_cloud', { provider }),
  syncManagedCloudNow: (provider: string) =>
    invoke<SyncReport>('sync_managed_cloud_now', { provider }),
  openCloudBook: (provider: string, bookId: string, parentPath: string) =>
    invoke<void>('open_cloud_book', { provider, bookId, parentPath }),
  deleteCurrentBook: (expectedBookName: string) =>
    invoke<DeleteCurrentBookReport>('delete_current_book', { expectedBookName }),

  // Style cards
  listStyleCards: () => invoke<StyleCard[]>('list_style_cards'),
  saveStyleCard: (card: StyleCard) => invoke<StyleCard>('save_style_card', { card }),
  deleteStyleCard: (id: string) => invoke<void>('delete_style_card', { id }),
};
