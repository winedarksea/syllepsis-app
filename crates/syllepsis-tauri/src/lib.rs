//! Tauri app library: registers state, plugins, and all IPC commands.

pub mod commands;
pub mod state;

use commands::{
    book::*, categories::*, cloud_llm::*, llm::*, notes::*, search::*, spatial::*, sync::*,
};
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            // book lifecycle
            get_version,
            open_book,
            create_book,
            // notes
            book_view,
            unsorted_notes,
            get_note,
            list_notes,
            create_note,
            update_note,
            set_prior,
            fork_note,
            delete_note,
            export_markdown,
            // categories
            all_categories,
            create_category,
            // search & embeddings
            search,
            related_notes,
            embedding_diagnostics,
            // llm
            llm_status,
            llm_route_statuses,
            cloud_llm_provider_descriptors,
            cloud_llm_provider_statuses,
            save_cloud_llm_provider_settings,
            clear_cloud_llm_provider_settings,
            generate_proposal,
            prepare_cloud_prompt,
            proposal_from_cloud_completion,
            accept_proposal,
            builtin_model_manifests,
            download_builtin_model,
            // sync (Phase 4)
            sync_to_folder,
            sync_status,
            sync_provider_descriptors,
            // spatial worlds (Phase 5)
            list_worlds,
            create_world,
            delete_world,
            world_overlay,
            location_lookup,
            set_location_lookup_entry,
            resolve_location,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Syllepsis");
}
