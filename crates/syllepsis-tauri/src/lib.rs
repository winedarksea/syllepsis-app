//! Tauri app library: registers state, plugins, and all IPC commands.

pub mod commands;
pub mod state;

use commands::{
    book::*, categories::*, cloud_llm::*, lifecycle::*, llm::*, notes::*, pack::*, publish::*,
    search::*, spatial::*, style_cards::*, sync::*, text_import::*,
};
use state::AppState;

/// Initialize tracing so "fancier" operations (LLM calls, search) log to the console in
/// `tauri dev`. Defaults to `info` in debug builds and `warn` in release; override with `RUST_LOG`.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let default_level = if cfg!(debug_assertions) {
        "info"
    } else {
        "warn"
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "syllepsis_core={default_level},syllepsis_tauri_lib={default_level}"
        ))
    });
    // `try_init` is idempotent-friendly: ignore the error if a subscriber is already set.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            // book lifecycle
            get_version,
            open_book,
            create_book,
            create_book_in_parent,
            list_tracked_books,
            forget_tracked_book,
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
            export_html,
            export_markdown_to_file,
            book_stats,
            import_asset,
            read_table_data,
            save_table_data,
            // categories
            all_categories,
            create_category,
            // search & embeddings
            search,
            related_notes,
            embedding_diagnostics,
            search_across_books,
            // llm
            llm_status,
            llm_route_statuses,
            cloud_llm_provider_descriptors,
            cloud_llm_provider_statuses,
            save_cloud_llm_provider_settings,
            clear_cloud_llm_provider_settings,
            generate_cloud_proposal,
            generate_proposal,
            prepare_cloud_prompt,
            proposal_from_cloud_completion,
            accept_proposal,
            builtin_model_manifests,
            builtin_model_cache_statuses,
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
            world_backdrop,
            location_lookup,
            set_location_lookup_entry,
            resolve_location,
            // privacy & lifecycle (Phase 6)
            policy_overview,
            set_note_private,
            set_note_archived,
            set_note_lock,
            set_category_private,
            request_deletion,
            restore_note,
            purge_expired,
            // knowledge packs (Phase 6)
            export_pack,
            read_pack_manifest,
            preview_pack,
            import_pack,
            import_pack_as_book,
            // text import
            read_text_import_file,
            preview_text_import,
            commit_text_import,
            // publishing & serving (Phase 6)
            publish_site,
            refresh_private_gitignore,
            // style cards
            list_style_cards,
            save_style_card,
            delete_style_card,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Syllepsis");
}
