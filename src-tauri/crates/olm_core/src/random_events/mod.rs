mod builders_reports;
mod message_builders;
mod responses;

pub use builders_reports::build_fan_petition_from_narrative;
pub use message_builders::build_media_story_from_narrative;
pub use responses::apply_event_response;

use crate::contracts::{ContractWarningStage, contract_warning_stage};
use crate::domain::message::*;
use crate::game::Game;
use chrono::Datelike;
use rand::RngExt;
use std::collections::HashMap;

fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn action(id: &str, label: &str, label_key: &str, action_type: ActionType) -> MessageAction {
    MessageAction {
        id: id.to_string(),
        label: label.to_string(),
        action_type,
        resolved: false,
        label_key: Some(label_key.to_string()),
    }
}

/// Generate random daily events. Called from process_day().
/// Events have low probability each day so they don't spam the player.
pub fn check_random_events(game: &mut Game) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let user_team_id = match game.manager.team_id.clone() {
        Some(id) => id,
        None => return,
    };

    let existing_ids: std::collections::HashSet<String> =
        game.messages.iter().map(|m| m.id.clone()).collect();

    let mut rng = rand::rng();
    let mut new_messages: Vec<InboxMessage> = Vec::new();

    // --- 1. Sponsor offer (1% chance per day) ---
    {
        let msg_id = format!("sponsor_{}", today);
        if !existing_ids.contains(&msg_id) && rng.random_range(0..100) == 0 {
            let team_name = game
                .teams
                .iter()
                .find(|t| t.id == user_team_id)
                .map(|t| t.name.as_str())
                .unwrap_or("Your Club");
            let amount = rng.random_range(12..=60) * 52_000; // 624k - 3.12M (annual)
            let sponsor_names = [
                "GreenTech Industries",
                "Nova Sports",
                "Titan Energy",
                "BlueWave Solutions",
                "Summit Capital",
                "Apex Motors",
                "FreshBrew Co.",
                "CityLink Telecom",
            ];
            let sponsor = sponsor_names[rng.random_range(0..sponsor_names.len())];

            new_messages.push(message_builders::sponsor_offer_message(
                &msg_id, team_name, sponsor, amount, &today,
            ));
        }
    }

    // --- 2. Media story (5% chance per day) ---
    {
        let msg_id = format!("media_{}", today);
        if !existing_ids.contains(&msg_id) && rng.random_range(0..20) == 0 {
            let team_name = game
                .teams
                .iter()
                .find(|t| t.id == user_team_id)
                .map(|t| t.name.as_str())
                .unwrap_or("Your Club");

            // Pick a random player for the story
            let team_players: Vec<&crate::domain::player::Player> = game
                .players
                .iter()
                .filter(|p| p.team_id.as_deref() == Some(&user_team_id))
                .collect();
            if !team_players.is_empty() {
                let player = team_players[rng.random_range(0..team_players.len())];
                let is_positive = rng.random_bool(0.6); // 60% positive stories

                new_messages.push(message_builders::media_story_message(
                    &msg_id,
                    team_name,
                    &player.id,
                    &player.match_name,
                    is_positive,
                    &today,
                ));

                // Apply morale effect
                let pid = player.id.clone();
                if let Some(p) = game.players.iter_mut().find(|p| p.id == pid) {
                    let delta: i16 = if is_positive {
                        rng.random_range(2..=5)
                    } else {
                        rng.random_range(-5..=-1)
                    };
                    p.morale = ((p.morale as i16) + delta).clamp(10, 100) as u8;
                }
            }
        }
    }

    // --- 3. Community event / club milestone (1% chance per day) ---
    {
        let msg_id = format!("community_{}", today);
        if !existing_ids.contains(&msg_id) && rng.random_range(0..100) == 0 {
            let team_name = game
                .teams
                .iter()
                .find(|t| t.id == user_team_id)
                .map(|t| t.name.as_str())
                .unwrap_or("Your Club");
            new_messages.push(message_builders::community_event_message(
                &msg_id, team_name, &today,
            ));
        }
    }

    // --- 5. Dressing room mood report (once per week, deterministic on Monday) ---
    {
        let is_monday = game.clock.current_date.weekday() == chrono::Weekday::Mon;
        let msg_id = format!("mood_report_{}", today);
        if !existing_ids.contains(&msg_id) && is_monday {
            let team_players: Vec<&crate::domain::player::Player> = game
                .players
                .iter()
                .filter(|p| p.team_id.as_deref() == Some(&user_team_id))
                .collect();
            if !team_players.is_empty() {
                let avg_morale: f64 = team_players.iter().map(|p| p.morale as f64).sum::<f64>()
                    / team_players.len() as f64;
                let low_morale_count = team_players.iter().filter(|p| p.morale < 40).count();
                let high_morale_count = team_players.iter().filter(|p| p.morale >= 80).count();

                new_messages.push(builders_reports::mood_report_message(
                    &msg_id,
                    avg_morale,
                    low_morale_count,
                    high_morale_count,
                    team_players.len(),
                    &today,
                ));
            }
        }
    }

    // --- 7. Board confidence check (after 3+ consecutive losses, 100% trigger once) ---
    {
        if let Some(league) = game.active_league() {
            let completed: Vec<_> = league
                .fixtures
                .iter()
                .filter(|f| {
                    f.status == crate::domain::league::FixtureStatus::Completed
                        && (f.home_team_id == user_team_id || f.away_team_id == user_team_id)
                })
                .collect();
            if completed.len() >= 3 {
                let last3 = &completed[completed.len() - 3..];
                let losses = last3
                    .iter()
                    .filter(|f| {
                        if let Some(r) = &f.result {
                            let user_goals = if f.home_team_id == user_team_id {
                                r.home_wins
                            } else {
                                r.away_wins
                            };
                            let opp_goals = if f.home_team_id == user_team_id {
                                r.away_wins
                            } else {
                                r.home_wins
                            };
                            user_goals < opp_goals
                        } else {
                            false
                        }
                    })
                    .count();
                let last_loss_date = last3
                    .last()
                    .map(|f| f.date.clone())
                    .unwrap_or(today.clone());
                let msg_id = format!("board_confidence_{}", last_loss_date);
                if losses >= 3 && !existing_ids.contains(&msg_id) {
                    new_messages.push(builders_reports::board_confidence_message(&msg_id, &today));
                }
            }
        }
    }

    // --- 8. Fan petition (2% chance per day) ---
    {
        let msg_id = format!("fan_petition_{}", today);
        if !existing_ids.contains(&msg_id) && rng.random_range(0..50) == 0 {
            let team_name = game
                .teams
                .iter()
                .find(|t| t.id == user_team_id)
                .map(|t| t.name.as_str())
                .unwrap_or("Your Club");
            new_messages.push(builders_reports::fan_petition_message(
                &msg_id, team_name, &today,
            ));
        }
    }

    // --- 9. Rival interest in player (5% chance per day) ---
    {
        let msg_id = format!("rival_interest_{}", today);
        if !existing_ids.contains(&msg_id) && rng.random_range(0..20) == 0 {
            let current_date = game.clock.current_date.date_naive();
            let eligible: Vec<&crate::domain::player::Player> = game
                .players
                .iter()
                .filter(|p| p.team_id.as_deref() == Some(&user_team_id))
                .collect();
            if !eligible.is_empty() {
                let player = choose_rival_interest_target(&eligible, current_date, &mut rng)
                    .unwrap_or(eligible[rng.random_range(0..eligible.len())]);
                let rival_pool: Vec<&str> = game
                    .teams
                    .iter()
                    .filter(|team| team.id != user_team_id)
                    .map(|team| team.name.as_str())
                    .collect();
                let rival = if rival_pool.is_empty() {
                    "Rival Club"
                } else {
                    rival_pool[rng.random_range(0..rival_pool.len())]
                };
                new_messages.push(builders_reports::rival_interest_message(
                    &msg_id,
                    &player.id,
                    &player.match_name,
                    rival,
                    &today,
                ));
            }
        }
    }

    // --- 10. Al Lío podcast (5% chance per day) ---
    {
        let msg_id = format!("allio_{}", today);
        if !existing_ids.contains(&msg_id) && rng.random_range(0..20) == 0 {
            let team_name = game
                .teams
                .iter()
                .find(|t| t.id == user_team_id)
                .map(|t| t.name.as_str())
                .unwrap_or("Your Club");
            let team_players: Vec<&crate::domain::player::Player> = game
                .players
                .iter()
                .filter(|p| p.team_id.as_deref() == Some(&user_team_id))
                .collect();
            let (player_id, player_name) = if !team_players.is_empty() {
                let p = team_players[rng.random_range(0..team_players.len())];
                (Some(p.id.as_str()), Some(p.match_name.as_str()))
            } else {
                (None, None)
            };
            new_messages.push(message_builders::allio_podcast_message(
                &msg_id,
                team_name,
                player_id,
                player_name,
                &today,
            ));
        }
    }

    // --- 11. el_yuste stream (3% chance per day) ---
    {
        let msg_id = format!("yuste_{}", today);
        if !existing_ids.contains(&msg_id) && rng.random_range(0..33) == 0 {
            let is_positive = rng.random_bool(0.5);
            new_messages.push(message_builders::yuste_stream_message(
                &msg_id,
                is_positive,
                &today,
            ));
        }
    }

    game.messages.extend(new_messages);
}

pub fn rival_interest_weight(
    player: &crate::domain::player::Player,
    current_date: chrono::NaiveDate,
) -> u32 {
    let contract_weight = match contract_warning_stage(player.contract_end.as_deref(), current_date)
    {
        Some(ContractWarningStage::FinalWeeks) => 16,
        Some(ContractWarningStage::ThreeMonths) => 12,
        Some(ContractWarningStage::SixMonths) => 8,
        Some(ContractWarningStage::TwelveMonths) => 5,
        None => 1,
    };

    let value_weight = if player.market_value >= 2_000_000 {
        3
    } else if player.market_value >= 750_000 {
        2
    } else {
        1
    };

    contract_weight + value_weight
}

fn choose_rival_interest_target<'a, R: rand::Rng + ?Sized>(
    eligible: &[&'a crate::domain::player::Player],
    current_date: chrono::NaiveDate,
    rng: &mut R,
) -> Option<&'a crate::domain::player::Player> {
    let total_weight: u32 = eligible
        .iter()
        .map(|player| rival_interest_weight(player, current_date))
        .sum();

    if total_weight == 0 {
        return None;
    }

    let mut roll = rng.random_range(0..total_weight);
    for player in eligible {
        let weight = rival_interest_weight(player, current_date);
        if roll < weight {
            return Some(*player);
        }
        roll -= weight;
    }

    eligible.last().copied()
}

fn format_money(amount: u64) -> String {
    if amount >= 1_000_000 {
        format!("{:.1}M", amount as f64 / 1_000_000.0)
    } else if amount >= 1_000 {
        format!("{}K", amount / 1_000)
    } else {
        amount.to_string()
    }
}
