use chrono::Utc;
use log::info;
use olm_core::academy::{
    academy_candidate_catalog, academy_erl_catalog, eligible_academy_acquisition_options,
    validate_academy_acquisition, AcademyAcquisitionOption,
};
use olm_core::domain::team::{Team, TeamKind};
use olm_core::finances::{record_transaction, BudgetImpact, FinanceTransactionInput};
use olm_core::game::Game;
use olm_core::state::StateManager;
use serde::{Deserialize, Serialize};
use tauri::State;

use olm_core::game_setup::ensure_example_academy_pool;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcademyAcquisitionOptionsResponse {
    pub parent_team_id: String,
    pub acquisition_allowed: bool,
    pub blocked_reason: Option<String>,
    pub options: Vec<AcademyAcquisitionOption>,
}

pub type AcademyCreationOptionsResponse = AcademyAcquisitionOptionsResponse;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquireAcademyTeamRequest {
    pub parent_team_id: String,
    pub source_team_id: String,
    pub custom_name: Option<String>,
    pub custom_short_name: Option<String>,
    pub custom_logo_url: Option<String>,
}

#[tauri::command]
pub fn get_academy_acquisition_options(
    state: State<'_, StateManager>,
    parent_team_id: String,
) -> Result<AcademyAcquisitionOptionsResponse, String> {
    info!(
        "[cmd] get_academy_acquisition_options: parent_team_id={}",
        parent_team_id
    );
    let mut game = state
        .get_game(|game| game.clone())
        .ok_or("No active game session".to_string())?;

    ensure_example_academy_pool(&mut game);
    state.set_game(game.clone());

    get_academy_acquisition_options_for_game(&game, &parent_team_id)
}

#[tauri::command]
pub fn acquire_academy_team(
    state: State<'_, StateManager>,
    request: AcquireAcademyTeamRequest,
) -> Result<Game, String> {
    info!(
        "[cmd] acquire_academy_team: parent_team_id={}, source_team_id={}",
        request.parent_team_id, request.source_team_id
    );
    let mut game = state
        .get_game(|game| game.clone())
        .ok_or("No active game session".to_string())?;
    ensure_example_academy_pool(&mut game);
    let updated = acquire_academy_team_in_game(&mut game, request)?;
    state.set_game(updated.clone());
    Ok(updated)
}

#[tauri::command]
pub fn promote_academy_player(
    state: State<'_, StateManager>,
    player_id: String,
) -> Result<Game, String> {
    info!("[cmd] promote_academy_player: player_id={}", player_id);
    let mut game = state
        .get_game(|game| game.clone())
        .ok_or("No active game session".to_string())?;

    let parent_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let academy_team_id =
        olm_core::academy::resolve_manager_academy_team_id(&game, &parent_team_id)?;

    let (moved_player_id, moved_player_name) = {
        let player = game
            .players
            .iter_mut()
            .find(|candidate| candidate.id == player_id)
            .ok_or_else(|| format!("Player '{}' not found", player_id))?;

        if player.team_id.as_deref() != Some(academy_team_id.as_str()) {
            return Err("Player does not belong to your academy team".to_string());
        }

        player.team_id = Some(parent_team_id.clone());
        (player.id.clone(), player.match_name.clone())
    };

    olm_core::academy::push_academy_player_moved_message(
        &mut game,
        "academy-promote",
        &parent_team_id,
        &moved_player_id,
        &moved_player_name,
        "Promocion desde la academia",
        "Subiste al jugador {player} desde la academia al equipo principal.",
    );

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn demote_main_player_to_academy(
    state: State<'_, StateManager>,
    player_id: String,
) -> Result<Game, String> {
    info!(
        "[cmd] demote_main_player_to_academy: player_id={}",
        player_id
    );
    let mut game = state
        .get_game(|game| game.clone())
        .ok_or("No active game session".to_string())?;

    let parent_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let academy_team_id =
        olm_core::academy::resolve_manager_academy_team_id(&game, &parent_team_id)?;

    let (moved_player_id, moved_player_name) = {
        let player = game
            .players
            .iter_mut()
            .find(|candidate| candidate.id == player_id)
            .ok_or_else(|| format!("Player '{}' not found", player_id))?;

        if player.team_id.as_deref() != Some(parent_team_id.as_str()) {
            return Err("Player does not belong to your main team".to_string());
        }

        player.team_id = Some(academy_team_id.clone());
        (player.id.clone(), player.match_name.clone())
    };

    olm_core::academy::push_academy_player_moved_message(
        &mut game,
        "academy-demote",
        &parent_team_id,
        &moved_player_id,
        &moved_player_name,
        "Jugador enviado a la academia",
        "Bajaste al jugador {player} del equipo principal a la academia.",
    );

    state.set_game(game.clone());
    Ok(game)
}

pub(crate) fn get_academy_acquisition_options_for_game(
    game: &Game,
    parent_team_id: &str,
) -> Result<AcademyAcquisitionOptionsResponse, String> {
    let parent = olm_core::academy::find_team(game, parent_team_id)?;
    let normalize_key = |value: &str| {
        value
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .flat_map(|ch| ch.to_lowercase())
            .collect::<String>()
    };
    let occupied_source_ids: std::collections::HashSet<String> = game
        .teams
        .iter()
        .filter(|team| team.team_kind == TeamKind::Academy && team.parent_team_id.is_some())
        .flat_map(|team| {
            let mut ids = vec![team.id.clone()];
            if let Some(metadata) = team.academy.as_ref() {
                ids.push(metadata.source_team_id.clone());
            }
            ids
        })
        .collect();
    let taken_original_names: std::collections::HashSet<String> = game
        .teams
        .iter()
        .filter(|team| team.team_kind == TeamKind::Academy && team.parent_team_id.is_some())
        .filter_map(|team| {
            team.academy
                .as_ref()
                .map(|metadata| normalize_key(&metadata.original_name))
        })
        .collect();

    let options = eligible_academy_acquisition_options(
        &parent.country,
        academy_erl_catalog(),
        academy_candidate_catalog(),
    )
    .into_iter()
    .filter(|option| {
        !occupied_source_ids.contains(&option.source_team_id)
            && !taken_original_names.contains(&normalize_key(&option.name))
    })
    .collect::<Vec<_>>();
    let blocked_reason = if !parent.is_main() {
        Some("Academy can only be acquired for a main team".to_string())
    } else if parent.academy_team_id.is_some() {
        Some("Parent team already has academy".to_string())
    } else if options.is_empty() {
        Some("No free academy candidates available in LES, LFL, or Prime League".to_string())
    } else if options
        .iter()
        .all(|option| parent.finance < option.acquisition_cost)
    {
        Some("Insufficient funds for all eligible academy acquisition options".to_string())
    } else {
        None
    };

    Ok(AcademyAcquisitionOptionsResponse {
        parent_team_id: parent.id.clone(),
        acquisition_allowed: blocked_reason.is_none(),
        blocked_reason,
        options,
    })
}

#[allow(dead_code)]
pub(crate) fn get_academy_creation_options_for_game(
    game: &Game,
    parent_team_id: &str,
) -> Result<AcademyCreationOptionsResponse, String> {
    get_academy_acquisition_options_for_game(game, parent_team_id)
}

pub(crate) fn acquire_academy_team_in_game(
    game: &mut Game,
    request: AcquireAcademyTeamRequest,
) -> Result<Game, String> {
    let option = get_academy_acquisition_options_for_game(game, &request.parent_team_id)?
        .options
        .into_iter()
        .find(|option| option.source_team_id == request.source_team_id)
        .ok_or_else(|| {
            format!(
                "Academy candidate '{}' is not eligible for this team",
                request.source_team_id
            )
        })?;

    let parent_snapshot = olm_core::academy::find_team(game, &request.parent_team_id)?.clone();
    validate_academy_acquisition(&parent_snapshot, &option).map_err(format_academy_error)?;

    let academy_id = option.source_team_id.clone();

    let created_at = game.clock.current_date.with_timezone(&Utc).to_rfc3339();
    let metadata = olm_core::academy::academy_metadata(
        &option,
        created_at.clone(),
        request.custom_logo_url.clone(),
    );

    let existing_academy_index = game
        .teams
        .iter()
        .position(|team| team.id == academy_id && team.team_kind == TeamKind::Academy);
    if let Some(academy_index) = existing_academy_index {
        if game
            .teams
            .get(academy_index)
            .and_then(|academy| academy.parent_team_id.as_ref())
            .is_some()
        {
            return Err(format!("Academy team id '{}' already linked", academy_id));
        }
    }

    let parent_index = game
        .teams
        .iter()
        .position(|team| team.id == request.parent_team_id)
        .ok_or("Parent team not found".to_string())?;

    {
        let parent = game
            .teams
            .get_mut(parent_index)
            .ok_or("Parent team not found".to_string())?;
        record_transaction(
            parent,
            FinanceTransactionInput {
                date: game.clock.current_date.format("%Y-%m-%d").to_string(),
                description: format!("Academy acquisition: {}", option.name),
                amount: -option.acquisition_cost,
                kind: olm_core::domain::team::FinancialTransactionKind::AcademyAcquisition,
                budget_impact: BudgetImpact::None,
                affects_season_totals: true,
                source: "academy".to_string(),
                source_id: Some(academy_id.clone()),
                correlation_id: Some(format!(
                    "academy-acquisition:{}:{}",
                    request.parent_team_id, academy_id
                )),
            },
        )
        .map_err(|err| format!("Failed to record academy acquisition: {err:?}"))?;
        parent.academy_team_id = Some(academy_id.clone());
    }

    if let Some(academy_index) = existing_academy_index {
        let academy = game
            .teams
            .get_mut(academy_index)
            .ok_or("Academy team not found".to_string())?;
        academy.name = request.custom_name.unwrap_or_else(|| option.name.clone());
        academy.short_name = request
            .custom_short_name
            .unwrap_or_else(|| option.short_name.clone());
        academy.parent_team_id = Some(parent_snapshot.id.clone());
        academy.academy = Some(metadata);
        academy.reputation = u32::from(option.reputation) * 100;
        academy.finance = 0;
        academy.wage_budget = 0;
        academy.transfer_budget = 0;
    } else {
        let mut academy = Team::new(
            academy_id.clone(),
            request.custom_name.unwrap_or_else(|| option.name.clone()),
            request
                .custom_short_name
                .unwrap_or_else(|| option.short_name.clone()),
            option.country_code.clone(),
            parent_snapshot.city.clone(),
            format!("{} Performance Centre", option.short_name),
            2_500,
        );
        academy.team_kind = TeamKind::Academy;
        academy.parent_team_id = Some(parent_snapshot.id.clone());
        academy.academy = Some(metadata);
        academy.finance = 0;
        academy.wage_budget = 0;
        academy.transfer_budget = 0;
        academy.reputation = u32::from(option.reputation) * 100;
        game.teams.push(academy);
    }

    olm_core::academy::push_academy_acquired_message(
        game,
        &parent_snapshot,
        &option.name,
        option.acquisition_cost,
    );
    Ok(game.clone())
}

#[allow(dead_code)]
pub(crate) fn create_academy_in_game(
    _game: &mut Game,
    _parent_team_id: &str,
    erl_league_id: &str,
) -> Result<Game, String> {
    Err(format!(
        "create_academy_in_game is deprecated; use acquire_academy_team_in_game with a source team candidate instead of ERL '{}'.",
        erl_league_id
    ))
}

fn format_academy_error(error: olm_core::academy::AcademyError) -> String {
    match error {
        olm_core::academy::AcademyError::ParentMustBeMainTeam { team_id } => {
            format!("Team '{}' is not a main team", team_id)
        }
        olm_core::academy::AcademyError::AcademyAlreadyExists {
            parent_team_id,
            academy_team_id,
        } => format!(
            "Parent team '{}' already has academy '{}'",
            parent_team_id, academy_team_id
        ),
        olm_core::academy::AcademyError::InsufficientFunds {
            available,
            required,
        } => format!(
            "Insufficient funds for academy acquisition: available {}, required {}",
            available, required
        ),
        olm_core::academy::AcademyError::UnrelatedAcademy {
            parent_team_id,
            academy_team_id,
        } => format!(
            "Academy '{}' is not linked to parent team '{}'",
            academy_team_id, parent_team_id
        ),
    }
}

// academy_erl_catalog, catalogs_from_tier2_manifests, and
// academy_candidate_catalog moved to olm_core::academy.

#[cfg(test)]
mod tests {
    use super::{
        acquire_academy_team_in_game, get_academy_acquisition_options_for_game,
        AcquireAcademyTeamRequest,
    };
    use chrono::{TimeZone, Utc};
    use olm_core::clock::GameClock;
    use olm_core::domain::manager::Manager;
    use olm_core::domain::team::{
        AcademyLifecycle, AcademyMetadata, ErlAssignment, ErlAssignmentRule,
        FinancialTransactionKind, Team, TeamKind,
    };
    use olm_core::game::Game;

    fn source_id_by_name(game: &Game, parent_team_id: &str, team_name: &str) -> String {
        get_academy_acquisition_options_for_game(game, parent_team_id)
            .expect("options")
            .options
            .into_iter()
            .find(|option| option.name == team_name)
            .map(|option| option.source_team_id)
            .expect("expected academy option by name")
    }

    fn team(id: &str, country: &str, finance: i64) -> Team {
        let mut team = Team::new(
            id.to_string(),
            format!("{} Esports", id),
            id.chars().take(3).collect::<String>().to_uppercase(),
            country.to_string(),
            "Berlin".to_string(),
            "Arena".to_string(),
            12_000,
        );
        team.finance = finance;
        team
    }

    fn game_with_team(mut parent: Team) -> Game {
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1985-01-01".to_string(),
            "ES".to_string(),
        );
        manager.hire(parent.id.clone());
        parent.manager_id = Some(manager.id.clone());

        Game::new(
            GameClock::new(Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()),
            manager,
            vec![parent],
            vec![],
            vec![],
            vec![],
        )
    }

    #[test]
    fn acquisition_options_list_existing_domestic_candidate_teams() {
        let game = game_with_team(team("koi", "ES", 1_000_000));

        let response = get_academy_acquisition_options_for_game(&game, "koi").expect("options");

        assert!(response.acquisition_allowed);
        assert_eq!(response.parent_team_id, "koi");
        assert!(response.options.len() >= 5);
        let koi_fenix = response
            .options
            .iter()
            .find(|option| option.name == "Movistar KOI Fénix")
            .expect("koi academy candidate in open pool");
        assert_eq!(koi_fenix.name, "Movistar KOI Fénix");
        assert_eq!(koi_fenix.erl_league_id, "les");
        assert_eq!(koi_fenix.assignment_rule, ErlAssignmentRule::Domestic);
        assert_eq!(koi_fenix.fallback_reason, None);
        assert!(koi_fenix.acquisition_cost > 0);
    }

    #[test]
    fn acquisition_options_include_karmine_corp_blue_for_france() {
        let game = game_with_team(team("karmine", "FR", 1_000_000));

        let response = get_academy_acquisition_options_for_game(&game, "karmine").expect("options");

        assert!(response.acquisition_allowed);
        assert!(response
            .options
            .iter()
            .any(|option| option.name == "Karmine Corp Blue"));
    }

    #[test]
    fn acquisition_options_include_cross_country_candidates_in_open_pool() {
        let game = game_with_team(team("swiss-team", "CH", 1_000_000));

        let response =
            get_academy_acquisition_options_for_game(&game, "swiss-team").expect("options");

        assert!(response.acquisition_allowed);
        assert!(response.options.len() >= 5);
        let kcb = response
            .options
            .iter()
            .find(|option| option.name == "Karmine Corp Blue")
            .expect("kcb option available cross-country");
        assert_eq!(kcb.assignment_rule, ErlAssignmentRule::Fallback);
        assert_eq!(kcb.fallback_reason, None);
    }

    #[test]
    fn acquisition_options_exclude_taken_source_teams() {
        let mut game = game_with_team(team("mad", "ES", 1_000_000));
        let mut taken_academy = team("academy-any", "ES", 0);
        taken_academy.team_kind = TeamKind::Academy;
        taken_academy.parent_team_id = Some("other-parent".to_string());
        taken_academy.academy = Some(AcademyMetadata {
            lifecycle: AcademyLifecycle::Active,
            erl_assignment: ErlAssignment {
                erl_league_id: "les".to_string(),
                country_rule: ErlAssignmentRule::Domestic,
                fallback_reason: None,
                reputation: 4,
                acquisition_cost: 300_000,
                acquired_at: "2026-01-01T12:00:00+00:00".to_string(),
                creation_cost: 0,
                created_at: String::new(),
            },
            source_team_id: "academy-les-team-heretics".to_string(),
            original_name: "Team Heretics".to_string(),
            original_short_name: "TH".to_string(),
            original_logo_url: None,
            current_logo_url: None,
            acquisition_cost: 300_000,
            acquired_at: "2026-01-01T12:00:00+00:00".to_string(),
        });
        game.teams.push(taken_academy);

        let response = get_academy_acquisition_options_for_game(&game, "mad").expect("options");

        assert!(!response
            .options
            .iter()
            .any(|option| option.name == "Team Heretics"));
    }

    #[test]
    fn options_generation_marks_acquisition_blocked_when_team_cannot_afford_any_option() {
        let game = game_with_team(team("broke-team", "ES", 1));

        let response =
            get_academy_acquisition_options_for_game(&game, "broke-team").expect("options");

        assert!(!response.acquisition_allowed);
        assert_eq!(
            response.blocked_reason.as_deref(),
            Some("Insufficient funds for all eligible academy acquisition options")
        );
        assert_eq!(response.options[0].erl_league_id, "les");
        assert!(response.options[0].acquisition_cost > 1);
    }

    #[test]
    fn acquire_academy_team_rejects_invalid_candidate_without_mutation() {
        let mut game = game_with_team(team("mad", "ES", 1_000_000));
        let before = game.clone();

        let result = acquire_academy_team_in_game(
            &mut game,
            AcquireAcademyTeamRequest {
                parent_team_id: "mad".to_string(),
                source_team_id: "not-a-real-candidate".to_string(),
                custom_name: None,
                custom_short_name: None,
                custom_logo_url: None,
            },
        );

        assert!(result
            .expect_err("invalid source candidate should be rejected")
            .contains("is not eligible for this team"));
        assert_eq!(game.teams.len(), before.teams.len());
        assert_eq!(game.teams[0].finance, before.teams[0].finance);
        assert_eq!(game.teams[0].academy_team_id, None);
    }

    #[test]
    fn acquire_academy_team_rejects_parent_that_already_has_academy_without_mutation() {
        let mut parent = team("g2", "DE", 1_000_000);
        parent.academy_team_id = Some("g2-academy-existing".to_string());
        let mut game = game_with_team(parent);
        let before_team_count = game.teams.len();
        let before_parent_finance = game
            .teams
            .iter()
            .find(|team| team.id == "g2")
            .unwrap()
            .finance;
        let spandau_source_id = source_id_by_name(&game, "g2", "Eintracht Spandau");

        let result = acquire_academy_team_in_game(
            &mut game,
            AcquireAcademyTeamRequest {
                parent_team_id: "g2".to_string(),
                source_team_id: spandau_source_id,
                custom_name: None,
                custom_short_name: None,
                custom_logo_url: None,
            },
        );

        assert!(result
            .expect_err("existing academy should be rejected")
            .contains("already has academy"));
        assert_eq!(game.teams.len(), before_team_count);
        assert_eq!(
            game.teams
                .iter()
                .find(|team| team.id == "g2")
                .unwrap()
                .finance,
            before_parent_finance
        );
    }

    #[test]
    fn acquire_academy_team_rejects_insufficient_funds_without_mutation() {
        let mut game = game_with_team(team("broke-mad", "ES", 1));
        let before_team_count = game.teams.len();
        let koi_source_id = source_id_by_name(&game, "broke-mad", "Movistar KOI Fénix");

        let result = acquire_academy_team_in_game(
            &mut game,
            AcquireAcademyTeamRequest {
                parent_team_id: "broke-mad".to_string(),
                source_team_id: koi_source_id,
                custom_name: None,
                custom_short_name: None,
                custom_logo_url: None,
            },
        );

        assert!(result
            .expect_err("insufficient funds should be rejected")
            .contains("Insufficient funds"));
        let parent = game
            .teams
            .iter()
            .find(|team| team.id == "broke-mad")
            .unwrap();
        assert_eq!(game.teams.len(), before_team_count);
        assert_eq!(parent.finance, 1);
        assert_eq!(parent.academy_team_id, None);
    }

    #[test]
    fn acquire_academy_team_links_existing_candidate_with_source_metadata_and_expense() {
        let mut game = game_with_team(team("mad", "ES", 1_000_000));
        let koi_source_id = source_id_by_name(&game, "mad", "Movistar KOI Fénix");

        let updated = acquire_academy_team_in_game(
            &mut game,
            AcquireAcademyTeamRequest {
                parent_team_id: "mad".to_string(),
                source_team_id: koi_source_id.clone(),
                custom_name: Some("MAD Academy".to_string()),
                custom_short_name: Some("MADA".to_string()),
                custom_logo_url: Some("logos/mad-academy.svg".to_string()),
            },
        )
        .expect("acquired");

        let parent = updated.teams.iter().find(|team| team.id == "mad").unwrap();
        let academy_id = parent.academy_team_id.as_deref().expect("academy link");
        let academy = updated
            .teams
            .iter()
            .find(|team| team.id == academy_id)
            .expect("academy team exists");

        let metadata = academy.academy.as_ref().unwrap();
        assert_eq!(academy.team_kind, TeamKind::Academy);
        assert_eq!(academy.id, koi_source_id);
        assert_eq!(academy.name, "MAD Academy");
        assert_eq!(academy.short_name, "MADA");
        assert_eq!(academy.parent_team_id.as_deref(), Some("mad"));
        assert_eq!(metadata.source_team_id, academy.id);
        assert_eq!(metadata.original_name, "Movistar KOI Fénix");
        assert!(metadata.original_logo_url.is_some());
        assert_eq!(
            metadata.current_logo_url.as_deref(),
            Some("logos/mad-academy.svg")
        );
        assert_eq!(metadata.acquisition_cost, 260_000);
        assert_eq!(metadata.acquired_at, "2026-01-01T12:00:00+00:00");
        assert_eq!(metadata.erl_assignment.erl_league_id, "les");
        assert_eq!(
            metadata.erl_assignment.country_rule,
            ErlAssignmentRule::Domestic
        );
        assert_eq!(parent.finance, 740_000);
        assert_eq!(parent.season_expenses, 260_000);
        assert_eq!(parent.financial_ledger.len(), 1);
        assert_eq!(
            parent.financial_ledger[0].kind,
            FinancialTransactionKind::AcademyAcquisition
        );
        assert_eq!(parent.financial_ledger[0].amount, -260_000);
        assert!(updated
            .messages
            .iter()
            .any(|message| message.id.starts_with("academy-acquired-")));
        assert_eq!(game.teams.len(), updated.teams.len());
        assert_eq!(
            game.teams
                .iter()
                .find(|team| team.id == "mad")
                .and_then(|team| team.academy_team_id.as_deref()),
            parent.academy_team_id.as_deref()
        );
    }
}
