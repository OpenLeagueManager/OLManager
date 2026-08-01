use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::engine::event::{EventType, MatchEvent};
use crate::engine::sim_background::{LolRole, LolUnitState};
use crate::engine::types::Side;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchReportEndReason {
    NexusDestroyed,
    TimeLimit,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamStats {
    #[serde(default, skip_serializing)]
    pub shots: u16,
    #[serde(default, skip_serializing)]
    pub shots_on_target: u16,
    pub kills: u16,
    pub deaths: u16,
    pub gold_earned: u32,
    pub damage_dealt: u32,
    pub objectives: u16,
    pub possession_ticks: u32,
}

impl TeamStats {
    pub fn pass_accuracy(&self) -> f64 {
        0.0
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerMatchStats {
    #[serde(default, skip_serializing)]
    pub minutes_played: u16,
    #[serde(default, skip_serializing)]
    pub rating: f32,
    #[serde(default, skip_serializing)]
    pub shots: u16,
    #[serde(default, skip_serializing)]
    pub shots_on_target: u16,
    #[serde(default, skip_serializing)]
    pub passes_completed: u16,
    #[serde(default, skip_serializing)]
    pub passes_attempted: u16,
    #[serde(default, skip_serializing)]
    pub tackles_won: u16,
    #[serde(default, skip_serializing)]
    pub interceptions: u16,
    pub role: Option<LolRole>,
    pub duration_seconds: u32,
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    pub creep_score: u16,
    pub gold_earned: u32,
    pub damage_dealt: u32,
    pub vision_score: u16,
    pub wards_placed: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillDetail {
    pub minute: u8,
    pub killer_id: String,
    pub victim_id: Option<String>,
    pub assist_id: Option<String>,
    pub side: Side,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchReport {
    pub home_wins: u8,
    pub away_wins: u8,
    pub home_stats: TeamStats,
    pub away_stats: TeamStats,
    pub events: Vec<MatchEvent>,
    pub kill_feed: Vec<KillDetail>,
    pub player_stats: HashMap<String, PlayerMatchStats>,
    pub home_possession: f64,
    #[serde(default, skip_serializing)]
    pub total_minutes: u8,
    pub game_duration_seconds: u32,
    pub ended_by: MatchReportEndReason,
}

impl MatchReport {
    pub fn from_events(
        events: Vec<MatchEvent>,
        home_possession_ticks: u32,
        away_possession_ticks: u32,
        total_minutes: u8,
    ) -> Self {
        Self::from_events_with_players(
            events,
            home_possession_ticks,
            away_possession_ticks,
            total_minutes,
            Vec::new(),
        )
    }

    pub fn from_events_with_players(
        events: Vec<MatchEvent>,
        home_possession_ticks: u32,
        away_possession_ticks: u32,
        total_minutes: u8,
        tracked_player_ids: Vec<String>,
    ) -> Self {
        Self::from_events_internal(
            events,
            home_possession_ticks,
            away_possession_ticks,
            total_minutes,
            tracked_player_ids,
            None,
            None,
        )
    }

    pub fn from_events_with_lol_snapshot(
        events: Vec<MatchEvent>,
        home_possession_ticks: u32,
        away_possession_ticks: u32,
        total_minutes: u8,
        tracked_player_ids: Vec<String>,
        lol_units: &[LolUnitState],
        destroyed_nexus_by: Option<Side>,
    ) -> Self {
        Self::from_events_internal(
            events,
            home_possession_ticks,
            away_possession_ticks,
            total_minutes,
            tracked_player_ids,
            Some(lol_units),
            destroyed_nexus_by,
        )
    }

    fn from_events_internal(
        events: Vec<MatchEvent>,
        home_possession_ticks: u32,
        away_possession_ticks: u32,
        total_minutes: u8,
        tracked_player_ids: Vec<String>,
        lol_units: Option<&[LolUnitState]>,
        destroyed_nexus_by: Option<Side>,
    ) -> Self {
        let mut home_stats = TeamStats {
            possession_ticks: home_possession_ticks,
            ..Default::default()
        };
        let mut away_stats = TeamStats {
            possession_ticks: away_possession_ticks,
            ..Default::default()
        };
        let mut kill_feed = Vec::new();
        let mut player_stats: HashMap<String, PlayerMatchStats> = tracked_player_ids
            .iter()
            .cloned()
            .map(|player_id| (player_id, PlayerMatchStats::default()))
            .collect();

        for event in &events {
            let (stats, opposing_stats) = match event.side {
                Side::Home => (&mut home_stats, &mut away_stats),
                Side::Away => (&mut away_stats, &mut home_stats),
            };
            let pid = event.player_id.as_deref().unwrap_or("");

            match &event.event_type {
                EventType::Kill => {
                    stats.kills += 1;
                    opposing_stats.deaths += 1;
                    kill_feed.push(KillDetail {
                        minute: event.minute,
                        killer_id: pid.to_string(),
                        victim_id: event.secondary_player_id.clone(),
                        assist_id: None,
                        side: event.side,
                    });

                    if !pid.is_empty() {
                        player_stats.entry(pid.to_string()).or_default().kills += 1;
                    }
                    if let Some(victim_id) = event.secondary_player_id.as_ref() {
                        player_stats.entry(victim_id.clone()).or_default().deaths += 1;
                    }
                }
                EventType::TowerDestroyed
                | EventType::InhibitorDestroyed
                | EventType::NexusTowerDestroyed
                | EventType::NexusDestroyed
                | EventType::ObjectiveTaken => {
                    stats.objectives += 1;
                }
                _ => {}
            }
        }

        if let Some(units) = lol_units {
            let mut home_kills = 0_u16;
            let mut away_kills = 0_u16;
            let mut home_deaths = 0_u16;
            let mut away_deaths = 0_u16;
            let mut home_gold = 0_u32;
            let mut away_gold = 0_u32;
            let mut home_damage = 0_u32;
            let mut away_damage = 0_u32;

            for unit in units {
                let stats = player_stats.entry(unit.player_id.clone()).or_default();
                stats.role = Some(unit.role);
                stats.kills = unit.kills as u16;
                stats.deaths = unit.deaths as u16;
                stats.gold_earned = unit.gold.round().max(0.0) as u32;
                stats.damage_dealt = unit.damage_dealt.round().max(0.0) as u32;

                match unit.side {
                    Side::Home => {
                        home_kills += unit.kills as u16;
                        home_deaths += unit.deaths as u16;
                        home_gold += stats.gold_earned;
                        home_damage += stats.damage_dealt;
                    }
                    Side::Away => {
                        away_kills += unit.kills as u16;
                        away_deaths += unit.deaths as u16;
                        away_gold += stats.gold_earned;
                        away_damage += stats.damage_dealt;
                    }
                }
            }

            home_stats.kills = home_kills;
            home_stats.deaths = home_deaths;
            home_stats.gold_earned = home_gold;
            home_stats.damage_dealt = home_damage;
            away_stats.kills = away_kills;
            away_stats.deaths = away_deaths;
            away_stats.gold_earned = away_gold;
            away_stats.damage_dealt = away_damage;
        }

        populate_duration_seconds(
            &events,
            total_minutes,
            &tracked_player_ids,
            &mut player_stats,
        );

        let total_possession = home_possession_ticks + away_possession_ticks;
        let home_possession = if total_possession > 0 {
            home_possession_ticks as f64 / total_possession as f64 * 100.0
        } else {
            50.0
        };

        let ended_by = if destroyed_nexus_by.is_some()
            || events
                .iter()
                .any(|event| matches!(&event.event_type, EventType::NexusDestroyed))
        {
            MatchReportEndReason::NexusDestroyed
        } else {
            MatchReportEndReason::TimeLimit
        };

        let winner = destroyed_nexus_by
            .or_else(|| {
                events.iter().find_map(|event| {
                    matches!(&event.event_type, EventType::NexusDestroyed).then_some(event.side)
                })
            })
            .unwrap_or_else(|| pick_winner(&home_stats, &away_stats));

        let (home_wins, away_wins) = match winner {
            Side::Home => (1, 0),
            Side::Away => (0, 1),
        };

        Self {
            home_wins,
            away_wins,
            home_stats,
            away_stats,
            events,
            kill_feed,
            player_stats,
            home_possession,
            total_minutes,
            game_duration_seconds: u32::from(total_minutes) * 60,
            ended_by,
        }
    }
}

fn pick_winner(home_stats: &TeamStats, away_stats: &TeamStats) -> Side {
    use std::cmp::Ordering;

    match home_stats
        .objectives
        .cmp(&away_stats.objectives)
        .then(home_stats.kills.cmp(&away_stats.kills))
        .then(home_stats.gold_earned.cmp(&away_stats.gold_earned))
        .then(home_stats.damage_dealt.cmp(&away_stats.damage_dealt))
        .then(
            home_stats
                .possession_ticks
                .cmp(&away_stats.possession_ticks),
        ) {
        Ordering::Greater => Side::Home,
        Ordering::Less => Side::Away,
        // LoL no permite empate; usamos desempate determinista final.
        Ordering::Equal => Side::Home,
    }
}

fn populate_duration_seconds(
    events: &[MatchEvent],
    total_minutes: u8,
    tracked_player_ids: &[String],
    player_stats: &mut HashMap<String, PlayerMatchStats>,
) {
    let mut minutes_by_player: HashMap<String, u8> = tracked_player_ids
        .iter()
        .cloned()
        .map(|player_id| (player_id, total_minutes))
        .collect();

    for event in events {
        match &event.event_type {
            EventType::Substitution => {
                if let Some(player_off_id) = event.secondary_player_id.as_ref() {
                    minutes_by_player
                        .insert(player_off_id.clone(), event.minute.min(total_minutes));
                }
                if let Some(player_on_id) = event.player_id.as_ref() {
                    minutes_by_player.insert(
                        player_on_id.clone(),
                        total_minutes.saturating_sub(event.minute),
                    );
                }
            }
            _ => {}
        }
    }

    for (player_id, minutes_played) in minutes_by_player {
        let stats = player_stats.entry(player_id).or_default();
        stats.minutes_played = minutes_played.into();
        stats.duration_seconds = u32::from(minutes_played) * 60;
    }
}
