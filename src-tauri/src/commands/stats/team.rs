use olm_core::domain::stats::{MatchOutcome, TeamMatchStatsRecord, TeamSide};
use olm_core::state::StateManager;

use super::dto::{
    TeamAdvancedMetricDto, TeamMatchHistoryEntryDto, TeamStatsOverviewDto,
    TeamStatsOverviewMetricsDto,
};
use super::shared::{calculate_average, competition_label, ensure_team_exists, opponent_name};

#[derive(Debug, Clone, Default)]
struct TeamAggregate {
    matches_played: u32,
    wins: u32,
    losses: u32,
    duration_seconds_total: u32,
    kills: u32,
    deaths: u32,
    gold_earned: u32,
    damage_dealt: u32,
    objectives: u32,
}

fn side_label(side: TeamSide) -> String {
    match side {
        TeamSide::Blue => "Blue".to_string(),
        TeamSide::Red => "Red".to_string(),
    }
}

fn outcome_label(outcome: MatchOutcome) -> String {
    match outcome {
        MatchOutcome::Win => "Win".to_string(),
        MatchOutcome::Loss => "Loss".to_string(),
    }
}

fn metric_dto(total: u32, matches_played: u32) -> TeamAdvancedMetricDto {
    TeamAdvancedMetricDto {
        total,
        per_match: calculate_average(total, matches_played),
    }
}

fn aggregate_team_history(records: &[TeamMatchStatsRecord]) -> Option<TeamAggregate> {
    if records.is_empty() {
        return None;
    }

    let mut aggregate = TeamAggregate::default();
    for record in records {
        aggregate.matches_played += 1;
        aggregate.duration_seconds_total += record.duration_seconds;
        aggregate.kills += record.kills as u32;
        aggregate.deaths += record.deaths as u32;
        aggregate.gold_earned += record.gold_earned;
        aggregate.damage_dealt += record.damage_dealt;
        aggregate.objectives += record.objectives as u32;

        match record.result {
            MatchOutcome::Win => aggregate.wins += 1,
            MatchOutcome::Loss => aggregate.losses += 1,
        }
    }

    Some(aggregate)
}

fn build_team_overview(aggregate: &TeamAggregate) -> TeamStatsOverviewDto {
    TeamStatsOverviewDto {
        matches_played: aggregate.matches_played,
        wins: aggregate.wins,
        losses: aggregate.losses,
        metrics: TeamStatsOverviewMetricsDto {
            kills: metric_dto(aggregate.kills, aggregate.matches_played),
            deaths: metric_dto(aggregate.deaths, aggregate.matches_played),
            gold_earned: metric_dto(aggregate.gold_earned, aggregate.matches_played),
            damage_to_champions: metric_dto(aggregate.damage_dealt, aggregate.matches_played),
            objectives: metric_dto(aggregate.objectives, aggregate.matches_played),
            average_game_duration_seconds: metric_dto(
                aggregate.duration_seconds_total,
                aggregate.matches_played,
            ),
        },
    }
}

fn to_team_history_dto(
    state: &StateManager,
    record: &TeamMatchStatsRecord,
) -> TeamMatchHistoryEntryDto {
    TeamMatchHistoryEntryDto {
        fixture_id: record.fixture_id.clone(),
        date: record.date.clone(),
        competition: competition_label(&record.match_type),
        matchday: record.matchday,
        opponent_team_id: record.opponent_team_id.clone(),
        opponent_name: opponent_name(state, &record.opponent_team_id),
        side: side_label(record.side),
        result: outcome_label(record.result),
        game_duration_seconds: record.duration_seconds,
        kills: record.kills,
        deaths: record.deaths,
        gold_earned: record.gold_earned,
        damage_to_champions: record.damage_dealt,
        objectives: record.objectives,
    }
}

pub(super) fn get_team_stats_overview_internal(
    state: &StateManager,
    team_id: &str,
) -> Result<Option<TeamStatsOverviewDto>, String> {
    ensure_team_exists(state, team_id)?;

    let Some(records) = state.get_stats_state(|stats| {
        stats
            .team_matches
            .iter()
            .filter(|record| record.team_id == team_id)
            .cloned()
            .collect::<Vec<_>>()
    }) else {
        return Ok(None);
    };

    Ok(aggregate_team_history(&records).map(|aggregate| build_team_overview(&aggregate)))
}

pub(super) fn get_team_match_history_internal(
    state: &StateManager,
    team_id: &str,
    limit: Option<usize>,
) -> Result<Vec<TeamMatchHistoryEntryDto>, String> {
    ensure_team_exists(state, team_id)?;

    let Some(mut history) = state.get_stats_state(|stats| {
        stats
            .team_matches
            .iter()
            .filter(|record| record.team_id == team_id)
            .cloned()
            .collect::<Vec<_>>()
    }) else {
        return Ok(Vec::new());
    };

    history.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then(right.matchday.cmp(&left.matchday))
            .then(right.fixture_id.cmp(&left.fixture_id))
    });

    let limit = limit.unwrap_or(5);
    Ok(history
        .into_iter()
        .take(limit)
        .map(|record| to_team_history_dto(state, &record))
        .collect())
}
