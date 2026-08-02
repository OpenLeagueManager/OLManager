use log::info;
use tauri::State;

use olm_core::end_of_season::EndOfSeasonSummary;
use olm_core::generator::definitions::CompetitionManifest;
use olm_core::state::StateManager;

#[tauri::command]
pub fn check_season_complete(state: State<'_, StateManager>) -> Result<bool, String> {
    log::debug!("[cmd] check_season_complete");
    let game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;
    Ok(olm_core::end_of_season::is_season_complete(&game))
}

/// Try to load the competition manifest from the globally-resolved resource dir.
fn resolve_competition_manifest(game: &olm_core::game::Game) -> Option<CompetitionManifest> {
    let competition_id = game.user_competition_id.as_deref()?;
    let data_dir = olm_core::state::RESOURCE_DATA_DIR.get()?;
    let path = data_dir
        .join("competitions")
        .join(competition_id)
        .join("manifest.json");
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<CompetitionManifest>(&json).ok()
}

#[tauri::command]
pub fn advance_to_next_season(state: State<'_, StateManager>) -> Result<serde_json::Value, String> {
    info!("[cmd] advance_to_next_season");
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    if !olm_core::end_of_season::is_season_complete(&game) {
        return Err("Season is not yet complete".to_string());
    }

    // Try to resolve competition manifest for manifest-driven flow
    let manifest = resolve_competition_manifest(&game);
    let mut was_season_end: bool;

    let summary = if let Some(ref manifest) = manifest {
        let is_wrapping = game
            .active_league()
            .is_some_and(|league| league.split_index + 1 >= manifest.schedule.splits.len());

        was_season_end = is_wrapping;

        if is_wrapping {
            info!("[cmd] advance_to_next_season: season complete, running full end-of-season");
            let summary = olm_core::end_of_season::process_end_of_season_with_config(
                &mut game,
                Some(manifest),
            );
            // After season-end processing, generate split 0 of the new season
            olm_core::end_of_season::process_end_of_split(&mut game, manifest);
            summary
        } else {
            was_season_end = false;
            info!("[cmd] advance_to_next_season: split complete, advancing to next split");
            // Build a meaningful summary from current standings before advancing
            let pre_advance_league = game.active_league().cloned();
            let pre_advance_standings = pre_advance_league
                .as_ref()
                .map(|l| l.sorted_standings())
                .unwrap_or_default();
            let pre_advance_user_team_id = game.manager.team_id.clone();
            let pre_advance_user_pos = pre_advance_user_team_id
                .as_deref()
                .and_then(|tid| pre_advance_standings.iter().position(|s| s.team_id == tid))
                .map(|i| i + 1)
                .unwrap_or(0);
            let pre_advance_user_st = pre_advance_user_pos
                .checked_sub(1)
                .and_then(|i| pre_advance_standings.get(i));
            let pre_advance_champion = pre_advance_standings.first();
            let pre_advance_champion_name = pre_advance_champion
                .and_then(|c| game.teams.iter().find(|t| t.id == c.team_id))
                .map(|t| t.name.clone())
                .unwrap_or_default();

            // Increment split, then generate its schedule
            if let Some(l) = game.active_league_mut() {
                l.split_index += 1;
            }
            olm_core::end_of_season::process_end_of_split(&mut game, manifest);
            EndOfSeasonSummary {
                season: pre_advance_league.as_ref().map(|l| l.season).unwrap_or(0),
                league_name: pre_advance_league
                    .as_ref()
                    .map(|l| l.name.clone())
                    .unwrap_or_default(),
                champion_id: pre_advance_champion
                    .map(|c| c.team_id.clone())
                    .unwrap_or_default(),
                champion_name: pre_advance_champion_name,
                user_position: pre_advance_user_pos as u32,
                user_points: pre_advance_user_st.map(|s| s.points).unwrap_or(0),
                user_won: pre_advance_user_st.map(|s| s.won).unwrap_or(0),
                user_lost: pre_advance_user_st.map(|s| s.lost).unwrap_or(0),
                user_maps_won: pre_advance_user_st.map(|s| s.maps_won).unwrap_or(0),
                user_maps_lost: pre_advance_user_st.map(|s| s.maps_lost).unwrap_or(0),
                total_teams: pre_advance_standings.len() as u32,
                ..Default::default()
            }
        }
    } else {
        was_season_end = true;
        info!("[cmd] advance_to_next_season: using legacy schedule");
        olm_core::end_of_season::process_end_of_season(&mut game)
    };

    // Process background league seasons
    {
        let manifests = game.competition_configs.clone();
        olm_core::end_of_season::process_background_seasons(&mut game, &manifests);
    }

    // End-of-season objective evaluation may have dropped satisfaction — check firing
    if was_season_end {
        olm_core::firing::check_manager_firing(&mut game);
    }

    state.set_game(game.clone());

    if game.manager.team_id.is_none() {
        return Ok(serde_json::json!({
            "action": "fired",
            "game": crate::client_game::game_for_client(&game),
            "summary": summary,
        }));
    }

    Ok(serde_json::json!({
        "game": crate::client_game::game_for_client(&game),
        "summary": summary,
    }))
}

#[tauri::command]
pub fn get_season_awards(
    state: State<'_, StateManager>,
) -> Result<olm_core::season_awards::SeasonAwards, String> {
    log::debug!("[cmd] get_season_awards");
    let game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;
    Ok(olm_core::season_awards::compute_season_awards(&game))
}
