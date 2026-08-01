use std::collections::HashMap;

use olm_core::domain::player::Player;
use olm_core::domain::stats::{LolRole, MatchOutcome, PlayerMatchStatsRecord, TeamSide};
use olm_core::state::StateManager;

use super::dto::{
    PlayerAdvancedMetricDto, PlayerMatchHistoryEntryDto, PlayerStatsOverviewDto,
    PlayerStatsOverviewMetricsDto,
};
use super::shared::{calculate_per_match, competition_label, opponent_name, percentile_rank};

const DEFAULT_MINIMUM_MATCHES: u32 = 3;
const DEFAULT_MINIMUM_COHORT_SIZE: usize = 3;

#[derive(Debug, Clone, Default)]
struct PlayerAggregate {
    matches_played: u32,
    kills: u32,
    deaths: u32,
    assists: u32,
    creep_score: u32,
    vision_score: u32,
    wards_placed: u32,
}

fn position_key(player: &Player) -> String {
    format!("{:?}", player.natural_position)
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

fn role_label(role: LolRole) -> String {
    match role {
        LolRole::Top => "Top".to_string(),
        LolRole::Jungle => "Jungle".to_string(),
        LolRole::Mid => "Mid".to_string(),
        LolRole::Adc => "ADC".to_string(),
        LolRole::Support => "Support".to_string(),
        LolRole::Unknown => "Unknown".to_string(),
    }
}

fn aggregate_from_history(records: &[PlayerMatchStatsRecord]) -> Option<PlayerAggregate> {
    if records.is_empty() {
        return None;
    }

    let mut aggregate = PlayerAggregate::default();
    for record in records {
        aggregate.matches_played += 1;
        aggregate.kills += record.kills as u32;
        aggregate.deaths += record.deaths as u32;
        aggregate.assists += record.assists as u32;
        aggregate.creep_score += record.creep_score as u32;
        aggregate.vision_score += record.vision_score as u32;
        aggregate.wards_placed += record.wards_placed as u32;
    }

    Some(aggregate)
}

fn metric_percentile<F>(
    peers: &[&PlayerAggregate],
    selector: F,
    player_aggregate: &PlayerAggregate,
) -> Option<u32>
where
    F: Fn(&PlayerAggregate) -> Option<f32>,
{
    let values = peers
        .iter()
        .filter_map(|aggregate| selector(aggregate))
        .collect::<Vec<_>>();

    percentile_rank(&values, selector(player_aggregate))
}

fn metric_dto<F>(
    aggregate: &PlayerAggregate,
    peers: &[&PlayerAggregate],
    can_compute_percentiles: bool,
    selector_total: F,
) -> PlayerAdvancedMetricDto
where
    F: Fn(&PlayerAggregate) -> u32,
{
    let total = selector_total(aggregate);

    PlayerAdvancedMetricDto {
        total,
        per_match: calculate_per_match(total, aggregate.matches_played),
        percentile: if can_compute_percentiles {
            metric_percentile(
                peers,
                |candidate| {
                    calculate_per_match(selector_total(candidate), candidate.matches_played)
                },
                aggregate,
            )
        } else {
            None
        },
    }
}

fn build_overview_from_aggregate(
    player_aggregate: &PlayerAggregate,
    peers: &[PlayerAggregate],
) -> PlayerStatsOverviewDto {
    let eligible_peers = peers
        .iter()
        .filter(|aggregate| aggregate.matches_played >= DEFAULT_MINIMUM_MATCHES)
        .collect::<Vec<_>>();
    let can_compute_percentiles = player_aggregate.matches_played >= DEFAULT_MINIMUM_MATCHES
        && eligible_peers.len() >= DEFAULT_MINIMUM_COHORT_SIZE;

    PlayerStatsOverviewDto {
        percentile_eligible: can_compute_percentiles,
        matches_played: player_aggregate.matches_played,
        metrics: PlayerStatsOverviewMetricsDto {
            kills: metric_dto(
                player_aggregate,
                &eligible_peers,
                can_compute_percentiles,
                |aggregate| aggregate.kills,
            ),
            deaths: metric_dto(
                player_aggregate,
                &eligible_peers,
                can_compute_percentiles,
                |aggregate| aggregate.deaths,
            ),
            assists: metric_dto(
                player_aggregate,
                &eligible_peers,
                can_compute_percentiles,
                |aggregate| aggregate.assists,
            ),
            creep_score: metric_dto(
                player_aggregate,
                &eligible_peers,
                can_compute_percentiles,
                |aggregate| aggregate.creep_score,
            ),
            vision_score: metric_dto(
                player_aggregate,
                &eligible_peers,
                can_compute_percentiles,
                |aggregate| aggregate.vision_score,
            ),
            wards_placed: metric_dto(
                player_aggregate,
                &eligible_peers,
                can_compute_percentiles,
                |aggregate| aggregate.wards_placed,
            ),
        },
    }
}

fn build_history_overview(
    state: &StateManager,
    player_id: &str,
) -> Result<Option<PlayerStatsOverviewDto>, String> {
    let game = state
        .get_game(|game| game.clone())
        .ok_or("No active game session".to_string())?;
    let Some(player) = game
        .players
        .iter()
        .find(|candidate| candidate.id == player_id)
    else {
        return Err("Player not found".to_string());
    };
    let target_position = position_key(player);
    let same_position_ids = game
        .players
        .iter()
        .filter(|candidate| position_key(candidate) == target_position)
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();

    let Some(history_aggregates) = state.get_stats_state(|stats| {
        let mut records_by_player: HashMap<String, Vec<PlayerMatchStatsRecord>> = HashMap::new();

        for record in &stats.player_matches {
            if same_position_ids
                .iter()
                .any(|candidate_id| candidate_id == &record.player_id)
            {
                records_by_player
                    .entry(record.player_id.clone())
                    .or_default()
                    .push(record.clone());
            }
        }

        records_by_player
            .into_iter()
            .filter_map(|(candidate_id, records)| {
                aggregate_from_history(&records).map(|aggregate| (candidate_id, aggregate))
            })
            .collect::<HashMap<_, _>>()
    }) else {
        return Ok(None);
    };

    let Some(player_aggregate) = history_aggregates.get(player_id) else {
        return Ok(None);
    };

    let peers = same_position_ids
        .iter()
        .filter_map(|candidate_id| history_aggregates.get(candidate_id).cloned())
        .collect::<Vec<_>>();

    Ok(Some(build_overview_from_aggregate(
        player_aggregate,
        &peers,
    )))
}

fn build_legacy_overview() -> PlayerStatsOverviewDto {
    // Transitional compatibility for legacy saves without LoL match history.
    PlayerStatsOverviewDto {
        percentile_eligible: false,
        matches_played: 0,
        metrics: PlayerStatsOverviewMetricsDto {
            kills: PlayerAdvancedMetricDto {
                total: 0,
                per_match: None,
                percentile: None,
            },
            deaths: PlayerAdvancedMetricDto {
                total: 0,
                per_match: None,
                percentile: None,
            },
            assists: PlayerAdvancedMetricDto {
                total: 0,
                per_match: None,
                percentile: None,
            },
            creep_score: PlayerAdvancedMetricDto {
                total: 0,
                per_match: None,
                percentile: None,
            },
            vision_score: PlayerAdvancedMetricDto {
                total: 0,
                per_match: None,
                percentile: None,
            },
            wards_placed: PlayerAdvancedMetricDto {
                total: 0,
                per_match: None,
                percentile: None,
            },
        },
    }
}

fn to_dto(state: &StateManager, record: &PlayerMatchStatsRecord) -> PlayerMatchHistoryEntryDto {
    PlayerMatchHistoryEntryDto {
        fixture_id: record.fixture_id.clone(),
        date: record.date.clone(),
        competition: competition_label(&record.match_type),
        matchday: record.matchday,
        opponent_team_id: record.opponent_team_id.clone(),
        opponent_name: opponent_name(state, &record.opponent_team_id),
        side: side_label(record.side),
        result: outcome_label(record.result),
        role: role_label(record.role),
        champion_id: record.champion.clone(),
        champion_win: Some(matches!(record.result, MatchOutcome::Win)),
        game_duration_seconds: record.duration_seconds,
        kills: record.kills,
        deaths: record.deaths,
        assists: record.assists,
        cs: record.creep_score,
        gold_earned: record.gold_earned,
        damage_to_champions: record.damage_dealt,
        vision_score: record.vision_score,
        wards_placed: record.wards_placed,
    }
}

pub(super) fn get_player_match_history_internal(
    state: &StateManager,
    player_id: &str,
    limit: Option<usize>,
) -> Result<Vec<PlayerMatchHistoryEntryDto>, String> {
    let Some(mut history) = state.get_stats_state(|stats| {
        stats
            .player_matches
            .iter()
            .filter(|record| record.player_id == player_id)
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
        .map(|record| to_dto(state, &record))
        .collect())
}

pub(super) fn get_player_stats_overview_internal(
    state: &StateManager,
    player_id: &str,
) -> Result<PlayerStatsOverviewDto, String> {
    if let Some(overview) = build_history_overview(state, player_id)? {
        return Ok(overview);
    }

    let game = state
        .get_game(|game| game.clone())
        .ok_or("No active game session".to_string())?;
    if game
        .players
        .iter()
        .all(|candidate| candidate.id != player_id)
    {
        return Err("Player not found".to_string());
    }

    Ok(build_legacy_overview())
}
