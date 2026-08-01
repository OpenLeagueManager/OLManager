use log::info;
use tauri::State;

use crate::application::time_advancement::advance_time_with_mode as advance_time_with_mode_service;
pub use crate::application::time_advancement::AdvanceTimeWithModeResponse;
use olm_core::game::Game;
use olm_core::state::StateManager;

fn advance_time_internal(state: &StateManager) -> Result<Game, String> {
    let mut current_game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    info!(
        "[cmd] advance_time: date={}",
        current_game.clock.current_date.format("%Y-%m-%d")
    );

    let mut captures = Vec::new();
    olm_core::turn::process_day_with_capture(&mut current_game, &mut |capture| {
        captures.push(capture);
    });

    for capture in captures {
        state.append_stats_state(capture);
    }

    state.set_game(current_game.clone());
    Ok(current_game)
}

fn advance_time_with_mode_internal(
    state: &StateManager,
    mode: &str,
) -> Result<AdvanceTimeWithModeResponse, String> {
    advance_time_with_mode_service(state, mode)
}

/// Advance time with a specific match mode.
/// mode: "live" | "spectator" | "delegate" | "instant"
/// If mode is "live" or "spectator" and there's a user match today,
/// it sets up the live match session instead of auto-simulating.
#[tauri::command]
pub fn advance_time_with_mode(
    state: State<'_, StateManager>,
    mode: String,
) -> Result<AdvanceTimeWithModeResponse, String> {
    advance_time_with_mode_internal(&state, &mode)
}

#[tauri::command]
pub fn advance_time(state: State<'_, StateManager>) -> Result<Game, String> {
    advance_time_internal(&state)
}

pub fn compute_blocking_actions(game: &Game) -> Vec<serde_json::Value> {
    olm_core::time_blockers::compute_blocking_actions(game)
}

#[tauri::command]
pub fn check_blocking_actions(state: State<'_, StateManager>) -> Result<serde_json::Value, String> {
    log::debug!("[cmd] check_blocking_actions");
    let game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session")?;

    let blockers = compute_blocking_actions(&game);
    info!(
        "[cmd] check_blocking_actions: date={}, blocker_count={}",
        game.clock.current_date.format("%Y-%m-%d"),
        blockers.len()
    );
    Ok(serde_json::json!(blockers))
}

#[tauri::command]
pub fn skip_to_match_day(state: State<'_, StateManager>) -> Result<serde_json::Value, String> {
    info!("[cmd] skip_to_match_day");
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session")?;

    // Precondition: manager must be employed at entry — guarantees that any later
    // `team_id.is_none()` inside the loop is a real firing transition, not a stale state.
    let user_team_id = game.manager.team_id.clone().ok_or("No team assigned")?;
    info!(
        "[cmd] skip_to_match_day: start_date={}, user_team_id={}",
        game.clock.current_date.format("%Y-%m-%d"),
        user_team_id
    );

    let mut days_skipped = 0u32;
    loop {
        if days_skipped >= 60 {
            break;
        }

        let today = game.clock.current_date.format("%Y-%m-%d").to_string();

        let has_match = game.active_league().is_some_and(|league| {
            league.fixtures.iter().any(|fixture| {
                fixture.date == today
                    && fixture.status == olm_core::domain::league::FixtureStatus::Scheduled
                    && (fixture.home_team_id == user_team_id
                        || fixture.away_team_id == user_team_id)
            })
        });

        if has_match {
            info!(
                "[cmd] skip_to_match_day: found match_day={}, days_skipped={}",
                today, days_skipped
            );
            break;
        }

        let mut captures = Vec::new();
        olm_core::turn::process_day_with_capture(&mut game, &mut |capture| {
            captures.push(capture);
        });
        for capture in captures {
            state.append_stats_state(capture);
        }
        days_skipped += 1;

        // Detect a firing that happened *during* this skip. Because the function
        // errors out above when the manager starts unemployed, seeing `team_id.is_none()`
        // here can only mean a real employed → unemployed transition.
        if game.manager.team_id.is_none() {
            info!(
                "[cmd] skip_to_match_day: manager fired after {} days",
                days_skipped
            );
            state.set_game(game.clone());
            return Ok(serde_json::json!({
                "action": "fired",
                "game": game,
                "days_skipped": days_skipped
            }));
        }

        // After processing, check if blocking actions arose
        let blockers = compute_blocking_actions(&game);
        if !blockers.is_empty() {
            info!(
                "[cmd] skip_to_match_day: blocked_after_days={}, date={}, blocker_count={}",
                days_skipped,
                game.clock.current_date.format("%Y-%m-%d"),
                blockers.len()
            );
            state.set_game(game.clone());
            return Ok(serde_json::json!({
                "action": "blocked",
                "game": game,
                "blockers": blockers,
                "days_skipped": days_skipped
            }));
        }
    }

    info!(
        "[cmd] skip_to_match_day: arrived_after_days={}, final_date={}",
        days_skipped,
        game.clock.current_date.format("%Y-%m-%d")
    );
    state.set_game(game.clone());
    Ok(serde_json::json!({
        "action": "arrived",
        "game": game,
        "days_skipped": days_skipped
    }))
}

#[cfg(test)]
mod tests {
    use super::{advance_time_with_mode_internal, compute_blocking_actions};
    use chrono::{TimeZone, Utc};
    use olm_core::clock::GameClock;
    use olm_core::domain::league::{Fixture, FixtureStatus, LeagueKind, MatchType};
    use olm_core::domain::manager::Manager;
    use olm_core::domain::message::{InboxMessage, MessagePriority};
    use olm_core::domain::player::{LolRole, Player, PlayerAttributes};
    use olm_core::domain::stats::StatsState;
    use olm_core::domain::team::Team;
    use olm_core::game::{DayPhase, Game};
    use olm_core::state::StateManager;
    use serde_json::Value;

    fn default_attrs() -> PlayerAttributes {
        PlayerAttributes {
            mechanics: 60,
            laning: 60,
            teamfighting: 60,
            macro_play: 60,
            consistency: 60,
            shotcalling: 60,
            champion_pool: 60,
            discipline: 60,
            mental_resilience: 60,
        }
    }

    fn make_player(id: &str, name: &str, team_id: &str, position: LolRole) -> Player {
        let mut player = Player::new(
            id.to_string(),
            name.to_string(),
            name.to_string(),
            "2000-01-01".to_string(),
            "England".to_string(),
            position,
            default_attrs(),
        );
        player.team_id = Some(team_id.to_string());
        player
    }

    fn make_game(roster_size: usize) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr1".to_string(),
            "Alex".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team1".to_string());

        let players: Vec<Player> = (1..=roster_size)
            .map(|idx| {
                let position = if idx == 1 {
                    LolRole::Support
                } else if idx <= 5 {
                    LolRole::Top
                } else if idx == 6 {
                    LolRole::Mid
                } else if idx <= 9 {
                    LolRole::Jungle
                } else {
                    LolRole::Adc
                };

                make_player(
                    &format!("p{}", idx),
                    &format!("Player {}", idx),
                    "team1",
                    position,
                )
            })
            .collect();

        let mut team = Team::new(
            "team1".to_string(),
            "Test FC".to_string(),
            "TST".to_string(),
            "England".to_string(),
            "Testville".to_string(),
            "Test Ground".to_string(),
            20_000,
        );
        team.active_lineup_ids = players
            .iter()
            .take(11)
            .map(|player| player.id.clone())
            .collect();

        Game::new(clock, manager, vec![team], players, vec![], vec![])
    }

    fn make_game_with_matchday() -> Game {
        let mut game = make_game(22);
        let today = game.clock.current_date.format("%Y-%m-%d").to_string();
        let mut opponent_team = Team::new(
            "team2".to_string(),
            "Rival FC".to_string(),
            "RIV".to_string(),
            "England".to_string(),
            "Rivaltown".to_string(),
            "Rival Ground".to_string(),
            21_000,
        );
        opponent_team.active_lineup_ids = game
            .players
            .iter()
            .skip(11)
            .take(11)
            .map(|p| p.id.clone())
            .collect();
        game.teams.push(opponent_team);

        for player in game.players.iter_mut().skip(11) {
            player.team_id = Some("team2".to_string());
        }

        game.teams[0].active_lineup_ids =
            game.players.iter().take(11).map(|p| p.id.clone()).collect();
        game.leagues = vec![olm_core::domain::league::League {
            id: "league-1".to_string(),
            name: "League".to_string(),
            season: 2025,
            competition_id: None,
            logo: None,
            league_kind: LeagueKind::Main,
            split_index: 0,
            tier: 0,
            active: false,
            fixtures: vec![Fixture {
                id: "fixture-1".to_string(),
                matchday: 1,
                date: today,
                home_team_id: "team1".to_string(),
                away_team_id: "team2".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Scheduled,
                result: None,
            }],
            standings: vec![
                olm_core::domain::league::StandingEntry::new("team1".to_string()),
                olm_core::domain::league::StandingEntry::new("team2".to_string()),
            ],
        }];
        game
    }

    fn make_message(id: &str, priority: MessagePriority, read: bool) -> InboxMessage {
        let mut message = InboxMessage::new(
            id.to_string(),
            "Subject".to_string(),
            "Body".to_string(),
            "Board".to_string(),
            "2025-06-15".to_string(),
        )
        .with_priority(priority);
        message.read = read;
        message
    }

    fn blocker_by_id<'a>(blockers: &'a [Value], id: &str) -> Option<&'a Value> {
        blockers
            .iter()
            .find(|blocker| blocker.get("id").and_then(Value::as_str) == Some(id))
    }

    #[test]
    fn advance_time_records_match_history_in_active_stats_state() {
        let state = StateManager::new();
        state.set_game(make_game_with_matchday());
        state.set_stats_state(StatsState::default());

        let advanced = super::advance_time_internal(&state).unwrap();
        let stats = state.get_stats_state(|current| current.clone()).unwrap();

        assert_eq!(
            advanced.clock.current_date.date_naive(),
            Utc.with_ymd_and_hms(2025, 6, 16, 12, 0, 0)
                .unwrap()
                .date_naive()
        );
        assert!(
            !stats.player_matches.is_empty(),
            "expected player match history to be recorded"
        );
        assert_eq!(stats.team_matches.len(), 2);
        assert_eq!(stats.player_matches[0].fixture_id, "fixture-1");
    }

    #[test]
    fn healthy_squad_with_no_urgent_messages_has_no_blockers() {
        let game = make_game(11);

        let blockers = compute_blocking_actions(&game);

        assert!(blockers.is_empty());
    }

    #[test]
    fn missing_lol_roles_trigger_main_role_coverage_blocker() {
        let mut game = make_game(5);
        game.players.truncate(5);
        game.teams[0].active_lineup_ids = game
            .players
            .iter()
            .map(|player| player.id.clone())
            .collect();

        let blockers = compute_blocking_actions(&game);

        let role_blocker =
            blocker_by_id(&blockers, "main_missing_roles").expect("missing role blocker");
        assert_eq!(
            role_blocker.get("severity").and_then(Value::as_str),
            Some("warn")
        );
        assert_eq!(
            role_blocker.get("tab").and_then(Value::as_str),
            Some("Squad")
        );
        let text = role_blocker.get("text").and_then(Value::as_str).unwrap();
        assert!(text.contains("JUNGLE"));
        assert!(text.contains("MID"));
        assert!(text.contains("ADC"));
    }

    #[test]
    fn incomplete_lineup_is_not_reported_when_a_partial_saved_lineup_can_be_filled_by_healthy_players(
    ) {
        let mut game = make_game(11);
        game.teams[0].active_lineup_ids = vec![
            "p1".to_string(),
            "p2".to_string(),
            "p3".to_string(),
            "p4".to_string(),
            "p5".to_string(),
            "p6".to_string(),
            "p7".to_string(),
            "p8".to_string(),
        ];

        let blockers = compute_blocking_actions(&game);

        assert!(blocker_by_id(&blockers, "incomplete_lineup").is_none());
    }

    #[test]
    fn only_unread_urgent_messages_produce_message_blockers() {
        let mut game = make_game(11);
        game.messages = vec![
            make_message("urgent-1", MessagePriority::Urgent, false),
            make_message("urgent-2", MessagePriority::Urgent, false),
            make_message("urgent-read", MessagePriority::Urgent, true),
            make_message("high", MessagePriority::High, false),
        ];

        let blockers = compute_blocking_actions(&game);

        assert_eq!(blockers.len(), 1);
        let urgent = blocker_by_id(&blockers, "urgent_messages").unwrap();
        assert_eq!(urgent.get("severity").and_then(Value::as_str), Some("info"));
        assert_eq!(urgent.get("tab").and_then(Value::as_str), Some("Inbox"));
        assert_eq!(
            urgent.get("text").and_then(Value::as_str),
            Some("2 urgent unread message(s)")
        );
    }

    #[test]
    fn key_player_contract_risk_triggers_squad_blocker() {
        let mut game = make_game(11);

        let first_key_player = game
            .players
            .iter_mut()
            .find(|player| player.id == "p10")
            .unwrap();
        first_key_player.contract_end = Some("2025-07-15".to_string());
        first_key_player.wage = 35_000;
        first_key_player.attributes.laning = 94;
        first_key_player.attributes.mechanics = 90;

        let second_key_player = game
            .players
            .iter_mut()
            .find(|player| player.id == "p11")
            .unwrap();
        second_key_player.contract_end = Some("2025-07-15".to_string());
        second_key_player.wage = 25_000;
        second_key_player.attributes.laning = 91;
        second_key_player.attributes.mechanics = 89;

        let blockers = compute_blocking_actions(&game);

        let contract_blocker = blocker_by_id(&blockers, "key_contract_risk").unwrap();
        assert_eq!(
            contract_blocker.get("severity").and_then(Value::as_str),
            Some("warn")
        );
        assert_eq!(
            contract_blocker.get("tab").and_then(Value::as_str),
            Some("Squad")
        );

        let text = contract_blocker
            .get("text")
            .and_then(Value::as_str)
            .unwrap();
        assert!(text.contains("Player 10"));
        assert!(text.contains("Player 11"));
    }

    #[test]
    fn large_at_risk_wage_share_triggers_finance_blocker() {
        let mut game = make_game(11);
        game.teams[0].wage_budget = 50_000;

        let first_risk = game
            .players
            .iter_mut()
            .find(|player| player.id == "p10")
            .unwrap();
        first_risk.contract_end = Some("2025-07-15".to_string());
        first_risk.wage = 35_000;

        let second_risk = game
            .players
            .iter_mut()
            .find(|player| player.id == "p11")
            .unwrap();
        second_risk.contract_end = Some("2025-07-15".to_string());
        second_risk.wage = 25_000;

        let blockers = compute_blocking_actions(&game);

        let finance_blocker = blocker_by_id(&blockers, "contract_wage_risk").unwrap();
        assert_eq!(
            finance_blocker.get("severity").and_then(Value::as_str),
            Some("warn")
        );
        assert_eq!(
            finance_blocker.get("tab").and_then(Value::as_str),
            Some("Finances")
        );

        let text = finance_blocker.get("text").and_then(Value::as_str).unwrap();
        assert!(text.contains("60000"));
        assert!(text.contains("wage budget"));
    }

    fn make_round_summary_game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr1".to_string(),
            "Alex".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team1".to_string());

        let teams = vec![
            Team::new(
                "team1".to_string(),
                "Test FC".to_string(),
                "TST".to_string(),
                "England".to_string(),
                "Testville".to_string(),
                "Test Ground".to_string(),
                20_000,
            ),
            Team::new(
                "team2".to_string(),
                "Rival FC".to_string(),
                "RIV".to_string(),
                "England".to_string(),
                "Rivaltown".to_string(),
                "Rival Ground".to_string(),
                20_000,
            ),
            Team::new(
                "team3".to_string(),
                "Third FC".to_string(),
                "THI".to_string(),
                "England".to_string(),
                "Thirdtown".to_string(),
                "Third Ground".to_string(),
                20_000,
            ),
            Team::new(
                "team4".to_string(),
                "Fourth FC".to_string(),
                "FOU".to_string(),
                "England".to_string(),
                "Fourthtown".to_string(),
                "Fourth Ground".to_string(),
                20_000,
            ),
        ];

        let mut players = Vec::new();
        for (team_id, prefix) in [
            ("team1", "a"),
            ("team2", "b"),
            ("team3", "c"),
            ("team4", "d"),
        ] {
            players.push(make_player(
                &format!("{}-gk", prefix),
                &format!("{} GK", prefix),
                team_id,
                LolRole::Support,
            ));
            for idx in 0..4 {
                players.push(make_player(
                    &format!("{}-def{}", prefix, idx),
                    &format!("{} Def{}", prefix, idx),
                    team_id,
                    LolRole::Top,
                ));
            }
            for idx in 0..4 {
                players.push(make_player(
                    &format!("{}-mid{}", prefix, idx),
                    &format!("{} Mid{}", prefix, idx),
                    team_id,
                    LolRole::Jungle,
                ));
            }
            for idx in 0..2 {
                players.push(make_player(
                    &format!("{}-fwd{}", prefix, idx),
                    &format!("{} Fwd{}", prefix, idx),
                    team_id,
                    LolRole::Adc,
                ));
            }
        }

        let league = olm_core::domain::league::League {
            id: "league1".to_string(),
            name: "Test League".to_string(),
            season: 1,
            competition_id: None,
            logo: None,
            league_kind: LeagueKind::Main,
            split_index: 0,
            tier: 0,
            active: false,
            fixtures: vec![
                olm_core::domain::league::Fixture {
                    id: "fix1".to_string(),
                    matchday: 1,
                    date: "2025-06-15".to_string(),
                    home_team_id: "team1".to_string(),
                    away_team_id: "team2".to_string(),
                    match_type: MatchType::League,
                    best_of: 1,
                    status: FixtureStatus::Scheduled,
                    result: None,
                },
                Fixture {
                    id: "fix2".to_string(),
                    matchday: 1,
                    date: "2025-06-15".to_string(),
                    home_team_id: "team3".to_string(),
                    away_team_id: "team4".to_string(),
                    match_type: MatchType::League,
                    best_of: 1,
                    status: olm_core::domain::league::FixtureStatus::Scheduled,
                    result: None,
                },
            ],
            standings: vec![
                olm_core::domain::league::StandingEntry::new("team1".to_string()),
                olm_core::domain::league::StandingEntry::new("team2".to_string()),
                olm_core::domain::league::StandingEntry::new("team3".to_string()),
                olm_core::domain::league::StandingEntry::new("team4".to_string()),
            ],
        };

        let mut game = Game::new(clock, manager, teams, players, vec![], vec![]);
        game.leagues = vec![league];
        game
    }

    #[test]
    fn advance_time_with_mode_live_returns_partial_round_summary() {
        let state = StateManager::new();
        state.set_game(make_round_summary_game());

        let response =
            advance_time_with_mode_internal(&state, "live").expect("live advance response");

        assert_eq!(response.action, "live_match");
        let round_summary = response.round_summary.expect("round summary");
        assert!(!round_summary.is_complete);
        assert_eq!(round_summary.pending_fixture_count, 1);
        assert_eq!(round_summary.completed_results.len(), 1);
    }

    #[test]
    fn advance_time_with_mode_delegate_returns_completed_round_summary() {
        let state = StateManager::new();
        state.set_game(make_round_summary_game());

        let response =
            advance_time_with_mode_internal(&state, "delegate").expect("delegate advance response");

        assert_eq!(response.action, "advanced");
        let round_summary = response.round_summary.expect("round summary");
        assert!(round_summary.is_complete);
        assert_eq!(round_summary.pending_fixture_count, 0);
        assert_eq!(round_summary.completed_results.len(), 2);
    }

    #[test]
    fn advance_time_with_mode_advances_phase_before_processing_non_match_day() {
        let state = StateManager::new();
        let game = make_game(11);
        let start_date = game.clock.current_date;
        state.set_game(game);

        let response =
            advance_time_with_mode_internal(&state, "delegate").expect("phase advance response");

        assert_eq!(response.action, "phase_advanced");
        let game = response.game.expect("game response");
        assert_eq!(game.day_phase, DayPhase::ScrimBlock);
        assert_eq!(game.clock.current_date, start_date);
    }

    #[test]
    fn advance_time_with_mode_resolves_scrims_when_entering_scrim_block() {
        let state = StateManager::new();
        let mut game = make_game(22);
        game.clock.current_date = Utc.with_ymd_and_hms(2025, 6, 17, 12, 0, 0).unwrap();
        game.teams[0].scrim_weekly_slots = 2;
        game.teams[0].weekly_scrim_plan_team_ids = vec![vec!["team2".to_string()]];

        let mut opponent_team = Team::new(
            "team2".to_string(),
            "Rival FC".to_string(),
            "RIV".to_string(),
            "England".to_string(),
            "Rivaltown".to_string(),
            "Rival Ground".to_string(),
            21_000,
        );
        opponent_team.active_lineup_ids = game
            .players
            .iter()
            .skip(11)
            .take(11)
            .map(|player| player.id.clone())
            .collect();
        for player in game.players.iter_mut().skip(11) {
            player.team_id = Some("team2".to_string());
        }
        game.teams.push(opponent_team);
        state.set_game(game);

        let response = advance_time_with_mode_internal(&state, "delegate")
            .expect("scrim block phase response");

        assert_eq!(response.action, "phase_advanced");
        let game = response.game.expect("game response");
        assert_eq!(game.day_phase, DayPhase::ScrimBlock);
        assert_eq!(game.teams[0].scrim_reports.len(), 1);
        assert_eq!(game.teams[0].scrim_reports[0].opponent_team_id, "team2");
    }

    #[test]
    fn advance_time_with_mode_processes_day_from_evening_phase() {
        let state = StateManager::new();
        let mut game = make_game(11);
        let start_date = game.clock.current_date;
        game.day_phase = DayPhase::Evening;
        state.set_game(game);

        let response =
            advance_time_with_mode_internal(&state, "delegate").expect("day advance response");

        assert_eq!(response.action, "advanced");
        let game = response.game.expect("game response");
        assert_eq!(game.day_phase, DayPhase::Morning);
        assert_eq!(
            game.clock.current_date,
            start_date + chrono::Duration::days(1)
        );
    }
}
