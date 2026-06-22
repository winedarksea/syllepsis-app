// Typed wrappers around tauri invoke. Each function mirrors a #[tauri::command] on the
// Rust side. Replace with tauri-specta generated bindings once binding generation is wired.

import { invoke } from '@tauri-apps/api/core';
import type {
  BookInfo, TrackedBookInfo, Category, NoteDto, ObjectType, PriorEdge, RenderItem,
  SearchResults, RelatedNote, EmbeddingDiagnostics,
  LlmStatus, LlmRouteStatus, LlmTask, ModelRef, Proposal, CloudLlmPrompt, CloudLlmCompletion,
  CloudLlmProviderDescriptor, CloudLlmProviderSettings, CloudLlmProviderStatus,
  ModelManifest, ModelCacheStatus, ModelDownloadReport,
  World, Overlay, LookupEntry, ResolvedLocation,
  LockMode, PolicyOverview,
  ExportSpec, PackManifest, ImportPreview, ImportOptions, ImportReport,
  PublishReport, GitignoreReport,
} from '../types';

export const api = {
  getVersion: () => invoke<string>('get_version'),

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
      parent_path: parentPath,
      name,
      language: language ?? null,
      location: location ?? null,
    }),

  bookView: () => invoke<RenderItem[]>('book_view'),
  unsortedNotes: () => invoke<NoteDto[]>('unsorted_notes'),
  getNote: (id: string) => invoke<NoteDto>('get_note', { id }),
  listNotes: () => invoke<NoteDto[]>('list_notes'),
  createNote: (objectType: ObjectType, title: string, inheritFrom?: string) =>
    invoke<NoteDto>('create_note', { object_type: objectType, title, inherit_from: inheritFrom ?? null }),
  updateNote: (note: NoteDto) => invoke<NoteDto>('update_note', { note }),
  setPrior: (id: string, prior: PriorEdge | null) =>
    invoke<NoteDto>('set_prior', { id, prior }),
  forkNote: (id: string) => invoke<NoteDto>('fork_note', { id }),
  deleteNote: (id: string) => invoke<void>('delete_note', { id }),
  exportMarkdown: () => invoke<string>('export_markdown'),

  allCategories: () => invoke<Category[]>('all_categories'),
  createCategory: (category: Category) => invoke<void>('create_category', { category }),

  // Search & embeddings
  search: (query: string, categoryFilter: string[] = []) =>
    invoke<SearchResults>('search', { query, category_filter: categoryFilter }),
  relatedNotes: (id: string) => invoke<RelatedNote[]>('related_notes', { id }),
  embeddingDiagnostics: () => invoke<EmbeddingDiagnostics>('embedding_diagnostics'),

  // LLM
  llmStatus: () => invoke<LlmStatus>('llm_status'),
  llmRouteStatuses: () => invoke<LlmRouteStatus[]>('llm_route_statuses'),
  cloudLlmProviderDescriptors: () =>
    invoke<CloudLlmProviderDescriptor[]>('cloud_llm_provider_descriptors'),
  cloudLlmProviderStatuses: () =>
    invoke<CloudLlmProviderStatus[]>('cloud_llm_provider_statuses'),
  saveCloudLlmProviderSettings: (settings: CloudLlmProviderSettings) =>
    invoke<CloudLlmProviderStatus>('save_cloud_llm_provider_settings', { settings }),
  clearCloudLlmProviderSettings: (provider: string) =>
    invoke<CloudLlmProviderStatus>('clear_cloud_llm_provider_settings', { provider }),
  generateCloudProposal: (noteId: string, task: LlmTask, modelOverride?: ModelRef) =>
    invoke<Proposal>('generate_cloud_proposal', {
      note_id: noteId,
      task,
      model_override: modelOverride ?? null,
    }),
  generateProposal: (noteId: string, task: LlmTask, modelOverride?: ModelRef) =>
    invoke<Proposal>('generate_proposal', {
      note_id: noteId,
      task,
      model_override: modelOverride ?? null,
    }),
  prepareCloudPrompt: (noteId: string, task: LlmTask, modelOverride?: ModelRef) =>
    invoke<CloudLlmPrompt>('prepare_cloud_prompt', {
      note_id: noteId,
      task,
      model_override: modelOverride ?? null,
    }),
  proposalFromCloudCompletion: (completion: CloudLlmCompletion) =>
    invoke<Proposal>('proposal_from_cloud_completion', { completion }),
  acceptProposal: (proposal: Proposal, storeOldAsCommentary = false, factCheckPassed = false) =>
    invoke<NoteDto>('accept_proposal', {
      proposal,
      store_old_as_commentary: storeOldAsCommentary,
      fact_check_passed: factCheckPassed,
    }),
  builtinModelManifests: () => invoke<ModelManifest[]>('builtin_model_manifests'),
  builtinModelCacheStatuses: (verifyHashes = false) =>
    invoke<ModelCacheStatus[]>('builtin_model_cache_statuses', { verify_hashes: verifyHashes }),
  downloadBuiltinModel: (modelId: string) =>
    invoke<ModelDownloadReport>('download_builtin_model', { model_id: modelId }),

  // Spatial worlds & overlays (Phase 5)
  listWorlds: () => invoke<World[]>('list_worlds'),
  createWorld: (world: World) => invoke<void>('create_world', { world }),
  deleteWorld: (id: string) => invoke<void>('delete_world', { id }),
  worldOverlay: (worldId: string) => invoke<Overlay>('world_overlay', { world_id: worldId }),
  worldBackdrop: (worldId: string) => invoke<string | null>('world_backdrop', { world_id: worldId }),
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

  // Knowledge packs (Phase 6)
  exportPack: (spec: ExportSpec, path: string) =>
    invoke<PackManifest>('export_pack', { spec, path }),
  readPackManifest: (path: string) => invoke<PackManifest>('read_pack_manifest', { path }),
  previewPack: (path: string) => invoke<ImportPreview>('preview_pack', { path }),
  importPack: (path: string, options: ImportOptions) =>
    invoke<ImportReport>('import_pack', { path, options }),
  importPackAsBook: (packPath: string, parentPath: string, bookName: string) =>
    invoke<BookInfo>('import_pack_as_book', {
      pack_path: packPath,
      parent_path: parentPath,
      book_name: bookName,
    }),

  // Publishing & serving (Phase 6)
  publishSite: (outDir: string) => invoke<PublishReport>('publish_site', { out_dir: outDir }),
  refreshPrivateGitignore: () => invoke<GitignoreReport>('refresh_private_gitignore'),
};
