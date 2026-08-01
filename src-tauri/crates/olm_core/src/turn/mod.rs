pub(crate) mod news;
mod post_match;
mod round_summary;

use crate::board_objectives;
use crate::champions;
use crate::contract_wage_policy::ai_transfer_cap_reset_if_new_day;
use crate::domain::league::{Fixture, FixtureStatus, League, MatchResult, MatchType};
use crate::domain::message::{InboxMessage, MessageCategory, MessageContext, MessagePriority};
use crate::domain::player::{LolRole as DomainLolRole, Player};
use crate::domain::stats::StatsState;
use crate::domain::team::{Team, TeamKind, TeamSeasonRecord};
use crate::end_of_season;
use crate::engine::LolRole as EngineLolRole;
use crate::game::Game;
use crate::player_events;
use crate::potential;
use crate::random_events;
use crate::schedule;
use crate::scouting;
use crate::training;
use crate::transfers;
use chrono::Datelike;
use log::{debug, info};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

// Re-export public items
pub use news::generate_matchday_news;
pub use post_match::{apply_match_report, apply_match_report_with_capture};
pub use round_summary::{
    NotableUpset, RoundResultSummary, RoundSummary, StandingDelta, TopScorerDelta,
    build_round_summary,
};

/// Process a single day advance.
pub fn process_day(game: &mut Game) {
    process_day_with_capture(game, &mut |_| {});
}

fn snapshot_transfer_listed_player_ids(game: &Game) -> HashSet<String> {
    game.players
        .iter()
        .filter(|player| player.transfer_listed)
        .map(|player| player.id.clone())
        .collect()
}

/// Run strategic recruitment for every AI main team before the generic
/// transfer simulation paths execute. Eligibility (reputation/tier/budget)
/// is evaluated inside `ai_team_agent::ai_strategic_recruitment`.
fn run_ai_strategic_recruitment(game: &mut Game) {
    let team_ids: Vec<String> = game
        .teams
        .iter()
        .filter(|team| team.team_kind == TeamKind::Main && team.manager_id.is_none())
        .map(|team| team.id.clone())
        .collect();

    for team_id in team_ids {
        crate::ai_team_agent::ai_strategic_recruitment(game, &team_id);
    }
}

pub fn process_day_with_capture<F>(game: &mut Game, on_capture: &mut F)
where
    F: FnMut(StatsState),
{
    ai_transfer_cap_reset_if_new_day(game);

    let today = game.clock.current_date.format("%Y-%m-%d").to_string();

    let has_match_today = game.active_league().is_some_and(|league| {
        league
            .fixtures
            .iter()
            .any(|f| f.date == today && f.status == FixtureStatus::Scheduled)
    });

    if has_match_today {
        info!("[turn] process_day {}: matchday", today);
        simulate_matchday_with_capture(game, &today, on_capture);
        maybe_schedule_playoffs(game);
    } else {
        let weekday_num = game.clock.current_date.weekday().num_days_from_monday();
        training::process_training(game, weekday_num);
        training::check_squad_fitness_warnings(game);
    }

    crate::contracts::process_contract_expiries(game);

    // Monthly financial processing (wages, matchday income, warnings)
    crate::finances::process_monthly_finances(game);

    // Board objectives (generate if missing, update progress)
    board_objectives::generate_objectives(game);
    board_objectives::update_objective_progress(game);

    // Player conversations, random events, and scouting
    player_events::check_player_events(game);
    random_events::check_random_events(game);
    scouting::process_scouting(game);
    transfers::generate_incoming_transfer_offers(game);
    crate::ai_team_agent::process_ai_team_agents(game);
    run_ai_strategic_recruitment(game);
    let previously_transfer_listed = snapshot_transfer_listed_player_ids(game);
    crate::ai_player_agent::process_ai_player_agents(game);
    crate::ai_team_agent::resolve_conflicts(game, &previously_transfer_listed);
    news::generate_ai_transfer_news(game);
    maybe_simulate_parallel_academy_leagues(game);
    process_background_leagues(game, &today);
    maybe_push_weekly_academy_report(game, &today);

    news::generate_weekly_digest_news(game, &today);
    news::generate_pre_match_messages(game, &today);

    crate::firing::check_manager_firing(game);
    crate::job_offers::check_job_offers(game);
    potential::process_potential_research(game);
    champions::process_daily_champion_system(game);

    debug!("[turn] process_day {}: complete, advancing clock", today);
    game.clock.advance_days(1);
    game.day_phase = crate::game::DayPhase::Morning;
    crate::season_context::refresh_game_context(game);
}

/// Called after a live match finishes to complete the day:
/// generates matchday news, pre-match messages, and advances the clock by one day.
pub fn finish_live_match_day(game: &mut Game) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    ai_transfer_cap_reset_if_new_day(game);
    info!("[turn] finish_live_match_day: {}", today);
    generate_matchday_news(game, &today);
    maybe_schedule_playoffs(game);

    crate::contracts::process_contract_expiries(game);

    // Match normal day advancement: monthly finances are evaluated on the
    // current date before the clock advances.
    crate::finances::process_monthly_finances(game);

    board_objectives::generate_objectives(game);
    board_objectives::update_objective_progress(game);

    player_events::check_player_events(game);
    random_events::check_random_events(game);
    scouting::process_scouting(game);
    transfers::generate_incoming_transfer_offers(game);
    crate::ai_team_agent::process_ai_team_agents(game);
    run_ai_strategic_recruitment(game);
    let previously_transfer_listed = snapshot_transfer_listed_player_ids(game);
    crate::ai_player_agent::process_ai_player_agents(game);
    crate::ai_team_agent::resolve_conflicts(game, &previously_transfer_listed);
    news::generate_ai_transfer_news(game);
    maybe_simulate_parallel_academy_leagues(game);
    process_background_leagues(game, &today);
    maybe_push_weekly_academy_report(game, &today);
    news::generate_weekly_digest_news(game, &today);
    news::generate_pre_match_messages(game, &today);

    crate::firing::check_manager_firing(game);
    crate::job_offers::check_job_offers(game);
    potential::process_potential_research(game);
    champions::process_daily_champion_system(game);

    game.clock.advance_days(1);
    game.day_phase = crate::game::DayPhase::Morning;
    crate::season_context::refresh_game_context(game);
}

// ---------------------------------------------------------------------------
// Domain → Engine type conversion
// ---------------------------------------------------------------------------

fn build_engine_team_from(
    teams: &[Team],
    players: &[Player],
    team_id: &str,
) -> crate::engine::TeamData {
    let team = teams.iter().find(|t| t.id == team_id);

    let (name, draft_strategy) = match team {
        Some(t) => (
            t.name.clone(),
            match t.draft_strategy {
                crate::domain::team::DraftStrategy::Aggressive => {
                    crate::engine::DraftStrategy::Aggressive
                }
                crate::domain::team::DraftStrategy::Passive => {
                    crate::engine::DraftStrategy::Passive
                }
                crate::domain::team::DraftStrategy::Scaling => {
                    crate::engine::DraftStrategy::Scaling
                }
                crate::domain::team::DraftStrategy::CounterPick => {
                    crate::engine::DraftStrategy::CounterPick
                }
                crate::domain::team::DraftStrategy::PriorityBans => {
                    crate::engine::DraftStrategy::PriorityBans
                }
                _ => crate::engine::DraftStrategy::Balanced,
            },
        ),
        None => ("Unknown".into(), crate::engine::DraftStrategy::Balanced),
    };

    let mut lineup_ids = team
        .map(|team| team.active_lineup_ids.clone())
        .unwrap_or_default();
    lineup_ids.retain(|id| {
        players
            .iter()
            .any(|player| player.id == *id && player.team_id.as_deref() == Some(team_id))
    });
    if lineup_ids.len() < 5 {
        let mut fallback_ids: Vec<String> = players
            .iter()
            .filter(|player| player.team_id.as_deref() == Some(team_id))
            .map(|player| player.id.clone())
            .collect();
        fallback_ids.sort();
        for player_id in fallback_ids {
            if !lineup_ids.contains(&player_id) {
                lineup_ids.push(player_id);
            }
            if lineup_ids.len() == 5 {
                break;
            }
        }
    }
    let engine_players: Vec<crate::engine::PlayerData> = lineup_ids
        .into_iter()
        .take(5)
        .filter_map(|player_id| players.iter().find(|player| player.id == player_id))
        .map(|p| crate::engine::PlayerData {
            id: p.id.clone(),
            name: p.match_name.clone(),
            profile_image_url: p.profile_image_url.clone(),
            role: to_engine_role(p.natural_position),
            condition: p.condition,
            fitness: p.fitness,
            // Map OLD domain fields to NEW LoL-native engine structure
            // Physical+Technical+Mental -> LoL attributes (post-#204 alignment)
            mechanics: p.attributes.mechanics,
            laning: p.attributes.laning,
            teamfighting: p.attributes.teamfighting,
            macro_play: p.attributes.macro_play,
            consistency: p.attributes.consistency,
            shotcalling: p.attributes.shotcalling,
            champion_pool: p.attributes.champion_pool,
            discipline: p.attributes.discipline,
            mental_resilience: p.attributes.mental_resilience,
            traits: p.traits.iter().map(|t| format!("{:?}", t)).collect(),
        })
        .collect();

    crate::engine::TeamData {
        id: team_id.to_string(),
        name,
        draft_strategy,
        players: engine_players,
    }
}

fn build_engine_team(game: &Game, team_id: &str) -> crate::engine::TeamData {
    build_engine_team_from(&game.teams, &game.players, team_id)
}

fn academy_player_ovr(player: &crate::domain::player::Player) -> u32 {
    let attrs = &player.attributes;
    let total = u32::from(attrs.mechanics)
        + u32::from(attrs.laning)
        + u32::from(attrs.teamfighting)
        + u32::from(attrs.macro_play)
        + u32::from(attrs.consistency)
        + u32::from(attrs.shotcalling)
        + u32::from(attrs.champion_pool)
        + u32::from(attrs.discipline)
        + u32::from(attrs.mental_resilience);
    (total + 4) / 9
}

/// Convert crate::domain::player::LolRole to crate::engine::LolRole
fn to_engine_role(role: DomainLolRole) -> EngineLolRole {
    match role {
        DomainLolRole::Top => EngineLolRole::Top,
        DomainLolRole::Jungle => EngineLolRole::Jungle,
        DomainLolRole::Mid => EngineLolRole::Mid,
        DomainLolRole::Adc => EngineLolRole::Adc,
        DomainLolRole::Support => EngineLolRole::Support,
        DomainLolRole::Unknown => EngineLolRole::Top,
    }
}

fn maybe_push_weekly_academy_report(game: &mut Game, today: &str) {
    if game.clock.current_date.weekday().num_days_from_monday() != 0 {
        return;
    }

    let Some(parent_team_id) = game.manager.team_id.clone() else {
        return;
    };
    let Some(parent_team) = game.teams.iter().find(|team| team.id == parent_team_id) else {
        return;
    };

    let academy_team_id = parent_team.academy_team_id.clone().or_else(|| {
        game.teams
            .iter()
            .find(|team| {
                team.team_kind == TeamKind::Academy
                    && team.parent_team_id.as_deref() == Some(parent_team.id.as_str())
            })
            .map(|team| team.id.clone())
    });
    let Some(academy_team_id) = academy_team_id else {
        return;
    };
    let Some(academy_team) = game.teams.iter().find(|team| team.id == academy_team_id) else {
        return;
    };
    let season = game.clock.current_date.year() as u32;
    let academy_league_id = academy_team
        .academy
        .as_ref()
        .map(|metadata| metadata.erl_assignment.erl_league_id.clone())
        .unwrap_or_default();

    let mut league_rows: Vec<(String, String, u32, i32, u32, u32, u32)> = game
        .teams
        .iter()
        .filter(|team| {
            team.team_kind == TeamKind::Academy
                && team
                    .academy
                    .as_ref()
                    .map(|metadata| metadata.erl_assignment.erl_league_id.as_str())
                    == Some(academy_league_id.as_str())
        })
        .map(|team| {
            let record = team
                .history
                .iter()
                .find(|record| record.season == season)
                .cloned()
                .unwrap_or(TeamSeasonRecord {
                    season,
                    league_position: 0,
                    played: 0,
                    won: 0,
                    lost: 0,
                    kills_for: 0,
                    kills_against: 0,
                });
            let points = record.won.saturating_mul(3);
            let goal_diff = record.kills_for as i32 - record.kills_against as i32;
            (
                team.id.clone(),
                team.name.clone(),
                points,
                goal_diff,
                record.kills_for,
                record.won,
                record.lost,
            )
        })
        .collect();
    league_rows.sort_by(|left, right| {
        right
            .2
            .cmp(&left.2)
            .then_with(|| right.3.cmp(&left.3))
            .then_with(|| right.4.cmp(&left.4))
            .then_with(|| left.1.cmp(&right.1))
    });
    let academy_position = league_rows
        .iter()
        .position(|row| row.0 == academy_team_id)
        .map(|index| index + 1)
        .unwrap_or(league_rows.len().max(1));
    let table_preview = league_rows
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, row)| format!("{}. {} {}-{}", index + 1, row.1, row.5, row.6))
        .collect::<Vec<_>>()
        .join(" | ");

    let report_id = format!("academy-weekly-report-{}-{}", parent_team.id, today);
    if game.messages.iter().any(|message| message.id == report_id) {
        return;
    }

    let mut academy_players: Vec<&crate::domain::player::Player> = game
        .players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(academy_team_id.as_str()))
        .collect();

    if academy_players.is_empty() {
        let message = InboxMessage::new(
            report_id,
            "Academy weekly report".to_string(),
            format!(
                "Academy {} has no active players this week. Review acquisition and promotion flow to keep your pipeline healthy.",
                academy_team.name
            ),
            "Academy Coordinator".to_string(),
            today.to_string(),
        )
        .with_category(MessageCategory::Training)
        .with_priority(MessagePriority::Normal)
        .with_sender_role("Academy Coordinator")
        .with_i18n(
            "be.msg.academyWeeklyEmpty.subject",
            "be.msg.academyWeeklyEmpty.body",
            params(&[("academy", &academy_team.name)]),
        )
        .with_sender_i18n("be.sender.academyCoordinator", "be.role.academyCoordinator")
        .with_context(MessageContext {
            team_id: Some(parent_team.id.clone()),
            ..Default::default()
        });
        game.messages.push(message);
        return;
    }

    academy_players.sort_by_key(|player| std::cmp::Reverse(academy_player_ovr(player)));
    let avg_ovr = academy_players
        .iter()
        .map(|player| academy_player_ovr(player))
        .sum::<u32>()
        / academy_players.len() as u32;
    let high_potential = academy_players
        .iter()
        .filter(|player| player.potential_base >= 80)
        .count();
    let top_labels = academy_players
        .iter()
        .take(2)
        .map(|player| format!("{} ({})", player.match_name, academy_player_ovr(player)))
        .collect::<Vec<_>>()
        .join(", ");

    let main_players: Vec<&crate::domain::player::Player> = game
        .players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(parent_team.id.as_str()))
        .collect();
    let mut main_best_by_role: HashMap<EngineLolRole, u32> = HashMap::new();
    for player in main_players {
        let role = to_engine_role(player.natural_position);
        let ovr = academy_player_ovr(player);
        let entry = main_best_by_role.entry(role).or_insert(0);
        if ovr > *entry {
            *entry = ovr;
        }
    }
    let promotion_ready: Vec<String> = academy_players
        .iter()
        .filter_map(|player| {
            let role = to_engine_role(player.natural_position);
            let main_ref = main_best_by_role.get(&role).copied().unwrap_or(75);
            let academy_ovr = academy_player_ovr(player);
            (academy_ovr >= main_ref.saturating_sub(2)).then(|| player.match_name.clone())
        })
        .take(2)
        .collect();
    let recommendation = if promotion_ready.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nRecommendation: {} player(s) ready for promotion -> {}.",
            promotion_ready.len(),
            promotion_ready.join(", ")
        )
    };

    let message = InboxMessage::new(
        report_id,
        format!("Academy weekly report: {}", academy_team.name),
        format!(
            "Academy weekly summary:\n- Active players: {}\n- Average OVR: {}\n- High potential talents (>= 80): {}\n- Highlights: {}\n- Current ERL rank: #{} of {}\n- Quick table: {}{}",
            academy_players.len(),
            avg_ovr,
            high_potential,
            top_labels,
            academy_position,
            league_rows.len(),
            table_preview,
            recommendation
        ),
        "Academy Coordinator".to_string(),
        today.to_string(),
    )
    .with_category(MessageCategory::ScoutReport)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Academy Coordinator")
    .with_i18n(
        "be.msg.academyWeekly.subject",
        if promotion_ready.is_empty() {
            "be.msg.academyWeekly.body"
        } else {
            "be.msg.academyWeekly.bodyWithPromotion"
        },
        {
            let mut p = params(&[
                ("academy", &academy_team.name),
                ("activePlayers", &academy_players.len().to_string()),
                ("avgOvr", &avg_ovr.to_string()),
                ("highPotential", &high_potential.to_string()),
                ("highlights", &top_labels),
                ("position", &academy_position.to_string()),
                ("total", &league_rows.len().to_string()),
                ("tablePreview", &table_preview),
            ]);
            if !promotion_ready.is_empty() {
                p.insert("promotionCount".to_string(), promotion_ready.len().to_string());
                p.insert("promotionList".to_string(), promotion_ready.join(", "));
            }
            p
        },
    )
    .with_sender_i18n("be.sender.academyCoordinator", "be.role.academyCoordinator")
    .with_context(MessageContext {
        team_id: Some(parent_team.id.clone()),
        ..Default::default()
    });
    game.messages.push(message);
}

fn round_robin_pairings(team_ids: &[String], round_index: usize) -> Vec<(String, String)> {
    if team_ids.len() < 2 {
        return Vec::new();
    }

    let mut participants: Vec<Option<String>> = team_ids.iter().cloned().map(Some).collect();
    if participants.len() % 2 == 1 {
        participants.push(None);
    }

    let n = participants.len();
    let rounds = n.saturating_sub(1).max(1);
    let effective_round = round_index % rounds;

    for _ in 0..effective_round {
        let last = participants.pop().unwrap_or(None);
        participants.insert(1, last);
    }

    let mut pairings = Vec::new();
    for idx in 0..(n / 2) {
        let home = participants[idx].clone();
        let away = participants[n - 1 - idx].clone();
        if let (Some(home_id), Some(away_id)) = (home, away) {
            pairings.push((home_id, away_id));
        }
    }

    pairings
}

fn ensure_team_season_record(team: &mut Team, season: u32) -> &mut TeamSeasonRecord {
    if let Some(index) = team
        .history
        .iter()
        .position(|record| record.season == season)
    {
        return &mut team.history[index];
    }

    team.history.push(TeamSeasonRecord {
        season,
        league_position: 0,
        played: 0,
        won: 0,
        lost: 0,
        kills_for: 0,
        kills_against: 0,
    });
    let last_index = team.history.len().saturating_sub(1);
    &mut team.history[last_index]
}

fn register_parallel_result(
    team: &mut Team,
    season: u32,
    scored: u8,
    conceded: u8,
    won_series: bool,
) {
    team.form
        .push(if won_series { "W" } else { "L" }.to_string());
    if team.form.len() > 5 {
        let overflow = team.form.len() - 5;
        team.form.drain(0..overflow);
    }

    let record = ensure_team_season_record(team, season);
    record.played = record.played.saturating_add(1);
    record.kills_for = record.kills_for.saturating_add(u32::from(scored));
    record.kills_against = record.kills_against.saturating_add(u32::from(conceded));
    if won_series {
        record.won = record.won.saturating_add(1);
    } else {
        record.lost = record.lost.saturating_add(1);
    }
}

/// Simulate due fixtures for a background league (academy or competition league[1..]).
/// Updates fixture results, standings, and team form. Does NOT generate playoff fixtures,
/// player stats, messages, or news.
/// Takes separate slices for teams, players, and league (disjoint Game fields) so
/// the borrow checker permits simultaneous mutable access.
fn simulate_background_league(
    teams: &mut [Team],
    players: &[Player],
    league: &mut League,
    today: &str,
    season: u32,
) {
    let mut completed_fixtures: Vec<(String, String, u8, u8)> = Vec::new();

    // Find due fixtures
    let fixtures_to_play: Vec<(usize, String, String)> = league
        .fixtures
        .iter()
        .enumerate()
        .filter(|(_, fixture)| fixture.status == FixtureStatus::Scheduled && fixture.date == today)
        .map(|(index, fixture)| {
            (
                index,
                fixture.home_team_id.clone(),
                fixture.away_team_id.clone(),
            )
        })
        .collect();

    if fixtures_to_play.is_empty() {
        return;
    }

    // Simulate each fixture
    let mut simulated_results: Vec<(usize, String, String, u8, u8)> = Vec::new();
    let mut match_reports: Vec<crate::engine::MatchReport> = Vec::new();
    for (fixture_index, home_team_id, away_team_id) in fixtures_to_play {
        let home_data = build_engine_team_from(teams, players, &home_team_id);
        let away_data = build_engine_team_from(teams, players, &away_team_id);
        let mut rng = rand::rng();
        let report = crate::engine::simulate_lol(
            &home_data,
            &away_data,
            &crate::engine::MatchConfig::default(),
            &mut rng,
        );
        simulated_results.push((
            fixture_index,
            home_team_id,
            away_team_id,
            report.home_wins,
            report.away_wins,
        ));
        match_reports.push(report);
    }

    // Store results and update standings
    for ((fixture_index, home_team_id, away_team_id, home_wins, away_wins), _report) in
        simulated_results.iter().zip(match_reports.iter())
    {
        if let Some(fixture) = league.fixtures.get_mut(*fixture_index) {
            fixture.result = Some(MatchResult {
                home_wins: *home_wins,
                away_wins: *away_wins,
                ..MatchResult::default()
            });
            fixture.status = FixtureStatus::Completed;
        }
        completed_fixtures.push((
            home_team_id.clone(),
            away_team_id.clone(),
            *home_wins,
            *away_wins,
        ));
    }

    for (home_team_id, away_team_id, home_wins, away_wins) in &completed_fixtures {
        if let Some(home) = league
            .standings
            .iter_mut()
            .find(|entry| entry.team_id == *home_team_id)
        {
            home.record_result(*home_wins, *away_wins);
        }
        if let Some(away) = league
            .standings
            .iter_mut()
            .find(|entry| entry.team_id == *away_team_id)
        {
            away.record_result(*away_wins, *home_wins);
        }
    }

    // Update team form (no player stats, messages, news)
    for (home_team_id, away_team_id, home_wins, away_wins) in &completed_fixtures {
        let home_won = if home_wins == away_wins {
            home_team_id <= away_team_id
        } else {
            home_wins > away_wins
        };
        let away_won = !home_won;

        if let Some(home_team) = teams.iter_mut().find(|team| team.id == *home_team_id) {
            register_parallel_result(home_team, season, *home_wins, *away_wins, home_won);
        }
        if let Some(away_team) = teams.iter_mut().find(|team| team.id == *away_team_id) {
            register_parallel_result(away_team, season, *away_wins, *home_wins, away_won);
        }
    }
}

/// Simulate all background leagues (game.leagues[1..]) for today.
/// This is a no-op when there is only the active league.
/// Before simulation, runs repair_league to ensure all AI teams are match eligible.
fn process_background_leagues(game: &mut Game, today: &str) {
    // Before background simulation, ensure all AI teams are match eligible.
    // If repair fails, log the warning — the game continues without crashing.
    match crate::roster_stability::repair_league(
        game,
        crate::roster_stability::RosterStabilityReason::BackgroundSimulation,
    ) {
        Ok(reports) => {
            let total_actions: usize = reports.iter().map(|r| r.actions.len()).sum();
            if total_actions > 0 {
                info!(
                    "[turn] background simulation: repaired {} team(s) with {} action(s)",
                    reports.len(),
                    total_actions
                );
            }
        }
        Err(e) => {
            info!(
                "[turn] background simulation repair failed for team {}: {}",
                e.team_id, e
            );
        }
    }

    let season = game.clock.current_date.year() as u32;
    for i in 1..game.leagues.len() {
        let league = &mut game.leagues[i];
        simulate_background_league(&mut game.teams, &game.players, league, today, season);
    }
}

fn maybe_simulate_parallel_academy_leagues(game: &mut Game) {
    let weekday = game.clock.current_date.weekday().num_days_from_monday();
    if weekday != 0 {
        return;
    }

    let Some(parent_team_id) = game.manager.team_id.clone() else {
        return;
    };
    let Some(parent_team) = game.teams.iter().find(|team| team.id == parent_team_id) else {
        return;
    };
    let Some(academy_team_id) = parent_team.academy_team_id.clone().or_else(|| {
        game.teams
            .iter()
            .find(|team| {
                team.team_kind == TeamKind::Academy
                    && team.parent_team_id.as_deref() == Some(parent_team.id.as_str())
            })
            .map(|team| team.id.clone())
    }) else {
        return;
    };
    let Some(academy_team) = game.teams.iter().find(|team| team.id == academy_team_id) else {
        return;
    };
    let Some(metadata) = academy_team.academy.as_ref() else {
        return;
    };

    let erl_league_id = metadata.erl_assignment.erl_league_id.clone();
    let season = game.clock.current_date.year() as u32;
    let mut ordered_team_ids: Vec<String> = game
        .teams
        .iter()
        .filter(|team| {
            team.team_kind == TeamKind::Academy
                && team
                    .academy
                    .as_ref()
                    .map(|academy| academy.erl_assignment.erl_league_id.as_str())
                    == Some(erl_league_id.as_str())
        })
        .map(|team| team.id.clone())
        .collect();
    ordered_team_ids.sort();

    if ordered_team_ids.len() < 2 {
        return;
    }

    // Find existing academy league in game.leagues, or create one
    let league_pos = game.leagues.iter().position(|l| {
        l.league_kind == crate::domain::league::LeagueKind::Academy && l.id == erl_league_id
    });

    let (league_idx, should_rebuild) = match league_pos {
        Some(idx) => {
            let league = &game.leagues[idx];
            let rebuild =
                league.season != season || league.standings.len() != ordered_team_ids.len();
            (idx, rebuild)
        }
        None => {
            // Add new academy league
            let idx = game.leagues.len();
            game.leagues.push(League::new(
                erl_league_id.clone(),
                format!("{} Academy", academy_team.name),
                season,
                &ordered_team_ids,
                None,
            ));
            game.leagues[idx].league_kind = crate::domain::league::LeagueKind::Academy;
            (idx, true)
        }
    };

    if should_rebuild {
        let league = &mut game.leagues[league_idx];
        league.season = season;
        league.standings = ordered_team_ids
            .iter()
            .map(|tid| crate::domain::league::StandingEntry::new(tid.clone()))
            .collect();
        league.fixtures.clear();
        league.league_kind = crate::domain::league::LeagueKind::Academy;

        let mut start_date = game.clock.current_date;
        while start_date.weekday().num_days_from_monday() != 0 {
            start_date += chrono::Duration::days(1);
        }
        let total_rounds = ordered_team_ids.len().saturating_sub(1);
        for round in 0..total_rounds {
            let pairings = round_robin_pairings(&ordered_team_ids, round);
            let date = (start_date + chrono::Duration::days((round as i64) * 7))
                .format("%Y-%m-%d")
                .to_string();
            for (idx, (home_team_id, away_team_id)) in pairings.into_iter().enumerate() {
                league.fixtures.push(Fixture {
                    id: format!("academy-{}-md{}-{}", league.id, round + 1, idx + 1),
                    matchday: (round + 1) as u32,
                    date: date.clone(),
                    home_team_id,
                    away_team_id,
                    match_type: MatchType::League,
                    best_of: 3,
                    status: FixtureStatus::Scheduled,
                    result: None,
                });
            }
        }
    }

    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let league = &mut game.leagues[league_idx];
    simulate_background_league(&mut game.teams, &mut game.players, league, &today, season);

    // Academy-specific: generate playoffs when regular season completes
    let regular_fixtures_total = league
        .fixtures
        .iter()
        .filter(|fixture| fixture.match_type == MatchType::League)
        .count();
    let regular_completed = league
        .fixtures
        .iter()
        .filter(|fixture| fixture.status == FixtureStatus::Completed)
        .count();
    if regular_completed >= regular_fixtures_total && regular_fixtures_total > 0 {
        let has_playoffs = league
            .fixtures
            .iter()
            .any(|fixture| fixture.match_type == MatchType::Playoffs);
        if !has_playoffs {
            let mut sorted = league.standings.clone();
            sorted.sort_by(|a, b| {
                b.points
                    .cmp(&a.points)
                    .then(
                        (b.maps_won as i32 - b.maps_lost as i32)
                            .cmp(&(a.maps_won as i32 - a.maps_lost as i32)),
                    )
                    .then(b.maps_won.cmp(&a.maps_won))
            });
            if sorted.len() >= 4 {
                let next_matchday = league
                    .fixtures
                    .iter()
                    .map(|fixture| fixture.matchday)
                    .max()
                    .unwrap_or(0)
                    + 1;
                let semis_date = game.clock.current_date + chrono::Duration::days(7);
                let final_date = game.clock.current_date + chrono::Duration::days(14);
                let semifinal_pairings = vec![
                    (sorted[0].team_id.clone(), sorted[3].team_id.clone()),
                    (sorted[1].team_id.clone(), sorted[2].team_id.clone()),
                ];
                for (idx, (home_team_id, away_team_id)) in
                    semifinal_pairings.into_iter().enumerate()
                {
                    league.fixtures.push(Fixture {
                        id: format!("academy-{}-po-semi-{}", league.id, idx + 1),
                        matchday: next_matchday,
                        date: semis_date.format("%Y-%m-%d").to_string(),
                        home_team_id,
                        away_team_id,
                        match_type: MatchType::Playoffs,
                        best_of: 5,
                        status: FixtureStatus::Scheduled,
                        result: None,
                    });
                }
                league.fixtures.push(Fixture {
                    id: format!("academy-{}-po-final", league.id),
                    matchday: next_matchday + 1,
                    date: final_date.format("%Y-%m-%d").to_string(),
                    home_team_id: sorted[0].team_id.clone(),
                    away_team_id: sorted[1].team_id.clone(),
                    match_type: MatchType::Playoffs,
                    best_of: 5,
                    status: FixtureStatus::Scheduled,
                    result: None,
                });
            }
        }
    }
}

fn maybe_schedule_playoffs(game: &mut Game) {
    let Some(league) = game.leagues.first_mut() else {
        return;
    };

    let split_name = league
        .competition_id
        .as_ref()
        .and_then(|cid| game.competition_configs.get(cid))
        .and_then(|manifest| manifest.schedule.splits.get(league.split_index))
        .map(|s| s.name.as_str());
    let split = split_name
        .and_then(schedule::parse_lec_split)
        .unwrap_or(schedule::LecSplit::Summer);

    let playoff_fixtures_exist = league
        .fixtures
        .iter()
        .any(|fixture| fixture.match_type == MatchType::Playoffs);

    if !playoff_fixtures_exist {
        if !regular_season_complete(league) {
            return;
        }

        let sorted = league.sorted_standings();
        let required_seeds = match split {
            schedule::LecSplit::Winter => 8,
            schedule::LecSplit::Spring | schedule::LecSplit::Summer => 6,
        };
        if sorted.len() < required_seeds {
            return;
        }

        let seeded_team_ids: Vec<String> = sorted
            .iter()
            .take(required_seeds)
            .map(|entry| entry.team_id.clone())
            .collect();

        let start_date = game.clock.current_date + chrono::Duration::days(1);
        let start_matchday = league
            .fixtures
            .iter()
            .map(|fixture| fixture.matchday)
            .max()
            .unwrap_or(0)
            + 1;

        let opening_pairings = match split {
            schedule::LecSplit::Winter => vec![
                (seeded_team_ids[0].clone(), seeded_team_ids[7].clone()),
                (seeded_team_ids[3].clone(), seeded_team_ids[4].clone()),
                (seeded_team_ids[1].clone(), seeded_team_ids[6].clone()),
                (seeded_team_ids[2].clone(), seeded_team_ids[5].clone()),
            ],
            schedule::LecSplit::Spring | schedule::LecSplit::Summer => vec![
                (seeded_team_ids[0].clone(), seeded_team_ids[3].clone()),
                (seeded_team_ids[1].clone(), seeded_team_ids[2].clone()),
            ],
        };

        let best_of = schedule::playoff_best_of(split, false);
        let opening_round = build_playoff_round_fixtures(
            start_matchday,
            start_date.format("%Y-%m-%d").to_string(),
            opening_pairings,
            best_of,
        );
        schedule::append_fixtures(league, opening_round);
        return;
    }

    let has_pending_playoffs = league.fixtures.iter().any(|fixture| {
        fixture.match_type == MatchType::Playoffs && fixture.status != FixtureStatus::Completed
    });
    if has_pending_playoffs {
        return;
    }

    let required_seeds = match split {
        schedule::LecSplit::Winter => 8,
        schedule::LecSplit::Spring | schedule::LecSplit::Summer => 6,
    };
    let seeded_team_ids: Vec<String> = league
        .sorted_standings()
        .iter()
        .take(required_seeds)
        .map(|entry| entry.team_id.clone())
        .collect();
    if seeded_team_ids.len() < required_seeds {
        return;
    }

    let next_matchday = league
        .fixtures
        .iter()
        .filter(|fixture| fixture.match_type == MatchType::Playoffs)
        .map(|fixture| fixture.matchday)
        .max()
        .unwrap_or(0)
        + 1;
    let next_date = (game.clock.current_date + chrono::Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();

    let next_pairings = match split {
        schedule::LecSplit::Winter => next_winter_playoff_pairings(league, &seeded_team_ids),
        schedule::LecSplit::Spring | schedule::LecSplit::Summer => {
            next_spring_summer_playoff_pairings(league, &seeded_team_ids)
        }
    };

    let Some((pairings, is_grand_final)) = next_pairings else {
        return;
    };

    let best_of = schedule::playoff_best_of(split, is_grand_final);
    let fixtures = build_playoff_round_fixtures(next_matchday, next_date, pairings, best_of);
    if fixtures.is_empty() {
        return;
    }

    schedule::append_fixtures(league, fixtures);
}

fn regular_season_complete(league: &League) -> bool {
    end_of_season::season_has_started(league)
        && end_of_season::has_full_schedule(league)
        && league
            .fixtures
            .iter()
            .filter(|fixture| fixture.counts_for_league_standings())
            .all(|fixture| fixture.status == FixtureStatus::Completed)
}

fn build_playoff_round_fixtures(
    matchday: u32,
    date: String,
    pairings: Vec<(String, String)>,
    best_of: u8,
) -> Vec<Fixture> {
    pairings
        .into_iter()
        .map(|(home_team_id, away_team_id)| Fixture {
            id: Uuid::new_v4().to_string(),
            matchday,
            date: date.clone(),
            home_team_id,
            away_team_id,
            match_type: MatchType::Playoffs,
            best_of,
            status: FixtureStatus::Scheduled,
            result: None,
        })
        .collect()
}

fn playoff_round_fixtures(league: &League, round: u32) -> Vec<&Fixture> {
    let Some(start_matchday) = league
        .fixtures
        .iter()
        .filter(|fixture| fixture.match_type == MatchType::Playoffs)
        .map(|fixture| fixture.matchday)
        .min()
    else {
        return Vec::new();
    };

    let target_matchday = start_matchday + round.saturating_sub(1);
    league
        .fixtures
        .iter()
        .filter(|fixture| {
            fixture.match_type == MatchType::Playoffs && fixture.matchday == target_matchday
        })
        .collect()
}

fn fixture_winner_loser(fixture: &Fixture) -> Option<(String, String)> {
    let result: &MatchResult = fixture.result.as_ref()?;
    if result.home_wins > result.away_wins {
        Some((fixture.home_team_id.clone(), fixture.away_team_id.clone()))
    } else if result.away_wins > result.home_wins {
        Some((fixture.away_team_id.clone(), fixture.home_team_id.clone()))
    } else {
        None
    }
}

fn outcome_for_pair(fixtures: &[&Fixture], a: &str, b: &str) -> Option<(String, String)> {
    fixtures
        .iter()
        .find(|fixture| {
            (fixture.home_team_id == a && fixture.away_team_id == b)
                || (fixture.home_team_id == b && fixture.away_team_id == a)
        })
        .and_then(|fixture| fixture_winner_loser(fixture))
}

fn next_spring_summer_playoff_pairings(
    league: &League,
    seeds: &[String],
) -> Option<(Vec<(String, String)>, bool)> {
    if seeds.len() < 6 {
        return None;
    }

    let r1 = playoff_round_fixtures(league, 1);
    let r2 = playoff_round_fixtures(league, 2);
    let r3 = playoff_round_fixtures(league, 3);
    let r4 = playoff_round_fixtures(league, 4);
    let r5 = playoff_round_fixtures(league, 5);
    let r6 = playoff_round_fixtures(league, 6);

    if r1.is_empty() {
        return None;
    }
    if r2.is_empty() {
        let (_w1, l1) = outcome_for_pair(&r1, &seeds[0], &seeds[3])?;
        let (_w2, l2) = outcome_for_pair(&r1, &seeds[1], &seeds[2])?;
        return Some((vec![(l1, seeds[5].clone()), (l2, seeds[4].clone())], false));
    }
    if r3.is_empty() {
        let (w1, _l1) = outcome_for_pair(&r1, &seeds[0], &seeds[3])?;
        let (w2, _l2) = outcome_for_pair(&r1, &seeds[1], &seeds[2])?;
        return Some((vec![(w1, w2)], false));
    }
    if r4.is_empty() {
        let (w_r2_a, _l_r2_a) = outcome_for_pair(&r2, &seeds[5], &seeds[0])
            .or_else(|| outcome_for_pair(&r2, &seeds[5], &seeds[3]))?;
        let (w_r2_b, _l_r2_b) = outcome_for_pair(&r2, &seeds[4], &seeds[1])
            .or_else(|| outcome_for_pair(&r2, &seeds[4], &seeds[2]))?;
        return Some((vec![(w_r2_a, w_r2_b)], false));
    }
    if r5.is_empty() {
        let (_w_r3, l_r3) = fixture_winner_loser(*r3.first()?)?;
        let (w_r4, _l_r4) = fixture_winner_loser(*r4.first()?)?;
        return Some((vec![(l_r3, w_r4)], false));
    }
    if r6.is_empty() {
        let (w_r3, _l_r3) = fixture_winner_loser(*r3.first()?)?;
        let (w_r5, _l_r5) = fixture_winner_loser(*r5.first()?)?;
        return Some((vec![(w_r3, w_r5)], true));
    }

    None
}

fn next_winter_playoff_pairings(
    league: &League,
    seeds: &[String],
) -> Option<(Vec<(String, String)>, bool)> {
    if seeds.len() < 8 {
        return None;
    }

    let r1 = playoff_round_fixtures(league, 1);
    let r2 = playoff_round_fixtures(league, 2);
    let r3 = playoff_round_fixtures(league, 3);
    let r4 = playoff_round_fixtures(league, 4);
    let r5 = playoff_round_fixtures(league, 5);
    let r6 = playoff_round_fixtures(league, 6);
    let r7 = playoff_round_fixtures(league, 7);
    let r8 = playoff_round_fixtures(league, 8);

    if r1.is_empty() {
        return None;
    }

    let (w1, l1) = outcome_for_pair(&r1, &seeds[0], &seeds[7])?;
    let (w2, l2) = outcome_for_pair(&r1, &seeds[3], &seeds[4])?;
    let (w3, l3) = outcome_for_pair(&r1, &seeds[1], &seeds[6])?;
    let (w4, l4) = outcome_for_pair(&r1, &seeds[2], &seeds[5])?;

    if r2.is_empty() {
        return Some((
            vec![(l1.clone(), l2.clone()), (l3.clone(), l4.clone())],
            false,
        ));
    }
    if r3.is_empty() {
        return Some((
            vec![(w1.clone(), w2.clone()), (w3.clone(), w4.clone())],
            false,
        ));
    }

    let (wlb1_a, _llb1_a) = outcome_for_pair(&r2, &l1, &l2)?;
    let (wlb1_b, _llb1_b) = outcome_for_pair(&r2, &l3, &l4)?;
    let (wwb2_a, lwb2_a) = outcome_for_pair(&r3, &w1, &w2)?;
    let (wwb2_b, lwb2_b) = outcome_for_pair(&r3, &w3, &w4)?;

    if r4.is_empty() {
        return Some((
            vec![
                (wlb1_a.clone(), lwb2_a.clone()),
                (wlb1_b.clone(), lwb2_b.clone()),
            ],
            false,
        ));
    }
    if r5.is_empty() {
        return Some((vec![(wwb2_a.clone(), wwb2_b.clone())], false));
    }

    let (wlb2_a, _llb2_a) = outcome_for_pair(&r4, &wlb1_a, &lwb2_a)?;
    let (wlb2_b, _llb2_b) = outcome_for_pair(&r4, &wlb1_b, &lwb2_b)?;
    let (wwbf, lwbf) = outcome_for_pair(&r5, &wwb2_a, &wwb2_b)?;

    if r6.is_empty() {
        return Some((vec![(wlb2_a.clone(), wlb2_b.clone())], false));
    }

    let (wlb3, _llb3) = outcome_for_pair(&r6, &wlb2_a, &wlb2_b)?;
    if r7.is_empty() {
        return Some((vec![(lwbf.clone(), wlb3.clone())], false));
    }

    let (wlb_final, _llb_final) = outcome_for_pair(&r7, &lwbf, &wlb3)?;
    if r8.is_empty() {
        return Some((vec![(wwbf, wlb_final)], true));
    }

    None
}

// ---------------------------------------------------------------------------
// Matchday simulation using the engine crate
// ---------------------------------------------------------------------------

fn simulate_matchday_with_capture<F>(game: &mut Game, today: &str, on_capture: &mut F)
where
    F: FnMut(StatsState),
{
    info!("[turn] simulate_matchday: {}", today);
    simulate_other_matches_with_capture(game, today, None, on_capture);
    generate_matchday_news(game, today);
}

/// Simulate all scheduled matches for `today`, optionally skipping one fixture
/// (the user's live match). Called by both process_day and advance_time_with_mode.
pub fn simulate_other_matches(game: &mut Game, today: &str, skip_fixture: Option<usize>) {
    simulate_other_matches_with_capture(game, today, skip_fixture, &mut |_| {});
}

pub fn simulate_other_matches_with_capture<F>(
    game: &mut Game,
    today: &str,
    skip_fixture: Option<usize>,
    on_capture: &mut F,
) where
    F: FnMut(StatsState),
{
    debug!(
        "[turn] simulate_other_matches: date={}, skip={:?}",
        today, skip_fixture
    );
    let fixture_indices: Vec<usize> = game.active_league().map_or(vec![], |league| {
        league
            .fixtures
            .iter()
            .enumerate()
            .filter(|(i, f)| {
                f.date == today
                    && f.status == FixtureStatus::Scheduled
                    && (skip_fixture != Some(*i))
            })
            .map(|(i, _)| i)
            .collect()
    });

    for idx in fixture_indices {
        simulate_single_match_with_capture(game, idx, on_capture);
    }
}

fn simulate_single_match_with_capture<F>(game: &mut Game, idx: usize, on_capture: &mut F)
where
    F: FnMut(StatsState),
{
    let (home_team_id, away_team_id, best_of) = {
        let f = &game.active_league().unwrap().fixtures[idx];
        (f.home_team_id.clone(), f.away_team_id.clone(), f.best_of)
    };

    let home_name = game
        .teams
        .iter()
        .find(|t| t.id == home_team_id)
        .map(|t| t.name.clone())
        .unwrap_or_else(|| "?".to_string());
    let away_name = game
        .teams
        .iter()
        .find(|t| t.id == away_team_id)
        .map(|t| t.name.clone())
        .unwrap_or_else(|| "?".to_string());
    debug!(
        "[turn] simulate_single_match: {} vs {} (fixture #{})",
        home_name, away_name, idx
    );

    // Pre-match: ensure both teams are match eligible before engine team building.
    // For user teams, repair_team is a no-op (skips non-schedulable teams).
    for team_id in [&home_team_id, &away_team_id] {
        if let Err(e) = crate::roster_stability::repair_team(
            game,
            team_id,
            crate::roster_stability::RosterStabilityReason::PreMatch,
        ) {
            info!("[turn] pre-match repair failed for team {team_id}: {e}");
        }
    }

    let draft = resolve_auto_draft(game, &home_team_id, &away_team_id);
    let home_data = build_engine_team(game, &home_team_id);
    let away_data = build_engine_team(game, &away_team_id);
    let config = crate::engine::MatchConfig {
        home_draft_modifier: draft.home_modifier,
        away_draft_modifier: draft.away_modifier,
        ..Default::default()
    };
    let report = if best_of <= 1 {
        let mut rng = rand::rng();
        crate::engine::simulate_lol(&home_data, &away_data, &config, &mut rng)
    } else {
        simulate_series(&home_data, &away_data, &config, best_of)
    };

    info!(
        "[turn] match result: {} {} - {} {} (fixture #{})",
        home_name, report.home_wins, report.away_wins, away_name, idx
    );

    let mastery_picks = draft.picks.clone();
    let winner_team_id = if report.home_wins == report.away_wins {
        if home_team_id <= away_team_id {
            home_team_id.clone()
        } else {
            away_team_id.clone()
        }
    } else if report.home_wins > report.away_wins {
        home_team_id.clone()
    } else {
        away_team_id.clone()
    };
    if !mastery_picks.is_empty() {
        champions::apply_match_mastery_progress(game, &winner_team_id, &mastery_picks);
    }

    post_match::apply_match_report_with_draft_capture(
        game,
        idx,
        &home_team_id,
        &away_team_id,
        &report,
        &draft.picks,
        &draft.bans,
        on_capture,
    );
}

struct AutoDraft {
    picks: Vec<(String, String)>,
    bans: Vec<String>,
    home_modifier: f64,
    away_modifier: f64,
}

fn resolve_auto_draft(game: &Game, home_team_id: &str, away_team_id: &str) -> AutoDraft {
    use crate::draft::{ChampionProfile, SkillDemandProfile, select_pick};
    use std::collections::HashSet;
    let profiles: Vec<ChampionProfile> = game
        .champion_patch
        .hidden_meta
        .iter()
        .filter_map(|meta| {
            let role = match meta.role.to_ascii_lowercase().as_str() {
                "top" => crate::domain::player::LolRole::Top,
                "jungle" | "jgl" => crate::domain::player::LolRole::Jungle,
                "mid" => crate::domain::player::LolRole::Mid,
                "adc" | "bot" => crate::domain::player::LolRole::Adc,
                "support" | "sup" => crate::domain::player::LolRole::Support,
                _ => return None,
            };
            let meta_power = match meta.tier.to_ascii_uppercase().as_str() {
                "S" => 90,
                "A" => 75,
                "B" => 60,
                "C" => 45,
                "D" => 30,
                _ => 60,
            };
            Some(ChampionProfile {
                champion_id: meta.champion_id.clone(),
                role,
                meta_power,
                demands: SkillDemandProfile::for_champion(&meta.champion_id, role),
            })
        })
        .collect();
    let mut ban_candidates: Vec<_> = profiles.iter().collect();
    ban_candidates.sort_by(|left, right| {
        let threat = |profile: &&ChampionProfile| {
            let mastery = game
                .champion_masteries
                .iter()
                .filter(|entry| entry.champion_id.eq_ignore_ascii_case(&profile.champion_id))
                .map(|entry| entry.mastery)
                .max()
                .unwrap_or(25);
            u16::from(profile.meta_power) * 2 + u16::from(mastery)
        };
        threat(right)
            .cmp(&threat(left))
            .then_with(|| left.champion_id.cmp(&right.champion_id))
    });
    // Preserve two legal options per role for the two opposing starters before banning.
    let mut bans = Vec::new();
    for candidate in ban_candidates {
        if bans.len() == 10 {
            break;
        }
        let remaining_for_role = profiles
            .iter()
            .filter(|profile| {
                profile.role == candidate.role && !bans.contains(&profile.champion_id)
            })
            .count();
        if remaining_for_role > 2 {
            bans.push(candidate.champion_id.clone());
        }
    }
    let mut unavailable: HashSet<String> = bans.iter().cloned().collect();
    let mut picks = Vec::new();
    let mut modifiers = [0.0; 2];
    for (team_index, team_id) in [home_team_id, away_team_id].iter().enumerate() {
        let team = game.teams.iter().find(|team| team.id == **team_id);
        let mut ids = team
            .map(|team| team.active_lineup_ids.clone())
            .unwrap_or_default();
        ids.retain(|id| {
            game.players
                .iter()
                .any(|player| player.id == *id && player.team_id.as_deref() == Some(*team_id))
        });
        if ids.len() < 5 {
            let mut fallback_ids: Vec<String> = game
                .players
                .iter()
                .filter(|player| player.team_id.as_deref() == Some(*team_id))
                .map(|player| player.id.clone())
                .collect();
            fallback_ids.sort();
            for player_id in fallback_ids {
                if !ids.contains(&player_id) {
                    ids.push(player_id);
                }
                if ids.len() == 5 {
                    break;
                }
            }
        }
        for player_id in ids.into_iter().take(5) {
            let Some(player) = game.players.iter().find(|player| player.id == player_id) else {
                continue;
            };
            let candidates: Vec<ChampionProfile> = profiles
                .iter()
                .filter(|profile| profile.role == player.natural_position)
                .cloned()
                .collect();
            let fallback: Vec<ChampionProfile> = game
                .champion_masteries
                .iter()
                .filter(|entry| entry.player_id == player.id)
                .map(|entry| ChampionProfile {
                    champion_id: entry.champion_id.clone(),
                    role: player.natural_position,
                    meta_power: 60,
                    demands: SkillDemandProfile::for_champion(
                        &entry.champion_id,
                        player.natural_position,
                    ),
                })
                .chain(
                    crate::champions::training_targets_for_player(player)
                        .into_iter()
                        .filter(|id| !id.trim().is_empty())
                        .map(|champion_id| ChampionProfile {
                            demands: SkillDemandProfile::for_champion(
                                &champion_id,
                                player.natural_position,
                            ),
                            champion_id,
                            role: player.natural_position,
                            meta_power: 60,
                        }),
                )
                .collect();
            let candidate_set = if candidates.is_empty() {
                &fallback
            } else {
                &candidates
            };
            let mastery: Vec<(String, u8)> = game
                .champion_masteries
                .iter()
                .filter(|entry| entry.player_id == player.id)
                .map(|entry| (entry.champion_id.clone(), entry.mastery))
                .collect();
            if let Some(evaluation) =
                select_pick(&player.attributes, &mastery, candidate_set, &unavailable)
            {
                unavailable.insert(evaluation.champion_id.clone());
                modifiers[team_index] += evaluation.engine_modifier;
                picks.push((player.id.clone(), evaluation.champion_id));
            }
        }
    }
    AutoDraft {
        picks,
        bans,
        home_modifier: (modifiers[0] / 5.0).clamp(-0.035, 0.035),
        away_modifier: (modifiers[1] / 5.0).clamp(-0.035, 0.035),
    }
}

fn simulate_series(
    home_data: &crate::engine::TeamData,
    away_data: &crate::engine::TeamData,
    config: &crate::engine::MatchConfig,
    best_of: u8,
) -> crate::engine::MatchReport {
    let mut rng = rand::rng();
    let target_wins = (best_of / 2) + 1;
    let mut home_wins = 0_u8;
    let mut away_wins = 0_u8;
    let mut reports: Vec<crate::engine::MatchReport> = Vec::new();

    while home_wins < target_wins && away_wins < target_wins {
        let report = crate::engine::simulate_lol(home_data, away_data, config, &mut rng);
        home_wins = home_wins.saturating_add(report.home_wins);
        away_wins = away_wins.saturating_add(report.away_wins);
        reports.push(report);
    }

    let mut merged = match reports.last() {
        Some(report) => report.clone(),
        None => crate::engine::simulate_lol(home_data, away_data, config, &mut rng),
    };
    merged.home_wins = home_wins;
    merged.away_wins = away_wins;

    merged.home_stats = crate::engine::TeamStats::default();
    merged.away_stats = crate::engine::TeamStats::default();
    merged.events.clear();
    merged.kill_feed.clear();
    merged.player_stats = HashMap::new();
    merged.game_duration_seconds = 0;

    let mut possession_sum = 0.0_f64;
    for report in reports {
        merged.home_stats.kills = merged
            .home_stats
            .kills
            .saturating_add(report.home_stats.kills);
        merged.home_stats.deaths = merged
            .home_stats
            .deaths
            .saturating_add(report.home_stats.deaths);
        merged.home_stats.gold_earned = merged
            .home_stats
            .gold_earned
            .saturating_add(report.home_stats.gold_earned);
        merged.home_stats.damage_dealt = merged
            .home_stats
            .damage_dealt
            .saturating_add(report.home_stats.damage_dealt);
        merged.home_stats.objectives = merged
            .home_stats
            .objectives
            .saturating_add(report.home_stats.objectives);
        merged.home_stats.possession_ticks = merged
            .home_stats
            .possession_ticks
            .saturating_add(report.home_stats.possession_ticks);

        merged.away_stats.kills = merged
            .away_stats
            .kills
            .saturating_add(report.away_stats.kills);
        merged.away_stats.deaths = merged
            .away_stats
            .deaths
            .saturating_add(report.away_stats.deaths);
        merged.away_stats.gold_earned = merged
            .away_stats
            .gold_earned
            .saturating_add(report.away_stats.gold_earned);
        merged.away_stats.damage_dealt = merged
            .away_stats
            .damage_dealt
            .saturating_add(report.away_stats.damage_dealt);
        merged.away_stats.objectives = merged
            .away_stats
            .objectives
            .saturating_add(report.away_stats.objectives);
        merged.away_stats.possession_ticks = merged
            .away_stats
            .possession_ticks
            .saturating_add(report.away_stats.possession_ticks);

        merged.events.extend(report.events);
        merged.kill_feed.extend(report.kill_feed);
        merged.game_duration_seconds = merged
            .game_duration_seconds
            .saturating_add(report.game_duration_seconds);
        possession_sum += report.home_possession;

        for (player_id, stats) in report.player_stats {
            let entry = merged.player_stats.entry(player_id).or_default();
            if entry.role.is_none() {
                entry.role = stats.role;
            }
            entry.duration_seconds = entry
                .duration_seconds
                .saturating_add(stats.duration_seconds);
            entry.kills = entry.kills.saturating_add(stats.kills);
            entry.deaths = entry.deaths.saturating_add(stats.deaths);
            entry.assists = entry.assists.saturating_add(stats.assists);
            entry.creep_score = entry.creep_score.saturating_add(stats.creep_score);
            entry.gold_earned = entry.gold_earned.saturating_add(stats.gold_earned);
            entry.damage_dealt = entry.damage_dealt.saturating_add(stats.damage_dealt);
            entry.vision_score = entry.vision_score.saturating_add(stats.vision_score);
            entry.wards_placed = entry.wards_placed.saturating_add(stats.wards_placed);
        }
    }

    if home_wins + away_wins > 0 {
        merged.home_possession = possession_sum / f64::from(home_wins + away_wins);
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::domain::player::{Player, PlayerAttributes};
    use crate::domain::stats::LolRole;
    use chrono::{TimeZone, Utc};

    fn make_player(id: &str, team_id: &str, overall: u8) -> Player {
        let attrs = PlayerAttributes {
            mechanics: overall,
            laning: overall,
            teamfighting: overall,
            macro_play: overall,
            consistency: overall,
            shotcalling: overall,
            champion_pool: overall,
            discipline: overall,
            mental_resilience: overall,
        };
        let mut player = Player::new(
            id.to_string(),
            id.to_string(),
            format!("Full {id}"),
            "2000-01-01".to_string(),
            "ES".to_string(),
            LolRole::Mid,
            attrs,
        );
        player.team_id = Some(team_id.to_string());
        player.condition = 100;
        player
    }

    fn make_team(id: &str) -> Team {
        let team = Team::new(
            id.to_string(),
            format!("Team {id}"),
            id.to_string(),
            "ES".to_string(),
            "Test League".to_string(),
            "Test Arena".to_string(),
            1000,
        );
        team
    }

    /// Create a minimal Game with 2 teams (5 players each) and a league with 1 fixture.
    fn bg_test_game(today: &str) -> (Game, String) {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap());
        let manager = crate::domain::manager::Manager::new(
            "mgr".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "ES".to_string(),
        );

        let teams = vec![make_team("team1"), make_team("team2")];
        let mut players = Vec::new();
        for tid in &["team1", "team2"] {
            for i in 0..5 {
                players.push(make_player(&format!("{}-p{}", tid, i), tid, 60 + i as u8));
            }
        }

        let mut game = Game::new(clock, manager, teams, players, vec![], vec![]);

        let league = League::new(
            "bg-league".to_string(),
            "BG League".to_string(),
            2025,
            &["team1".to_string(), "team2".to_string()],
            None,
        );
        game.leagues = vec![league];

        // Create a fixture dated today
        if let Some(league) = game.active_league_mut() {
            league.fixtures.push(Fixture {
                id: "fix-1".to_string(),
                matchday: 1,
                date: today.to_string(),
                home_team_id: "team1".to_string(),
                away_team_id: "team2".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Scheduled,
                result: None,
            });
        }

        (game, today.to_string())
    }

    #[test]
    fn auto_draft_is_pre_match_legal_and_captured_in_stats() {
        let (mut game, _) = bg_test_game("2025-06-15");
        let roles = [
            LolRole::Top,
            LolRole::Jungle,
            LolRole::Mid,
            LolRole::Adc,
            LolRole::Support,
        ];
        for (index, player) in game.players.iter_mut().enumerate() {
            player.natural_position = roles[index % roles.len()];
            player.position = player.natural_position;
        }
        game.champion_patch.hidden_meta = roles
            .iter()
            .flat_map(|role| {
                let name = format!("{:?}", role).to_ascii_lowercase();
                (0..4).map(move |index| crate::champions::ChampionMetaEntry {
                    champion_id: format!("{name}-{index}"),
                    role: name.clone(),
                    tier: if index == 0 { "S" } else { "A" }.to_string(),
                })
            })
            .collect();
        for team in &mut game.teams {
            team.active_lineup_ids = game
                .players
                .iter()
                .filter(|player| player.team_id.as_deref() == Some(team.id.as_str()))
                .take(5)
                .map(|player| player.id.clone())
                .collect();
        }
        let draft = resolve_auto_draft(&game, "team1", "team2");
        assert_eq!(draft.picks.len(), 10);
        assert_eq!(draft.bans.len(), 10);
        assert!(
            draft
                .bans
                .iter()
                .all(|ban| !draft.picks.iter().any(|(_, pick)| pick == ban))
        );
        assert_eq!(
            draft
                .picks
                .iter()
                .map(|(_, champion)| champion)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            10
        );
        let mut capture = StatsState::default();
        simulate_single_match_with_capture(&mut game, 0, &mut |stats| capture = stats);
        assert_eq!(capture.player_matches.len(), 10);
        assert!(
            capture
                .player_matches
                .iter()
                .all(|record| record.champion.is_some())
        );
        assert!(
            capture
                .player_matches
                .iter()
                .all(|record| record.bans_json.contains("top-"))
        );
    }

    // -----------------------------------------------------------------------
    // T9: simulate_background_league tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_bg_league_simulates_due_fixture() {
        let today = "2025-06-15";
        let (mut game, today_str) = bg_test_game(today);
        let season = 2025;

        let league = game.leagues.first_mut().unwrap();
        simulate_background_league(
            &mut game.teams,
            &mut game.players,
            league,
            &today_str,
            season,
        );

        // The fixture should now be Completed with a result
        let fixture = &league.fixtures[0];
        assert_eq!(fixture.status, FixtureStatus::Completed);
        assert!(fixture.result.is_some());
        let result = fixture.result.as_ref().unwrap();
        // One team must have won (home_wins + away_wins >= 1 in a bo1)
        assert!(result.home_wins >= 1 || result.away_wins >= 1);
    }

    #[test]
    fn test_bg_league_updates_standings() {
        let today = "2025-06-15";
        let (mut game, today_str) = bg_test_game(today);
        let season = 2025;

        let league = game.leagues.first_mut().unwrap();
        simulate_background_league(
            &mut game.teams,
            &mut game.players,
            league,
            &today_str,
            season,
        );

        // Both teams should have played=1
        let home_entry = league
            .standings
            .iter()
            .find(|e| e.team_id == "team1")
            .unwrap();
        let away_entry = league
            .standings
            .iter()
            .find(|e| e.team_id == "team2")
            .unwrap();

        assert_eq!(home_entry.played, 1);
        assert_eq!(away_entry.played, 1);

        // Total maps_won + maps_lost for both teams should equal total maps played
        let total_maps = home_entry.maps_won + home_entry.maps_lost;
        assert_eq!(total_maps, 1);
        let total_maps_away = away_entry.maps_won + away_entry.maps_lost;
        assert_eq!(total_maps_away, 1);
    }

    #[test]
    fn test_bg_league_updates_team_form() {
        let today = "2025-06-15";
        let (mut game, today_str) = bg_test_game(today);
        let season = 2025;

        let league = game.leagues.first_mut().unwrap();
        simulate_background_league(&mut game.teams, &game.players, league, &today_str, season);

        // Both teams should have form entries (W or L)
        let team1 = game.teams.iter().find(|t| t.id == "team1").unwrap();
        let team2 = game.teams.iter().find(|t| t.id == "team2").unwrap();
        assert_eq!(team1.form.len(), 1);
        assert_eq!(team2.form.len(), 1);
        // One team won, one lost
        let all_forms = [team1.form[0].as_str(), team2.form[0].as_str()];
        assert!(all_forms.contains(&"W"));
        assert!(all_forms.contains(&"L"));
    }

    #[test]
    fn test_bg_league_no_player_stats_modified() {
        let today = "2025-06-15";
        let (mut game, today_str) = bg_test_game(today);
        let season = 2025;

        // Capture player stats before
        let before: Vec<_> = game
            .players
            .iter()
            .map(|p| (p.id.clone(), p.stats.clone()))
            .collect();

        let league = game.leagues.first_mut().unwrap();
        simulate_background_league(&mut game.teams, &game.players, league, &today_str, season);

        // Player stats should be unchanged
        for (id, before_stats) in &before {
            let after = game.players.iter().find(|p| &p.id == id).unwrap();
            assert_eq!(
                format!("{:?}", before_stats),
                format!("{:?}", after.stats),
                "player {} stats changed",
                id
            );
        }
    }

    #[test]
    fn test_bg_league_no_side_effects() {
        let today = "2025-06-15";
        let (mut game, today_str) = bg_test_game(today);
        let season = 2025;

        let before_msgs = game.messages.len();
        let before_news = game.news.len();

        let league = game.leagues.first_mut().unwrap();
        simulate_background_league(&mut game.teams, &game.players, league, &today_str, season);

        // No messages or news should be generated
        assert_eq!(game.messages.len(), before_msgs);
        assert_eq!(game.news.len(), before_news);
    }

    #[test]
    fn test_bg_league_no_fixtures_is_noop() {
        let today = "2025-06-15";
        let (mut game, _) = bg_test_game(today);

        // Remove the fixture — no due fixtures
        if let Some(league) = game.active_league_mut() {
            league.fixtures.clear();
        }

        let season = 2025;

        // Add a future fixture instead
        if let Some(league) = game.active_league_mut() {
            league.fixtures.push(Fixture {
                id: "fix-future".to_string(),
                matchday: 2,
                date: "2025-06-22".to_string(),
                home_team_id: "team1".to_string(),
                away_team_id: "team2".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Scheduled,
                result: None,
            });
        }

        let league = game.leagues.first_mut().unwrap();
        simulate_background_league(&mut game.teams, &game.players, league, "2025-06-15", season);

        // Fixture should still be Scheduled
        let fixture = &game.leagues[0].fixtures[0];
        assert_eq!(fixture.status, FixtureStatus::Scheduled);
        assert!(fixture.result.is_none());
    }

    // -----------------------------------------------------------------------
    // T10: process_background_leagues tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_bg_loop_multiple_leagues() {
        let today = "2025-06-15";
        let (mut base_game, _) = bg_test_game(today);

        // Add two background leagues with due fixtures. process_background_leagues
        // intentionally skips index 0 because that is the user-active league.
        for index in 1..=2 {
            let mut league = League::new(
                format!("bg-league-{index}"),
                format!("BG League {index}"),
                2025,
                &["team1".to_string(), "team2".to_string()],
                None,
            );
            league.fixtures.push(Fixture {
                id: format!("fix-bg{index}"),
                matchday: 1,
                date: today.to_string(),
                home_team_id: "team1".to_string(),
                away_team_id: "team2".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Scheduled,
                result: None,
            });
            base_game.leagues.push(league);
        }

        process_background_leagues(&mut base_game, today);

        // Active league (index 0) should be unchanged
        assert_eq!(
            base_game.leagues[0].fixtures[0].status,
            FixtureStatus::Scheduled
        );
        // BG league 1 (index 1) should have simulated
        assert_eq!(
            base_game.leagues[1].fixtures[0].status,
            FixtureStatus::Completed
        );
        // BG league 2 (index 2) should have simulated
        assert_eq!(
            base_game.leagues[2].fixtures[0].status,
            FixtureStatus::Completed
        );
    }

    #[test]
    fn test_bg_loop_no_bg_leagues_is_noop() {
        let today = "2025-06-15";
        let (mut game, _) = bg_test_game(today);

        // Only 1 league — should be a no-op
        process_background_leagues(&mut game, today);

        // League still has the original fixture in Scheduled
        assert_eq!(game.leagues[0].fixtures[0].status, FixtureStatus::Scheduled);
    }

    #[test]
    fn test_bg_loop_no_due_fixtures_is_noop() {
        let today = "2025-06-15";
        let (mut game, _) = bg_test_game(today);

        // Remove fixture from index 0 to simulate no due fixtures in active
        game.leagues[0].fixtures.clear();

        // Add a bg league with no due fixture (future date)
        let bg_league = League::new(
            "bg-league".to_string(),
            "BG League".to_string(),
            2025,
            &["team1".to_string(), "team2".to_string()],
            None,
        );
        game.leagues.push(bg_league);
        if let Some(league) = game.leagues.last_mut() {
            league.fixtures.push(Fixture {
                id: "fix-future".to_string(),
                matchday: 2,
                date: "2025-06-22".to_string(),
                home_team_id: "team1".to_string(),
                away_team_id: "team2".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Scheduled,
                result: None,
            });
        }

        process_background_leagues(&mut game, today);

        // BG league fixture should still be Scheduled (no due fixtures)
        assert_eq!(game.leagues[1].fixtures[0].status, FixtureStatus::Scheduled);
    }
}
