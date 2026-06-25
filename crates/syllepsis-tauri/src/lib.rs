//! Tauri app library: registers state, plugins, and all IPC commands.

pub mod commands;
pub mod local_ai;
mod model_bootstrap;
pub mod state;

use commands::{
    book::*, categories::*, cloud_llm::*, config::*, lifecycle::*, llm::*, local_ai::*, notes::*,
    pack::*, plugins::*, publish::*, search::*, spatial::*, style_cards::*, sync::*,
    text_import::*,
};
use state::AppState;
use tauri::Manager;

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
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            // Discover and load WASM plugins once at startup, then share them as app state.
            let (builtin_dir, user_dir) = commands::plugins::plugin_dirs(app.handle());
            let prefs_path = app
                .path()
                .app_data_dir()
                .map(|d| d.join("plugin_prefs.json"))
                .unwrap_or_default();
            let runtime = commands::plugins::PluginRuntime::load(builtin_dir, user_dir, prefs_path);
            app.manage(runtime);
            if let Ok(app_data_dir) = app.path().app_data_dir() {
                app.state::<AppState>()
                    .local_ai
                    .configure_preferences_path(app_data_dir.join("local-ai-device-policy.json"));
            }
            model_bootstrap::provision_default_embedding_model(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // book lifecycle
            get_version,
            get_build_info,
            open_book,
            create_book,
            create_book_in_parent,
            list_tracked_books,
            forget_tracked_book,
            // book config / settings
            get_book_config,
            update_privacy_config,
            update_sync_config,
            update_search_config,
            update_cleanup_config,
            update_llm_config,
            update_embedding_config,
            get_local_ai_device_policy,
            update_local_ai_device_policy,
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
            import_image_object,
            asset_data,
            read_table_data,
            save_table_data,
            // categories
            all_categories,
            create_category,
            // search & embeddings
            search,
            related_notes,
            embedding_diagnostics,
            local_ai_status,
            enqueue_all_stale_embeddings,
            note_editing_finished,
            graph_analysis,
            search_across_books,
            // llm
            llm_status,
            llm_route_statuses,
            cloud_llm_provider_descriptors,
            save_cloud_llm_provider_settings,
            clear_cloud_llm_provider_settings,
            test_cloud_llm_provider_connection,
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
            git_status,
            git_init,
            git_stage_commit,
            git_push,
            git_pull,
            start_file_watch,
            stop_file_watch,
            sync_activity,
            operational_activity_summary,
            note_sync_activity,
            cloud_sync_provider_descriptors,
            cloud_sync_provider_statuses,
            connect_cloud_sync_provider,
            disconnect_cloud_sync_provider,
            list_cloud_books,
            upload_book_to_cloud,
            open_cloud_book,
            sync_managed_cloud_now,
            delete_current_book,
            // spatial worlds (Phase 5)
            list_worlds,
            create_image_world,
            world_deletion_impact,
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
            // plugins (WASM)
            list_plugins,
            set_plugin_enabled,
            install_user_plugin,
            run_render_plugin,
            preview_plugin_import,
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
