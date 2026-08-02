mod application;
mod client_game;
mod commands;
mod discord_rpc;
pub mod error;
use commands::*;

use crate::discord_rpc::DiscordRpcState;
use olm_core::db::save_manager::SaveManager;
use olm_core::sim_live::SimLiveStoreState;
use olm_core::state::StateManager;
use std::path::Path;
use std::sync::Mutex;

/// Tauri-managed wrapper around SaveManager.
pub struct SaveManagerState(pub Mutex<SaveManager>);

/// Minimal percent-decoder for `olm-asset://` request paths (filenames may
/// contain encoded spaces or unicode). Path separators are not encoded.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let hex = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    };
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;

    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            if let Some(parent) = destination_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&source_path, &destination_path)?;
        }
    }

    Ok(())
}

fn contains_competition_manifest(data_dir: &Path) -> bool {
    let competitions_dir = data_dir.join("competitions");
    let Ok(entries) = std::fs::read_dir(competitions_dir) else {
        return false;
    };

    entries
        .filter_map(Result::ok)
        .any(|entry| entry.path().join("manifest.json").is_file())
}

fn seed_bundled_data_if_missing(app: &tauri::App) {
    use tauri::Manager as TauriManager;

    let Ok(app_data_dir) = app.path().app_data_dir() else {
        log::warn!("[setup] Could not resolve app data dir; skipping bundled data seed");
        return;
    };

    let destination = app_data_dir.join("data");
    if contains_competition_manifest(&destination) {
        log::info!("[setup] App data directory already contains competition data; skipping seed");
        return;
    }

    let Ok(resource_dir) = app.path().resource_dir() else {
        log::warn!("[setup] Could not resolve resource dir; skipping bundled data seed");
        return;
    };

    let source = resource_dir.join("data");
    if contains_competition_manifest(&source) {
        match copy_dir_recursive(&source, &destination) {
            Ok(()) => {
                log::info!(
                    "[setup] Seeded bundled data from {:?} into {:?}",
                    source,
                    destination
                );
                return;
            }
            Err(error) => {
                log::error!(
                    "[setup] Failed to seed bundled data from {:?} into {:?}: {}",
                    source,
                    destination,
                    error
                );
            }
        }
    }

    log::warn!(
        "[setup] Bundled data directory not found in resources; league selection may be empty"
    );
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    eprintln!("[OLManager] BUILD: format-v2-fix");
    // Workaround for WebKitGTK DMABuf rendering issues on Wayland (Linux)
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("olmanager_lib", log::LevelFilter::Debug)
                .level_for("olm_core", log::LevelFilter::Debug)
                .level_for("engine", log::LevelFilter::Debug)
                .level_for("db", log::LevelFilter::Debug)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                .max_file_size(5_000_000) // 5 MB per log file
                .build(),
        )
        .register_uri_scheme_protocol("olm-asset", |ctx, request| {
            use tauri::http::Response;
            let rel = percent_decode(request.uri().path().trim_start_matches('/'));
            match commands::import::resolve_public_asset(ctx.app_handle(), &rel) {
                Some((bytes, mime)) => Response::builder()
                    .status(200)
                    .header("Content-Type", mime)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(bytes)
                    .unwrap_or_else(|_| Response::new(Vec::new())),
                None => Response::builder()
                    .status(404)
                    .body(Vec::new())
                    .unwrap_or_else(|_| Response::new(Vec::new())),
            }
        })
        .manage(StateManager::new())
        .manage(SimLiveStoreState::default())
        .setup(|app| {
            use tauri::Manager as TauriManager;
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data dir");
            seed_bundled_data_if_missing(app);

            let saves_dir = app_data_dir.join("saves");
            let mut save_manager =
                SaveManager::init(&saves_dir).expect("Failed to initialize SaveManager");

            // Run legacy migration if old saves.db exists
            if olm_core::db::legacy_migration::has_legacy_db(&app_data_dir) {
                log::info!("[setup] Legacy saves.db detected, migrating...");
                match olm_core::db::legacy_migration::migrate_legacy_saves(&app_data_dir, &mut save_manager) {
                    Ok(results) => {
                        let success = results
                            .iter()
                            .filter(|r| {
                                matches!(
                                    r,
                                    olm_core::db::legacy_migration::LegacyMigrationResult::Success { .. }
                                )
                            })
                            .count();
                        let failed = results
                            .iter()
                            .filter(|r| {
                                matches!(
                                    r,
                                    olm_core::db::legacy_migration::LegacyMigrationResult::Failed { .. }
                                )
                            })
                            .count();
                        log::info!(
                            "[setup] Legacy migration complete: {} succeeded, {} failed",
                            success,
                            failed
                        );
                    }
                    Err(e) => log::error!("[setup] Legacy migration failed: {}", e),
                }
            }

            app.manage(SaveManagerState(Mutex::new(save_manager)));
            app.manage(DiscordRpcState::new());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_new_game_lightweight,
            select_team,
            get_saves,
            load_game,
            get_active_game,
            get_champions,
            get_team_selection_data,
            get_league_selection_data,
            get_academy_acquisition_options,
            acquire_academy_team,
            promote_academy_player,
            demote_main_player_to_academy,

            advance_time,
            advance_time_with_mode,
            upgrade_facility,
            upgrade_main_facility_module,
            expand_main_facility_hub,
            propose_renewal,
            delegate_renewals,
            preview_renewal_financial_impact,
            set_active_lineup,
            set_starting_xi,
            set_draft_strategy,
            set_lol_tactics,
            set_team_roles,
            set_training,
            set_training_schedule,
            set_training_groups,
            set_weekly_scrims,
            set_weekly_scrim_plans,
            set_weekly_scrim_slots,
            set_weekly_scrim_objective,
            finalize_weekly_scrim_setup,
            auto_configure_weekly_scrim_setup,
            get_scrim_context,
            cancel_todays_scrims,
            choose_post_scrim_decision,
            choose_daily_scrim_action,
            delegate_scrim_decision,
            set_player_training_focus,
            set_player_champion_training_target,
            delegate_champion_training,
            get_soloq_statuses,
            start_potential_research,
            reroll_player_lol_role,
            hire_staff,
            release_staff,
            mark_message_read,
            delete_message,
            delete_messages,
            mark_all_messages_read,
            clear_old_messages,
            save_game,
            auto_select_team_roles,
            toggle_transfer_list,
            toggle_loan_list,
            make_transfer_bid,
            preview_transfer_bid_financial_impact,
            respond_to_offer,
            counter_offer,
            negotiate_player_wage,
            get_transfer_history_cmd,
            release_player_contract,
            send_scout,
            check_season_complete,
            advance_to_next_season,
            get_season_awards,
            resolve_message_action,
            start_live_match,
            evaluate_draft_state,
            get_player_match_history,
            get_player_stats_overview,
            get_team_match_history,
            get_team_stats_overview,
            step_live_match,
            apply_match_command,
            get_match_snapshot,
            finish_live_match,
            record_fixture_champion_picks,
            apply_champion_mastery_from_draft,
            delete_save,
            skip_to_match_day,
            check_blocking_actions,
            export_bug_report,
            apply_team_talk,
            submit_press_conference,
            exit_to_menu,
            get_settings,
            save_settings,
            get_social_feed,
            create_manager_social_post,
            get_social_accounts,
            save_social_accounts,
            get_social_templates,
            save_social_templates,
            relocalize_social_feed,
            clear_all_saves,
            get_available_jobs,
            apply_for_job,
            sim_live_init,
            sim_live_tick,
            sim_live_reset,
            sim_live_dispose,
            sim_live_run_to_completion,
            sim_live_skip_to_end,
            auto_import_database,
            import_export_zip,
            import_cached_export,
            get_import_cache_info,
            get_catalog_summary,
            get_catalog,
            init_discord_rpc,
            update_discord_presence,
            shutdown_discord_rpc,
            debug_log,
            update_manager_profile,
            debug_serde_test,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
