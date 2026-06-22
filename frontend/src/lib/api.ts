// Typed wrappers around tauri invoke. Each function mirrors a #[tauri::command] on the
// Rust side. Replace with tauri-specta generated bindings once binding generation is wired.

import { invoke } from '@tauri-apps/api/core';
import type {
  BookInfo, Category, NoteDto, ObjectType, PriorEdge, RenderItem,
  SearchResults, RelatedNote, EmbeddingDiagnostics,
  LlmStatus, LlmRouteStatus, LlmTask, ModelRef, Proposal, CloudLlmPrompt, CloudLlmCompletion,
  CloudLlmProviderDescriptor, CloudLlmProviderSettings, CloudLlmProviderStatus,
  ModelManifest, ModelDownloadReport,
  World, Overlay, LookupEntry, ResolvedLocation,
} from '../types';

export const api = {
  getVersion: () => invoke<string>('get_version'),

  openBook: (path: string) => invoke<BookInfo>('open_book', { path }),
  createBook: (path: string, name: string) => invoke<BookInfo>('create_book', { path, name }),

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
  acceptProposal: (proposal: Proposal, storeOldAsCommentary = false) =>
    invoke<NoteDto>('accept_proposal', {
      proposal,
      store_old_as_commentary: storeOldAsCommentary,
    }),
  builtinModelManifests: () => invoke<ModelManifest[]>('builtin_model_manifests'),
  downloadBuiltinModel: (modelId: string) =>
    invoke<ModelDownloadReport>('download_builtin_model', { model_id: modelId }),

  // Spatial worlds & overlays (Phase 5)
  listWorlds: () => invoke<World[]>('list_worlds'),
  createWorld: (world: World) => invoke<void>('create_world', { world }),
  deleteWorld: (id: string) => invoke<void>('delete_world', { id }),
  worldOverlay: (worldId: string) => invoke<Overlay>('world_overlay', { world_id: worldId }),
  locationLookup: () => invoke<LookupEntry[]>('location_lookup'),
  setLocationLookupEntry: (entry: LookupEntry) =>
    invoke<void>('set_location_lookup_entry', { entry }),
  resolveLocation: (token: string) => invoke<ResolvedLocation>('resolve_location', { token }),
};
