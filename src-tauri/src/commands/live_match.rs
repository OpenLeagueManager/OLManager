use log::info;
use rand::RngExt;
use tauri::State;

pub use crate::application::live_match::FinishLiveMatchResponse;
use crate::application::live_match::{
    apply_match_command as apply_match_command_service,
    finish_live_match as finish_live_match_service,
    get_match_snapshot as get_match_snapshot_service, start_live_match as start_live_match_service,
    step_live_match as step_live_match_service, LolSimMatchReportInput,
};
use olm_core::domain::stats::MatchOutcome;
use olm_core::draft::{
    evaluate_draft_state as evaluate_draft_state_core, DraftStateEvaluation, DraftStateInput,
};
use olm_core::game::Game;
use olm_core::state::StateManager;
use olm_core::team_talk::apply_team_talk as apply_team_talk_core;

fn apply_delta(value: u8, delta: i16) -> u8 {
    ((value as i16) + delta).clamp(10, 100) as u8
}

fn press_effect_delta(effect_id: &str) -> Option<(&'static str, i16)> {
    match effect_id {
        "press_squad_morale_small_up" => Some(("squad", 3)),
        "press_player_pressure_small_down" => Some(("player", -2)),
        "press_no_effect" => Some(("none", 0)),
        _ => None,
    }
}

fn apply_press_conference_effects(
    game: &mut Game,
    answers: &[serde_json::Value],
    user_team_id: &str,
) -> i16 {
    let mut morale_delta: i16 = 0;

    for answer in answers {
        let effect_id = answer
            .get("effect_id")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let Some((target, delta)) = press_effect_delta(effect_id) else {
            continue;
        };

        match target {
            "squad" => morale_delta += delta,
            "player" => {
                if let Some(player_id) = answer.get("player_id").and_then(|value| value.as_str()) {
                    if let Some(player) = game
                        .players
                        .iter_mut()
                        .find(|player| player.id == player_id)
                    {
                        player.morale = apply_delta(player.morale, delta);
                    }
                }
            }
            _ => {}
        }
    }

    morale_delta = morale_delta.clamp(-8, 8);
    if morale_delta != 0 {
        for player in game.players.iter_mut() {
            if player.team_id.as_deref() == Some(user_team_id) {
                player.morale = apply_delta(player.morale, morale_delta);
            }
        }
    }

    morale_delta
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureChampionPickInput {
    pub player_id: String,
    pub champion_id: String,
}

// ---------------------------------------------------------------------------
// Live Match Commands
// ---------------------------------------------------------------------------

fn finish_live_match_internal(
    state: &StateManager,
    lol_report: Option<LolSimMatchReportInput>,
    locale: Option<&str>,
    data_base: Option<&std::path::Path>,
) -> Result<FinishLiveMatchResponse, String> {
    finish_live_match_service(state, lol_report, locale, data_base)
}

fn apply_team_talk_internal(
    game: &mut Game,
    tone: &str,
    context: &str,
    seed: u64,
) -> Result<Vec<serde_json::Value>, String> {
    apply_team_talk_core(game, tone, context, seed)
}

/// Start a live match for a given fixture.
/// mode: "live" | "spectator" | "instant"
#[tauri::command]
pub fn start_live_match(
    state: State<'_, StateManager>,
    fixture_index: usize,
    mode: String,
    allows_extra_time: bool,
) -> Result<olm_core::engine::MatchSnapshot, String> {
    start_live_match_service(&state, fixture_index, &mode, allows_extra_time)
}

/// Evaluates a completed manual draft without serializing hidden meta or unapproved rival data.
#[tauri::command]
pub fn evaluate_draft_state(
    state: State<'_, StateManager>,
    input: DraftStateInput,
) -> Result<DraftStateEvaluation, String> {
    let game = state
        .get_game(|game: &Game| game.clone())
        .ok_or("No active game session".to_string())?;
    evaluate_draft_state_core(&game, &input)
}

/// Step the live match forward by N minutes. Returns the events from each minute.
#[tauri::command]
pub fn step_live_match(
    state: State<'_, StateManager>,
    minutes: u16,
) -> Result<Vec<olm_core::engine::MinuteResult>, String> {
    step_live_match_service(&state, minutes)
}

/// Apply a match command (substitution, tactic change, set piece taker, etc.)
#[tauri::command]
pub fn apply_match_command(
    state: State<'_, StateManager>,
    command: olm_core::engine::MatchCommand,
) -> Result<olm_core::engine::MatchSnapshot, String> {
    apply_match_command_service(&state, command)
}

/// Get current match snapshot without advancing time.
#[tauri::command]
pub fn get_match_snapshot(
    state: State<'_, StateManager>,
) -> Result<olm_core::engine::MatchSnapshot, String> {
    get_match_snapshot_service(&state)
}

/// Finish the live match: generate report, update game state, clean up.
#[tauri::command]
pub fn finish_live_match(
    app_handle: tauri::AppHandle,
    state: State<'_, StateManager>,
    lol_report: Option<LolSimMatchReportInput>,
) -> Result<FinishLiveMatchResponse, String> {
    let settings = crate::commands::settings::get_settings(app_handle.clone()).unwrap_or_default();
    let data_base = crate::commands::competitions::resolve_data_base(&app_handle);
    finish_live_match_internal(
        &state,
        lol_report,
        Some(settings.language.as_str()),
        data_base.as_deref(),
    )
}

#[tauri::command]
pub fn record_fixture_champion_picks(
    state: State<'_, StateManager>,
    fixture_id: String,
    winner_team_id: String,
    picks: Vec<FixtureChampionPickInput>,
    bans: Vec<String>,
) -> Result<Game, String> {
    info!(
        "[cmd] record_fixture_champion_picks: fixture={}, picks={}, bans={}",
        fixture_id,
        picks.len(),
        bans.len()
    );

    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let league = game
        .leagues
        .first_mut()
        .ok_or("No active league in game state".to_string())?;
    let fixture = league
        .fixtures
        .iter_mut()
        .find(|candidate| candidate.id == fixture_id)
        .ok_or_else(|| format!("Fixture not found: {}", fixture_id))?;
    if fixture.result.is_none() {
        return Err("Fixture has no completed result yet".to_string());
    }

    let bans_json = serde_json::to_string(&bans).unwrap_or_default();

    state.with_stats_state(|stats| {
        for record in stats
            .player_matches
            .iter_mut()
            .filter(|record| record.fixture_id == fixture_id)
        {
            record.champion = picks
                .iter()
                .find(|pick| pick.player_id == record.player_id)
                .map(|pick| pick.champion_id.clone());
            record.bans_json = bans_json.clone();
            record.result = if record.team_id == winner_team_id {
                MatchOutcome::Win
            } else {
                MatchOutcome::Loss
            };
        }
    });

    let mastery_picks: Vec<(String, String)> = picks
        .iter()
        .map(|pick| (pick.player_id.clone(), pick.champion_id.clone()))
        .collect();
    olm_core::champions::apply_match_mastery_progress(&mut game, &winner_team_id, &mastery_picks);

    state.set_game(game.clone());
    Ok(crate::client_game::game_for_client(&game))
}

#[tauri::command]
pub fn apply_champion_mastery_from_draft(
    state: State<'_, StateManager>,
    winner_team_id: String,
    picks: Vec<FixtureChampionPickInput>,
) -> Result<Game, String> {
    info!(
        "[cmd] apply_champion_mastery_from_draft: picks={}",
        picks.len()
    );

    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let mastery_picks: Vec<(String, String)> = picks
        .iter()
        .map(|pick| (pick.player_id.clone(), pick.champion_id.clone()))
        .collect();
    olm_core::champions::apply_match_mastery_progress(&mut game, &winner_team_id, &mastery_picks);

    state.set_game(game.clone());
    Ok(crate::client_game::game_for_client(&game))
}

/// Apply a team talk and return per-player morale changes.
/// tone: "calm" | "motivational" | "assertive" | "aggressive" | "praise" | "disappointed"
/// context: "winning" | "losing" | "drawing"
#[tauri::command]
pub fn apply_team_talk(
    state: State<'_, StateManager>,
    tone: String,
    context: String,
) -> Result<Vec<serde_json::Value>, String> {
    info!("[cmd] apply_team_talk: tone={}, context={}", tone, context);
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session")?;
    let seed = rand::rng().random::<u64>();
    let results = apply_team_talk_internal(&mut game, &tone, &context, seed)?;

    state.set_game(game);
    Ok(results)
}

/// Process press conference answers: generate news article, affect squad morale.
/// answers: array of { question_id, response_id, response_tone, response_text, question_text }
#[tauri::command]
pub fn submit_press_conference(
    state: State<'_, StateManager>,
    answers: Vec<serde_json::Value>,
    home_team: String,
    away_team: String,
    home_score: u8,
    away_score: u8,
    user_team_name: String,
    user_team_id: String,
    prerendered_body: Option<String>,
    prerendered_headline: Option<String>,
) -> Result<serde_json::Value, String> {
    info!(
        "[cmd] submit_press_conference: {} {} - {} {}",
        home_team, home_score, away_score, away_team
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session")?;

    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let mut rng = rand::rng();

    // Build news article from press conference answers
    let mut quotes: Vec<String> = Vec::new();
    let mut morale_delta: i16 = 0;
    let mut mentioned_player_ids: Vec<String> = Vec::new();
    let has_stable_effects = answers.iter().any(|answer| {
        answer
            .get("effect_id")
            .and_then(|value| value.as_str())
            .is_some_and(|effect_id| !effect_id.is_empty())
    });

    for answer in &answers {
        let tone = answer
            .get("response_tone")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let text = answer
            .get("response_text")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let qid = answer
            .get("question_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !text.is_empty() {
            quotes.push(format!("\"{}\"", text));
        }

        // Track player mentions
        if let Some(pid) = answer.get("player_id").and_then(|v| v.as_str()) {
            if !pid.is_empty() {
                mentioned_player_ids.push(pid.to_string());
            }
        }

        if !has_stable_effects {
            // Legacy morale effects based on localized tone. Kept only for old payloads.
            match tone {
                "Humble" | "Fair" | "Positive" | "Focused" => {
                    morale_delta += rng.random_range(1..=3)
                }
                "Confident" | "Ambitious" => morale_delta += rng.random_range(2..=5),
                "Defiant" | "Frustrated" => morale_delta += rng.random_range(-2..=2),
                "Curt" | "Evasive" => morale_delta += rng.random_range(-3..=0),
                "Accept" | "Detailed" => morale_delta += rng.random_range(0..=2),
                "Deflect" => morale_delta += rng.random_range(-1..=1),
                "Praise" => morale_delta += rng.random_range(3..=6),
                "Demanding" => morale_delta += rng.random_range(-2..=3),
                _ => {}
            }
        }

        // Legacy player-focused question effects.
        if !has_stable_effects && qid == "player_focus" {
            if let Some(pid) = answer.get("player_id").and_then(|v| v.as_str()) {
                if !pid.is_empty() {
                    let player_delta: i16 = match tone {
                        "Praise" => rng.random_range(4..=8),
                        "Demanding" => rng.random_range(-3..=4),
                        "Deflect" => rng.random_range(-2..=1),
                        _ => rng.random_range(0..=3),
                    };
                    if let Some(p) = game.players.iter_mut().find(|p| p.id == pid) {
                        p.morale = ((p.morale as i16) + player_delta).clamp(10, 100) as u8;
                    }
                }
            }
        }
    }

    if has_stable_effects {
        morale_delta = apply_press_conference_effects(&mut game, &answers, &user_team_id);
    } else {
        // Apply legacy squad-wide morale effect
        morale_delta = morale_delta.clamp(-8, 8);
        if morale_delta != 0 {
            for p in game.players.iter_mut() {
                if p.team_id.as_deref() == Some(&user_team_id) {
                    p.morale = apply_delta(p.morale, morale_delta);
                }
            }
        }
    }

    // Generate news article
    let result_str = format!(
        "{} {} - {} {}",
        home_team, home_score, away_score, away_team
    );
    let headline = prerendered_headline.unwrap_or_else(|| {
        if quotes.is_empty() {
            format!("Post-Match: {} on {}", user_team_name, result_str)
        } else {
            let sources = [
                format!("{} Manager: {}", user_team_name, quotes[0]),
                format!(
                    "Press Conference: \"{}\" — {} boss",
                    quotes[0].trim_matches('"'),
                    user_team_name
                ),
            ];
            sources[rng.random_range(0..sources.len())].clone()
        }
    });

    let body = prerendered_body.unwrap_or_else(|| {
        if quotes.len() > 1 {
            format!(
                "Speaking after the {} result, the {} manager addressed the press.\n\n{}\n\n\
                The conference covered the result, tactical approach, and what lies ahead for the team.",
                result_str, user_team_name,
                quotes.iter().map(|q| format!("• {}", q)).collect::<Vec<_>>().join("\n")
            )
        } else if quotes.len() == 1 {
            format!(
                "The {} manager spoke briefly after the {} result.\n\n{}",
                user_team_name, result_str, quotes[0]
            )
        } else {
            format!(
                "The {} manager declined to speak at length after the {} result.",
                user_team_name, result_str
            )
        }
    });

    let article_id = format!("press_conf_{}", today);
    let article = olm_core::domain::news::NewsArticle::new(
        article_id,
        headline,
        body,
        "Sports Daily".to_string(),
        today.clone(),
        olm_core::domain::news::NewsCategory::MatchReport,
    )
    .with_teams(vec![user_team_id.clone()]);

    game.news.push(article);
    state.set_game(game.clone());

    Ok(serde_json::json!({
        "game": crate::client_game::game_for_client(&game),
        "morale_delta": morale_delta
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_press_conference_effects, apply_team_talk_internal, finish_live_match_internal,
    };
    use chrono::{TimeZone, Utc};
    use olm_core::clock::GameClock;
    use olm_core::domain::league::{
        Fixture, FixtureStatus, League, LeagueKind, MatchType, StandingEntry,
    };
    use olm_core::domain::manager::Manager;
    use olm_core::domain::player::{
        LolRole, Player, PlayerAttributes, PlayerIssue, PlayerIssueCategory,
    };
    use olm_core::domain::team::Team;
    use olm_core::game::Game;
    use olm_core::live_match_manager::{self, MatchMode};
    use olm_core::state::StateManager;

    fn default_attrs(position: LolRole) -> PlayerAttributes {
        let is_goalkeeper = matches!(position, LolRole::Support);

        PlayerAttributes {
            mechanics: if is_goalkeeper { 30 } else { 65 },
            laning: if is_goalkeeper { 30 } else { 65 },
            teamfighting: 65,
            macro_play: 65,
            consistency: 65,
            shotcalling: 50,
            champion_pool: 65,
            discipline: 65,
            mental_resilience: 65,
        }
    }

    fn make_player(id: &str, name: &str, team_id: &str, position: LolRole) -> Player {
        let mut player = Player::new(
            id.to_string(),
            name.to_string(),
            name.to_string(),
            "1995-01-01".to_string(),
            "England".to_string(),
            position.clone(),
            default_attrs(position),
        );
        player.team_id = Some(team_id.to_string());
        player.condition = 100;
        player.morale = 70;
        player
    }

    fn make_team(id: &str, name: &str) -> Team {
        Team::new(
            id.to_string(),
            name.to_string(),
            name[..3].to_string(),
            "England".to_string(),
            "London".to_string(),
            "Stadium".to_string(),
            40_000,
        )
    }

    fn make_squad(team_id: &str, prefix: &str) -> Vec<Player> {
        let mut players = Vec::new();
        players.push(make_player(
            &format!("{}_gk", prefix),
            &format!("{} GK", prefix),
            team_id,
            LolRole::Support,
        ));
        for index in 0..4 {
            players.push(make_player(
                &format!("{}_def{}", prefix, index),
                &format!("{} Def{}", prefix, index),
                team_id,
                LolRole::Top,
            ));
        }
        for index in 0..4 {
            players.push(make_player(
                &format!("{}_mid{}", prefix, index),
                &format!("{} Mid{}", prefix, index),
                team_id,
                LolRole::Jungle,
            ));
        }
        for index in 0..2 {
            players.push(make_player(
                &format!("{}_fwd{}", prefix, index),
                &format!("{} Fwd{}", prefix, index),
                team_id,
                LolRole::Adc,
            ));
        }
        players
    }

    fn make_game_with_round() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team1".to_string());

        let teams = vec![
            make_team("team1", "Home FC"),
            make_team("team2", "Away FC"),
            make_team("team3", "Third FC"),
            make_team("team4", "Fourth FC"),
        ];
        let mut players = make_squad("team1", "t1");
        players.extend(make_squad("team2", "t2"));
        players.extend(make_squad("team3", "t3"));
        players.extend(make_squad("team4", "t4"));

        let league = League {
            id: "league1".to_string(),
            name: "Test League".to_string(),
            season: 1,
            competition_id: None,
            logo: None,
            league_kind: LeagueKind::Main,
            split_index: 0,
            tier: 0,
            active: false,
            fixtures: vec![Fixture {
                id: "fix1".to_string(),
                matchday: 1,
                date: "2025-06-15".to_string(),
                home_team_id: "team1".to_string(),
                away_team_id: "team2".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Scheduled,
                result: None,
            }],
            standings: vec![
                StandingEntry::new("team1".to_string()),
                StandingEntry::new("team2".to_string()),
                StandingEntry::new("team3".to_string()),
                StandingEntry::new("team4".to_string()),
            ],
        };

        let mut game = Game::new(clock, manager, teams, players, vec![], vec![]);
        game.leagues = vec![league];
        game
    }

    fn delta_for(results: &[serde_json::Value], player_id: &str) -> i64 {
        results
            .iter()
            .find(|result| result["player_id"] == player_id)
            .and_then(|result| result["delta"].as_i64())
            .unwrap()
    }

    #[test]
    fn stable_press_effect_id_applies_squad_morale_once() {
        let mut game = make_game_with_round();
        apply_press_conference_effects(
            &mut game,
            &[serde_json::json!({ "effect_id": "press_squad_morale_small_up" })],
            "team1",
        );

        let team_morale: Vec<u8> = game
            .players
            .iter()
            .filter(|player| player.team_id.as_deref() == Some("team1"))
            .map(|player| player.morale)
            .collect();
        let opponent_morale: Vec<u8> = game
            .players
            .iter()
            .filter(|player| player.team_id.as_deref() == Some("team2"))
            .map(|player| player.morale)
            .collect();

        assert!(team_morale.iter().all(|morale| *morale == 73));
        assert!(opponent_morale.iter().all(|morale| *morale == 70));
    }

    #[test]
    fn stable_press_effect_id_applies_player_morale_once() {
        let mut game = make_game_with_round();
        apply_press_conference_effects(
            &mut game,
            &[
                serde_json::json!({
                    "effect_id": "press_player_pressure_small_down",
                    "player_id": "t1_mid0"
                }),
                serde_json::json!({ "effect_id": "press_no_effect", "player_id": "t1_mid0" }),
            ],
            "team1",
        );

        let focused = game
            .players
            .iter()
            .find(|player| player.id == "t1_mid0")
            .unwrap();
        let teammate = game
            .players
            .iter()
            .find(|player| player.id == "t1_mid1")
            .unwrap();

        assert_eq!(focused.morale, 68);
        assert_eq!(teammate.morale, 70);
    }

    #[test]
    fn finish_live_match_returns_completed_round_summary_response() {
        let state = StateManager::new();
        let mut game = make_game_with_round();
        let today = game.clock.current_date.format("%Y-%m-%d").to_string();
        olm_core::turn::simulate_other_matches(&mut game, &today, Some(0));

        let mut session =
            live_match_manager::create_live_match(&game, 0, MatchMode::Instant, false).unwrap();
        session.user_side = None;
        session.run_to_completion();

        state.set_game(game);
        state.set_live_match(session);

        let response = finish_live_match_internal(&state, None, None, None)
            .expect("finish live match response");

        let round_summary = response.round_summary.expect("round summary response");
        assert!(round_summary.is_complete);
        assert_eq!(round_summary.pending_fixture_count, 0);
        // The fixture builder defines one match in matchday 1; the summary includes
        // completed fixtures from that matchday, not every team in the standings.
        assert_eq!(round_summary.completed_results.len(), 1);
        assert_eq!(
            response
                .game
                .clock
                .current_date
                .format("%Y-%m-%d")
                .to_string(),
            "2025-06-16"
        );
    }

    #[test]
    fn team_talk_reactions_vary_by_player_context() {
        let mut game = make_game_with_round();
        let composed = game
            .players
            .iter_mut()
            .find(|player| player.id == "t1_mid0")
            .unwrap();
        composed.attributes.discipline = 90;
        composed.attributes.shotcalling = 90;
        composed.morale_core.manager_trust = 80;

        let volatile = game
            .players
            .iter_mut()
            .find(|player| player.id == "t1_fwd0")
            .unwrap();
        volatile.attributes.discipline = 20;
        volatile.attributes.shotcalling = 20;
        volatile.morale_core.manager_trust = 25;
        volatile.morale_core.unresolved_issue = Some(PlayerIssue {
            category: PlayerIssueCategory::Morale,
            severity: 70,
        });

        let results = apply_team_talk_internal(&mut game, "aggressive", "winning", 7).unwrap();

        assert!(delta_for(&results, "t1_mid0") > delta_for(&results, "t1_fwd0"));
    }

    #[test]
    fn repeating_same_team_talk_loses_effectiveness() {
        let mut game = make_game_with_round();
        let player = game
            .players
            .iter_mut()
            .find(|player| player.id == "t1_mid0")
            .unwrap();
        player.morale = 50;
        player.morale_core.manager_trust = 70;

        let first = apply_team_talk_internal(&mut game, "motivational", "losing", 13).unwrap();
        let second = apply_team_talk_internal(&mut game, "motivational", "losing", 13).unwrap();

        assert!(delta_for(&second, "t1_mid0") <= delta_for(&first, "t1_mid0"));
    }
}
