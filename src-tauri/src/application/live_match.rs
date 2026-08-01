use log::info;
use std::collections::HashMap;

use crate::commands::round_summary::{build_round_summary_dto, RoundSummaryDto};
use olm_core::engine::event::{EventType, MatchEvent};
use olm_core::engine::report::{MatchReport, MatchReportEndReason, PlayerMatchStats, TeamStats};
use olm_core::engine::types::{Side, Zone};
use olm_core::game::Game;
use olm_core::live_match_manager::{self, MatchMode};
use olm_core::roster_stability;
use olm_core::state::StateManager;
use serde::{Deserialize, Serialize};

fn role_to_string(role: &olm_core::domain::stats::LolRole) -> &'static str {
    use olm_core::domain::stats::LolRole;
    match role {
        LolRole::Top => "TOP",
        LolRole::Jungle => "JUNGLE",
        LolRole::Mid => "MID",
        LolRole::Adc => "ADC",
        LolRole::Support => "SUPPORT",
        LolRole::Unknown => "UNKNOWN",
    }
}

fn validate_user_team_role_coverage(game: &Game) -> Result<(), String> {
    let Some(user_team_id) = game.manager.team_id.as_deref() else {
        return Ok(());
    };

    let role_set: std::collections::HashSet<&'static str> = game
        .players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(user_team_id))
        .map(|player| role_to_string(&player.natural_position))
        .collect();
    let required_roles = ["TOP", "JUNGLE", "MID", "ADC", "SUPPORT"];
    let missing_roles: Vec<&str> = required_roles
        .iter()
        .copied()
        .filter(|role| !role_set.contains(role))
        .collect();

    if missing_roles.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Main roster role coverage incomplete: missing {}. You need at least one TOP, JUNGLE, MID, ADC, and SUPPORT before starting a match.",
            missing_roles.join(", ")
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinishLiveMatchResponse {
    pub game: Game,
    pub round_summary: Option<RoundSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LolSimMatchReportInput {
    pub winner: Option<String>,
    pub time_sec: f64,
    pub events: Vec<LolSimMatchReportEventInput>,
    pub stats: LolSimMatchReportStatsInput,
    pub champions: Vec<LolSimMatchReportChampionInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LolSimMatchReportEventInput {
    pub t: f64,
    pub text: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LolSimMatchReportStatsInput {
    pub blue: LolSimMatchReportTeamStatsInput,
    pub red: LolSimMatchReportTeamStatsInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LolSimMatchReportTeamStatsInput {
    pub kills: u16,
    pub deaths: u16,
    pub gold: u32,
    pub towers: u16,
    pub dragons: u16,
    pub barons: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LolSimMatchReportChampionInput {
    pub id: String,
    pub name: String,
    pub team: String,
    pub role: String,
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    pub cs: u16,
    pub gold: u32,
    pub spent_gold: u32,
}

fn map_sim_team_to_side(team: &str) -> Side {
    if team.eq_ignore_ascii_case("red") {
        Side::Away
    } else {
        Side::Home
    }
}

fn map_sim_role_to_engine(role: &str) -> Option<olm_core::engine::sim_background::LolRole> {
    match role {
        "TOP" => Some(olm_core::engine::sim_background::LolRole::Top),
        "JGL" => Some(olm_core::engine::sim_background::LolRole::Jungle),
        "MID" => Some(olm_core::engine::sim_background::LolRole::Mid),
        "ADC" => Some(olm_core::engine::sim_background::LolRole::Adc),
        "SUP" => Some(olm_core::engine::sim_background::LolRole::Support),
        _ => None,
    }
}

fn map_sim_event_kind_to_engine(kind: &str) -> Option<EventType> {
    match kind {
        "kill" => Some(EventType::Kill),
        "tower" => Some(EventType::TowerDestroyed),
        "dragon" | "baron" => Some(EventType::ObjectiveTaken),
        "nexus" => Some(EventType::NexusDestroyed),
        "spawn" => Some(EventType::ObjectiveSpawned),
        _ => None,
    }
}

fn parse_kill_text(text: &str) -> Option<(String, String)> {
    let (killer, victim) = text.split_once(" killed ")?;
    let killer = killer.trim();
    let victim = victim.trim();
    if killer.is_empty() || victim.is_empty() {
        return None;
    }
    Some((killer.to_string(), victim.to_string()))
}

fn infer_event_side(
    text: &str,
    killer_name: Option<&str>,
    champion_name_to_side: &HashMap<String, Side>,
) -> Side {
    let lower_text = text.to_lowercase();
    if lower_text.contains("blue") {
        return Side::Home;
    }
    if lower_text.contains("red") {
        return Side::Away;
    }
    if let Some(killer) = killer_name {
        let key = killer.to_lowercase();
        if let Some(side) = champion_name_to_side.get(&key) {
            return *side;
        }
    }
    Side::Home
}

fn build_match_report_from_lol_sim(input: LolSimMatchReportInput) -> MatchReport {
    let mut champion_name_to_side: HashMap<String, Side> = HashMap::new();
    let mut champion_name_to_id: HashMap<String, String> = HashMap::new();
    for champion in &input.champions {
        let key = champion.name.to_lowercase();
        champion_name_to_side.insert(key.clone(), map_sim_team_to_side(&champion.team));
        champion_name_to_id.insert(key, champion.id.clone());
    }

    let events = input
        .events
        .into_iter()
        .filter_map(|event| {
            let mut event_type = map_sim_event_kind_to_engine(&event.kind)?;
            if matches!(event_type, EventType::TowerDestroyed)
                && event.text.to_lowercase().contains("inhib")
            {
                event_type = EventType::InhibitorDestroyed;
            }

            let kill_data = if matches!(event_type, EventType::Kill) {
                parse_kill_text(&event.text)
            } else {
                None
            };

            let side = infer_event_side(
                &event.text,
                kill_data.as_ref().map(|(killer, _)| killer.as_str()),
                &champion_name_to_side,
            );

            let player_id = kill_data
                .as_ref()
                .and_then(|(killer, _)| champion_name_to_id.get(&killer.to_lowercase()).cloned());
            let secondary_player_id = kill_data
                .as_ref()
                .and_then(|(_, victim)| champion_name_to_id.get(&victim.to_lowercase()).cloned());

            Some(MatchEvent {
                minute: (event.t / 60.0).round().clamp(0.0, 255.0) as u8,
                event_type,
                side,
                zone: Zone::Midfield,
                player_id,
                secondary_player_id,
            })
        })
        .collect::<Vec<_>>();

    let mut player_stats: HashMap<String, PlayerMatchStats> = HashMap::new();
    for champion in input.champions {
        player_stats.insert(
            champion.id,
            PlayerMatchStats {
                role: map_sim_role_to_engine(&champion.role),
                duration_seconds: input.time_sec.round().max(0.0) as u32,
                kills: champion.kills,
                deaths: champion.deaths,
                assists: champion.assists,
                creep_score: champion.cs,
                gold_earned: champion.gold.saturating_add(champion.spent_gold),
                damage_dealt: 0,
                vision_score: 0,
                wards_placed: 0,
                ..Default::default()
            },
        );
    }

    let winner_side = input
        .winner
        .as_deref()
        .map(map_sim_team_to_side)
        .unwrap_or_else(|| {
            if input.stats.blue.kills >= input.stats.red.kills {
                Side::Home
            } else {
                Side::Away
            }
        });

    let (home_wins, away_wins) = match winner_side {
        Side::Home => (1, 0),
        Side::Away => (0, 1),
    };

    MatchReport {
        home_wins,
        away_wins,
        home_stats: TeamStats {
            kills: input.stats.blue.kills,
            deaths: input.stats.blue.deaths,
            gold_earned: input.stats.blue.gold,
            damage_dealt: 0,
            objectives: input
                .stats
                .blue
                .towers
                .saturating_add(input.stats.blue.dragons)
                .saturating_add(input.stats.blue.barons),
            possession_ticks: 0,
            ..Default::default()
        },
        away_stats: TeamStats {
            kills: input.stats.red.kills,
            deaths: input.stats.red.deaths,
            gold_earned: input.stats.red.gold,
            damage_dealt: 0,
            objectives: input
                .stats
                .red
                .towers
                .saturating_add(input.stats.red.dragons)
                .saturating_add(input.stats.red.barons),
            possession_ticks: 0,
            ..Default::default()
        },
        events,
        kill_feed: vec![],
        player_stats,
        home_possession: 50.0,
        total_minutes: (input.time_sec / 60.0).round().clamp(0.0, 255.0) as u8,
        game_duration_seconds: input.time_sec.round().max(0.0) as u32,
        ended_by: if input.winner.is_some() {
            MatchReportEndReason::NexusDestroyed
        } else {
            MatchReportEndReason::TimeLimit
        },
    }
}

pub fn finish_live_match(
    state: &StateManager,
    lol_report: Option<LolSimMatchReportInput>,
    locale: Option<&str>,
    data_base: Option<&std::path::Path>,
) -> Result<FinishLiveMatchResponse, String> {
    info!("[cmd] finish_live_match");
    let session = state.take_live_match().ok_or("No active live match")?;

    let fixture_index = session.fixture_index;
    let round_matchday = session.round_matchday;
    let round_previous_standings = session.round_previous_standings.clone();
    let home_team_id = session.home_team_id.clone();
    let away_team_id = session.away_team_id.clone();

    let report = if let Some(input) = lol_report {
        build_match_report_from_lol_sim(input)
    } else {
        session.match_state.into_report()
    };
    info!(
        "[cmd] finish_live_match: fixture_index={}, home_team_id={}, away_team_id={}, events= {}",
        fixture_index,
        home_team_id,
        away_team_id,
        report.events.len()
    );

    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session")?;

    let mut captures = Vec::new();
    olm_core::turn::apply_match_report_with_capture(
        &mut game,
        fixture_index,
        &home_team_id,
        &away_team_id,
        &report,
        &mut |capture| captures.push(capture),
    );
    for capture in captures {
        state.append_stats_state(capture);
    }

    olm_core::social::generate_match_social_posts(
        &mut game,
        fixture_index,
        &report,
        locale,
        data_base,
    );

    let round_summary = build_round_summary_dto(&game, round_matchday, &round_previous_standings);

    olm_core::turn::finish_live_match_day(&mut game);

    state.set_game(game.clone());
    Ok(FinishLiveMatchResponse {
        game,
        round_summary,
    })
}

pub fn start_live_match(
    state: &StateManager,
    fixture_index: usize,
    mode: &str,
    allows_extra_time: bool,
) -> Result<olm_core::engine::MatchSnapshot, String> {
    info!(
        "[cmd] start_live_match: fixture={}, mode={}, extra_time={}",
        fixture_index, mode, allows_extra_time
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session")?;

    validate_user_team_role_coverage(&game)?;

    // Pre-match: ensure both teams are match eligible before building engine teams.
    // For the user team, repair_team is a no-op (skips non-schedulable teams).
    // For the AI opponent, repair fills missing roles, reconciles lineups, etc.
    let match_team_ids: Vec<String> = {
        let league = game
            .active_league()
            .ok_or("No active league for start_live_match")?;
        let fixture = league
            .fixtures
            .get(fixture_index)
            .ok_or_else(|| format!("Fixture index {fixture_index} out of range"))?;
        vec![fixture.home_team_id.clone(), fixture.away_team_id.clone()]
    };
    for team_id in &match_team_ids {
        if !team_id.is_empty() {
            roster_stability::repair_team(
                &mut game,
                team_id,
                roster_stability::RosterStabilityReason::PreMatch,
            )
            .map_err(|e| {
                format!("Team {team_id} roster is invalid and could not be repaired: {e}")
            })?;
        }
    }
    // Sync repaired game back to state so live_match_manager uses current data
    state.set_game(game.clone());

    let match_mode = match mode {
        "spectator" => MatchMode::Spectator,
        "instant" => MatchMode::Instant,
        _ => MatchMode::Live,
    };

    let session =
        live_match_manager::create_live_match(&game, fixture_index, match_mode, allows_extra_time)?;
    let snapshot = session.snapshot();
    info!(
        "[cmd] start_live_match: created fixture={}, phase={:?}, home_team={}, away_team={}, home_players={}, away_players={}",
        fixture_index,
        snapshot.phase,
        snapshot.home_team.name,
        snapshot.away_team.name,
        snapshot.home_team.players.len(),
        snapshot.away_team.players.len()
    );
    state.set_live_match(session);
    Ok(snapshot)
}

pub fn step_live_match(
    state: &StateManager,
    minutes: u16,
) -> Result<Vec<olm_core::engine::MinuteResult>, String> {
    log::debug!("[cmd] step_live_match: minutes={}", minutes);
    let results = state
        .with_live_match(|session| {
            if minutes <= 1 {
                vec![session.step()]
            } else {
                session.step_many(minutes)
            }
        })
        .ok_or_else(|| "No active live match".to_string())?;

    if let Some(last) = results.last() {
        info!(
            "[cmd] step_live_match: minutes={}, result_count={}, last_minute={}, phase={:?}, finished={}",
            minutes,
            results.len(),
            last.minute,
            last.phase,
            last.is_finished
        );
    }

    Ok(results)
}

pub fn apply_match_command(
    state: &StateManager,
    command: olm_core::engine::MatchCommand,
) -> Result<olm_core::engine::MatchSnapshot, String> {
    info!("[cmd] apply_match_command: {:?}", command);
    let snapshot = state
        .with_live_match(|session| {
            session.apply_command(command)?;
            Ok::<olm_core::engine::MatchSnapshot, String>(session.snapshot())
        })
        .ok_or_else(|| "No active live match".to_string())??;

    info!(
        "[cmd] apply_match_command: snapshot phase={:?}, minute={}, home_players={}, away_players={}",
        snapshot.phase,
        snapshot.current_minute,
        snapshot.home_team.players.len(),
        snapshot.away_team.players.len()
    );

    Ok(snapshot)
}

pub fn get_match_snapshot(state: &StateManager) -> Result<olm_core::engine::MatchSnapshot, String> {
    log::debug!("[cmd] get_match_snapshot");
    let snapshot = state
        .with_live_match(|session| session.snapshot())
        .ok_or_else(|| "No active live match".to_string())?;

    info!(
        "[cmd] get_match_snapshot: phase={:?}, minute={}, home_team={}, away_team={}, home_roles={:?}, away_roles={:?}, events={}",
        snapshot.phase, snapshot.current_minute, snapshot.home_team.name, snapshot.away_team.name,
        snapshot.home_roles, snapshot.away_roles, snapshot.events.len()
    );

    Ok(snapshot)
}
