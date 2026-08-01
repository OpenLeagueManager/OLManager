use log::info;

use crate::game::Game;
use crate::player_rating::{effective_rating_for_assignment, natural_ovr, position_slots};

fn user_team_context<'a>(
    game: &'a Game,
) -> Option<(
    &'a crate::domain::team::Team,
    Vec<&'a crate::domain::player::Player>,
)> {
    let user_team_id = game.manager.team_id.as_deref()?;
    let team = game.teams.iter().find(|team| team.id == user_team_id)?;
    let roster = game
        .players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(user_team_id))
        .collect();

    Some((team, roster))
}

fn build_blocker(id: &str, severity: &str, text: String, tab: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "severity": severity,
        "text": text,
        "tab": tab
    })
}

fn contract_days_remaining(
    contract_end: Option<&str>,
    current_date: chrono::NaiveDate,
) -> Option<i64> {
    let contract_end = contract_end?;
    let end_date = chrono::NaiveDate::parse_from_str(contract_end, "%Y-%m-%d").ok()?;
    Some((end_date - current_date).num_days())
}

fn should_notify_contract_risk_30d(
    contract_end: Option<&str>,
    current_date: chrono::NaiveDate,
) -> bool {
    contract_days_remaining(contract_end, current_date) == Some(30)
}

fn build_effective_lineup_ids(
    saved_lineup_ids: &[String],
    roster: &[&crate::domain::player::Player],
) -> Vec<String> {
    let by_id: std::collections::HashMap<&str, &crate::domain::player::Player> = roster
        .iter()
        .copied()
        .map(|player| (player.id.as_str(), player))
        .collect();
    let mut used = std::collections::HashSet::new();
    let mut valid_saved_ids = Vec::new();

    for id in saved_lineup_ids {
        if by_id.contains_key(id.as_str()) && used.insert(id.clone()) {
            valid_saved_ids.push(id.clone());
        } else {
            eprintln!(
                "[build_effective_lineup_ids] saved_id {} NOT in by_id (keys: {:?}) or already used",
                id,
                by_id.keys().collect::<Vec<_>>()
            );
        }
    }

    let mut remaining_players: Vec<&crate::domain::player::Player> = roster
        .iter()
        .copied()
        .filter(|player| !used.contains(&player.id))
        .collect();
    remaining_players.sort_by(|left, right| {
        natural_ovr(right)
            .partial_cmp(&natural_ovr(left))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let slots = position_slots();

    if valid_saved_ids.len() >= 8 {
        let mut xi_ids = valid_saved_ids;
        while xi_ids.len() < 11 {
            let slot = slots.get(xi_ids.len());
            let best_index = remaining_players
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| {
                    let left_rating = slot.map_or_else(
                        || natural_ovr(left),
                        |slot| effective_rating_for_assignment(left, slot),
                    );
                    let right_rating = slot.map_or_else(
                        || natural_ovr(right),
                        |slot| effective_rating_for_assignment(right, slot),
                    );
                    left_rating
                        .partial_cmp(&right_rating)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(index, _)| index);

            let Some(best_index) = best_index else {
                break;
            };

            let player = remaining_players.remove(best_index);
            xi_ids.push(player.id.clone());
        }
        xi_ids.truncate(11);
        return xi_ids;
    }

    let mut xi_ids = valid_saved_ids;

    for slot in slots.iter().take(11) {
        let best_player = roster
            .iter()
            .copied()
            .filter(|player| !used.contains(&player.id))
            .max_by(|left, right| {
                effective_rating_for_assignment(left, slot)
                    .partial_cmp(&effective_rating_for_assignment(right, slot))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        let Some(player) = best_player else {
            break;
        };

        if used.insert(player.id.clone()) {
            xi_ids.push(player.id.clone());
        }
    }

    xi_ids
}

fn incomplete_lineup_blocker(
    effective_lineup_ids: &[String],
    roster: &[&crate::domain::player::Player],
) -> Option<serde_json::Value> {
    let selected_count = effective_lineup_ids.len();

    // For LoL, require only 5 roles instead of legacy 11-player lineups.
    let required_count = if is_lol_mode(roster) { 5 } else { 11 };
    let count_text = if is_lol_mode(roster) {
        "5 Starter Roles"
    } else {
        "Active lineup"
    };

    // Check minimum quantity first
    if selected_count < required_count && roster.len() >= required_count {
        return Some(build_blocker(
            "incomplete_lineup",
            "warn",
            format!(
                "{} has only {} players — set your lineup",
                count_text, selected_count
            ),
            "Squad",
        ));
    }

    // For LoL mode, also validate role coverage in the active lineup.
    if is_lol_mode(roster) && selected_count >= 5 {
        // Get players in the active lineup.
        let xi_id_set: std::collections::HashSet<&str> =
            effective_lineup_ids.iter().map(String::as_str).collect();

        let xi_roles: std::collections::HashSet<&'static str> = roster
            .iter()
            .filter(|player| xi_id_set.contains(player.id.as_str()))
            .map(|player| role_to_string(&player.position))
            .collect();

        let required_roles = ["TOP", "JUNGLE", "MID", "ADC", "SUPPORT"];
        let missing_roles: Vec<&str> = required_roles
            .iter()
            .copied()
            .filter(|role| !xi_roles.contains(role))
            .collect();

        if !missing_roles.is_empty() {
            // Build role-specific message
            let role_list = missing_roles.join(", ");
            let role_article = if missing_roles.len() == 1 {
                "rol"
            } else {
                "roles"
            };
            return Some(build_blocker(
                "incomplete_lineup",
                "warn",
                format!(
                    "Lineup incompleto: falta el {} {} en tu lineup. Asegurate de tener TOP, JUNGLE, MID, ADC y SUPPORT.",
                    role_article, role_list
                ),
                "Squad",
            ));
        }
    }

    None
}

fn urgent_unread_messages_blocker(game: &Game) -> Option<serde_json::Value> {
    let urgent_unread = game
        .messages
        .iter()
        .filter(|message| {
            !message.read && message.priority == crate::domain::message::MessagePriority::Urgent
        })
        .count();

    (urgent_unread > 0).then(|| {
        build_blocker(
            "urgent_messages",
            "info",
            format!("{} urgent unread message(s)", urgent_unread),
            "Inbox",
        )
    })
}

fn key_contract_risk_blocker(
    roster: &[&crate::domain::player::Player],
    effective_lineup_ids: &[String],
    current_date: chrono::NaiveDate,
) -> Option<serde_json::Value> {
    let effective_xi_id_set: std::collections::HashSet<&str> =
        effective_lineup_ids.iter().map(String::as_str).collect();

    let mut effective_xi_players: Vec<&crate::domain::player::Player> = roster
        .iter()
        .copied()
        .filter(|player| effective_xi_id_set.contains(player.id.as_str()))
        .collect();
    effective_xi_players.sort_by(|left, right| {
        natural_ovr(right)
            .partial_cmp(&natural_ovr(left))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let risky_key_players: Vec<&str> = effective_xi_players
        .into_iter()
        .take(3)
        .filter(|player| {
            should_notify_contract_risk_30d(player.contract_end.as_deref(), current_date)
        })
        .map(|player| player.match_name.as_str())
        .collect();

    (!risky_key_players.is_empty()).then(|| {
        build_blocker(
            "key_contract_risk",
            "warn",
            format!(
                "Key player contract risk in squad planning: {}",
                risky_key_players.join(", ")
            ),
            "Squad",
        )
    })
}

fn contract_wage_risk_blocker(
    team: &crate::domain::team::Team,
    roster: &[&crate::domain::player::Player],
    current_date: chrono::NaiveDate,
) -> Option<serde_json::Value> {
    let at_risk_wages: u32 = roster
        .iter()
        .copied()
        .filter(|player| {
            should_notify_contract_risk_30d(player.contract_end.as_deref(), current_date)
        })
        .map(|player| player.wage)
        .sum();

    let wage_budget = team.wage_budget.max(0) as u32;
    (wage_budget > 0 && at_risk_wages > wage_budget).then(|| {
        build_blocker(
            "contract_wage_risk",
            "warn",
            format!(
                "{} of wages are tied to at-risk contracts — review your wage budget",
                at_risk_wages
            ),
            "Finances",
        )
    })
}

fn minimum_main_roster_blocker(
    roster: &[&crate::domain::player::Player],
) -> Option<serde_json::Value> {
    (roster.len() < 5).then(|| {
        build_blocker(
            "main_roster_minimum",
            "warn",
            format!(
                "Plantel principal incompleto: tenes {} jugador(es). Necesitas al menos 5 para avanzar.",
                roster.len()
            ),
            "Squad",
        )
    })
}

fn main_role_coverage_blocker(
    roster: &[&crate::domain::player::Player],
) -> Option<serde_json::Value> {
    let role_set: std::collections::HashSet<&'static str> = roster
        .iter()
        .map(|player| role_to_string(&player.position))
        .collect();
    let required_roles = ["TOP", "JUNGLE", "MID", "ADC", "SUPPORT"];
    let missing_roles: Vec<&str> = required_roles
        .iter()
        .copied()
        .filter(|role| !role_set.contains(role))
        .collect();

    (!missing_roles.is_empty()).then(|| {
        build_blocker(
            "main_missing_roles",
            "warn",
            format!(
                "Plantel principal incompleto por roles: falta al menos un jugador en {}. No podes avanzar hasta cubrir TOP, JUNGLE, MID, ADC y SUPPORT.",
                missing_roles.join(", ")
            ),
            "Squad",
        )
    })
}

fn role_to_string(role: &crate::domain::stats::LolRole) -> &'static str {
    use crate::domain::stats::LolRole;
    match role {
        LolRole::Top => "TOP",
        LolRole::Jungle => "JUNGLE",
        LolRole::Mid => "MID",
        LolRole::Adc => "ADC",
        LolRole::Support => "SUPPORT",
        LolRole::Unknown => "UNKNOWN",
    }
}

/// Determines if the game is using LoL mode by checking for any player with a known LoL role.
/// In LoL mode, teams need 5 roles; in football mode, teams need 11 players.
fn is_lol_mode(roster: &[&crate::domain::player::Player]) -> bool {
    roster
        .iter()
        .any(|player| player.natural_position != crate::domain::stats::LolRole::Unknown)
}

fn academy_role_coverage_blocker(
    game: &Game,
    team: &crate::domain::team::Team,
) -> Option<serde_json::Value> {
    let academy_team_id = team.academy_team_id.clone().or_else(|| {
        game.teams
            .iter()
            .find(|candidate| {
                candidate.team_kind == crate::domain::team::TeamKind::Academy
                    && candidate.parent_team_id.as_deref() == Some(team.id.as_str())
            })
            .map(|academy| academy.id.clone())
    })?;

    let role_set: std::collections::HashSet<&'static str> = game
        .players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(academy_team_id.as_str()))
        .map(|player| role_to_string(&player.position))
        .collect();
    let required_roles = ["TOP", "JUNGLE", "MID", "ADC", "SUPPORT"];
    let missing_roles: Vec<&str> = required_roles
        .iter()
        .copied()
        .filter(|role| !role_set.contains(role))
        .collect();

    (!missing_roles.is_empty()).then(|| {
        build_blocker(
            "academy_missing_roles",
            "warn",
            format!(
                "Academia incompleta: falta al menos un jugador en {}. No podes avanzar hasta cubrir todos los roles.",
                missing_roles.join(", ")
            ),
            "YouthAcademy",
        )
    })
}

pub fn compute_blocking_actions(game: &Game) -> Vec<serde_json::Value> {
    let mut blockers = Vec::new();
    let (team, roster) = match user_team_context(game) {
        Some(context) => context,
        None => {
            info!("[cmd] compute_blocking_actions: no user team context");
            return blockers;
        }
    };
    let saved_xi_ids = &team.active_lineup_ids;
    let current_date = game.clock.current_date.date_naive();
    let effective_lineup_ids = build_effective_lineup_ids(saved_xi_ids, &roster);

    eprintln!(
        "[blockers] team={} active_lineup_ids={:?} effective_ids={:?} roster_count={}",
        team.id,
        saved_xi_ids,
        effective_lineup_ids,
        roster.len()
    );
    for p in &roster {
        eprintln!(
            "[blockers] roster player: id={} pos={:?} nat={:?}",
            p.id, p.position, p.natural_position
        );
    }

    if let Some(blocker) = incomplete_lineup_blocker(&effective_lineup_ids, &roster) {
        blockers.push(blocker);
    }

    if let Some(blocker) = minimum_main_roster_blocker(&roster) {
        blockers.push(blocker);
    }

    if let Some(blocker) = main_role_coverage_blocker(&roster) {
        blockers.push(blocker);
    }

    if let Some(blocker) = key_contract_risk_blocker(&roster, &effective_lineup_ids, current_date) {
        blockers.push(blocker);
    }

    if let Some(blocker) = contract_wage_risk_blocker(team, &roster, current_date) {
        blockers.push(blocker);
    }

    if let Some(blocker) = academy_role_coverage_blocker(game, team) {
        blockers.push(blocker);
    }

    if let Some(blocker) = urgent_unread_messages_blocker(game) {
        blockers.push(blocker);
    }

    let blocker_ids: Vec<String> = blockers
        .iter()
        .filter_map(|blocker| blocker.get("id").and_then(|id| id.as_str()))
        .map(|id| id.to_string())
        .collect();

    info!(
        "[cmd] compute_blocking_actions: date={}, team={}, roster={}, lineup={}, blockers={:?}",
        game.clock.current_date.format("%Y-%m-%d"),
        team.id,
        roster.len(),
        effective_lineup_ids.len(),
        blocker_ids
    );

    blockers
}
