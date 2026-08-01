use crate::domain::message::*;
use crate::domain::staff::StaffRole;
use crate::domain::stats::LolRole;
use crate::domain::team::MainFacilityModuleKind;
use crate::game::{Game, ScoutingAssignment};
use rand::RngExt;
use std::collections::HashMap;
use uuid::Uuid;

fn lol_role_to_string(role: &LolRole) -> &'static str {
    match role {
        LolRole::Top => "TOP",
        LolRole::Jungle => "JUNGLE",
        LolRole::Mid => "MID",
        LolRole::Adc => "ADC",
        LolRole::Support => "SUPPORT",
        LolRole::Unknown => "UNKNOWN",
    }
}

fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Determine how many concurrent scouting assignments a scout can handle.
/// Higher judging_ability = more slots (1 to 5).
pub fn scout_max_assignments(judging_ability: u8) -> usize {
    if judging_ability >= 80 {
        5
    } else if judging_ability >= 60 {
        4
    } else if judging_ability >= 40 {
        3
    } else if judging_ability >= 20 {
        2
    } else {
        1
    }
}

/// Send a scout to evaluate a player. Returns an error string if invalid.
pub fn send_scout(game: &mut Game, scout_id: &str, player_id: &str) -> Result<(), String> {
    let user_team_id = game.manager.team_id.as_ref().ok_or("No team")?;
    let scouting_facility_level = game
        .teams
        .iter()
        .find(|team| team.id == user_team_id.as_str())
        .map(|team| {
            team.facilities
                .module_level(MainFacilityModuleKind::ScoutingLab)
        })
        .unwrap_or(1);

    // Validate scout exists and belongs to user's team
    let scout = game
        .staff
        .iter()
        .find(|s| s.id == scout_id)
        .ok_or("Scout not found")?;
    if scout.role != StaffRole::Scout {
        return Err("Staff member is not a scout".to_string());
    }
    if scout.team_id.as_ref() != Some(user_team_id) {
        return Err("Scout does not belong to your team".to_string());
    }

    // Validate player exists
    if !game.players.iter().any(|p| p.id == player_id) {
        return Err("Player not found".to_string());
    }

    // Check scout capacity: higher ability = more concurrent assignments
    let max_slots = scout_max_assignments(scout.attributes.judging_ability);
    let facility_slot_bonus = usize::from(scouting_facility_level.saturating_sub(1) / 2);
    let max_slots = max_slots + facility_slot_bonus;
    let current_count = game
        .scouting_assignments
        .iter()
        .filter(|a| a.scout_id == scout_id)
        .count();
    if current_count >= max_slots {
        return Err(format!(
            "Scout is at capacity ({}/{} assignments). Higher judging ability allows more.",
            current_count, max_slots
        ));
    }

    // Check if player is already being scouted
    if game
        .scouting_assignments
        .iter()
        .any(|a| a.player_id == player_id)
    {
        return Err("This player is already being scouted".to_string());
    }

    // Create assignment (2-5 days depending on scout quality)
    let judging = scout.attributes.judging_ability as u32;
    let base_days: u32 = if judging >= 80 {
        2
    } else if judging >= 60 {
        3
    } else if judging >= 40 {
        4
    } else {
        5
    };
    let facility_days_reduction: u32 = if scouting_facility_level >= 4 {
        2
    } else if scouting_facility_level >= 2 {
        1
    } else {
        0
    };
    let days = base_days.saturating_sub(facility_days_reduction).max(1);

    game.scouting_assignments.push(ScoutingAssignment {
        id: Uuid::new_v4().to_string(),
        scout_id: scout_id.to_string(),
        player_id: player_id.to_string(),
        days_remaining: days,
    });

    Ok(())
}

/// Process scouting assignments daily. Called from process_day().
/// Decrements days, delivers reports when complete.
pub fn process_scouting(game: &mut Game) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let mut completed: Vec<ScoutingAssignment> = Vec::new();

    for assignment in game.scouting_assignments.iter_mut() {
        if assignment.days_remaining > 0 {
            assignment.days_remaining -= 1;
        }
        if assignment.days_remaining == 0 {
            completed.push(assignment.clone());
        }
    }

    // Remove completed assignments
    game.scouting_assignments.retain(|a| a.days_remaining > 0);

    // Generate reports for completed assignments
    let user_team_id = game.manager.team_id.clone();
    for assignment in &completed {
        let scout = game.staff.iter().find(|s| s.id == assignment.scout_id);
        let player = game.players.iter().find(|p| p.id == assignment.player_id);

        if let (Some(scout), Some(player)) = (scout, player) {
            let scout_name = format!("{} {}", scout.first_name, scout.last_name);
            let judging_ability = scout.attributes.judging_ability;
            let judging_potential = scout.attributes.judging_potential;
            let team_name = player
                .team_id
                .as_ref()
                .and_then(|tid| game.teams.iter().find(|t| &t.id == tid))
                .map(|t| t.name.clone());
            let is_own_player = player.team_id.as_deref() == user_team_id.as_deref();

            let msg = build_scout_report(
                &assignment.id,
                &scout_name,
                &player.id,
                &player.match_name,
                &player.nationality,
                &player.date_of_birth,
                lol_role_to_string(&player.natural_position),
                &player.attributes,
                player.morale,
                player.condition,
                judging_ability,
                judging_potential,
                team_name.as_deref(),
                &today,
                is_own_player,
            );
            game.messages.push(msg);
        }
    }
}

fn build_scout_report(
    assignment_id: &str,
    scout_name: &str,
    player_id: &str,
    player_name: &str,
    nationality: &str,
    dob: &str,
    position: &str,
    attrs: &crate::domain::player::PlayerAttributes,
    morale: u8,
    condition: u8,
    judging_ability: u8,
    judging_potential: u8,
    team_name: Option<&str>,
    date: &str,
    is_own_player: bool,
) -> InboxMessage {
    let mut rng = rand::rng();

    // Accuracy: higher judging = less noise on reported attributes
    // Own players get perfect reports (no noise) since the scout knows the squad
    let noise_range = if is_own_player {
        0
    } else if judging_ability >= 80 {
        2
    } else if judging_ability >= 60 {
        5
    } else if judging_ability >= 40 {
        8
    } else {
        12
    };

    let mut fuzz = |val: u8| -> u8 {
        let delta: i16 = rng.random_range(-(noise_range as i16)..=(noise_range as i16));
        ((val as i16) + delta).clamp(1, 99) as u8
    };

    // Build fuzzed LoL-facing attribute values. The underlying save still uses
    // legacy-shaped fields, but reports should reveal the concepts the
    // LoL UI teaches: mechanics, laning, teamfighting, macro, champion pool and
    // discipline.
    let all_fuzzed: [(u8, &str); 6] = [
        (fuzz(attrs.mechanics), "Mechanics"),
        (fuzz(attrs.laning), "Laning"),
        (fuzz(attrs.teamfighting), "Teamfighting"),
        (fuzz(attrs.macro_play), "Macro"),
        (fuzz(attrs.champion_pool), "Champion Pool"),
        (fuzz(attrs.discipline), "Discipline"),
    ];

    // Discovery mechanic: scout ability determines how many attrs are revealed
    // 80+: all 6 attrs + condition + morale
    // 60-79: 5 attrs + condition
    // 40-59: 3 attrs
    // <40: 2 attrs
    let reveal_count: usize = if judging_ability >= 80 {
        6
    } else if judging_ability >= 60 {
        5
    } else if judging_ability >= 40 {
        3
    } else {
        2
    };

    // Shuffle indices to determine which attrs are hidden
    let mut indices: Vec<usize> = (0..6).collect();
    for i in (1..indices.len()).rev() {
        let j = rng.random_range(0..=i);
        indices.swap(i, j);
    }
    let revealed: std::collections::HashSet<usize> =
        indices[..reveal_count].iter().cloned().collect();

    let to_opt = |idx: usize| -> Option<u8> {
        if revealed.contains(&idx) {
            Some(all_fuzzed[idx].0)
        } else {
            None
        }
    };

    let mechanics = to_opt(0);
    let laning = to_opt(1);
    let teamfighting = to_opt(2);
    let macro_ = to_opt(3);
    let champion_pool = to_opt(4);
    let discipline = to_opt(5);

    let reported_condition = if judging_ability >= 60 {
        Some(condition)
    } else {
        None
    };
    let reported_morale = if judging_ability >= 80 {
        Some(morale)
    } else {
        None
    };

    // Overall assessment based on revealed attrs only
    let revealed_vals: Vec<u32> = (0..6).filter_map(|i| to_opt(i).map(|v| v as u32)).collect();
    let avg_attrs = if revealed_vals.is_empty() {
        0
    } else {
        revealed_vals.iter().sum::<u32>() / revealed_vals.len() as u32
    };

    let rating_key = if avg_attrs >= 80 {
        "common.scoutRatings.excellent"
    } else if avg_attrs >= 70 {
        "common.scoutRatings.veryGood"
    } else if avg_attrs >= 60 {
        "common.scoutRatings.good"
    } else if avg_attrs >= 50 {
        "common.scoutRatings.average"
    } else {
        "common.scoutRatings.belowAverage"
    };

    // Potential assessment (based on judging_potential accuracy)
    let potential_key = if judging_potential >= 70 {
        if avg_attrs >= 75 {
            "common.scoutPotential.worldClass"
        } else if avg_attrs >= 60 {
            "common.scoutPotential.strong"
        } else {
            "common.scoutPotential.moderate"
        }
    } else {
        "common.scoutPotential.unclear"
    };

    // Confidence level
    let confidence_key = if judging_ability >= 80 {
        "common.scoutConfidence.high"
    } else if judging_ability >= 60 {
        "common.scoutConfidence.moderate"
    } else {
        "common.scoutConfidence.low"
    };

    // Build structured report data for the player card
    let report_data = ScoutReportData {
        player_id: player_id.to_string(),
        player_name: player_name.to_string(),
        position: position.to_string(),
        nationality: nationality.to_string(),
        dob: dob.to_string(),
        team_name: team_name.map(|s| s.to_string()),
        // Legacy field names are kept for saved-message compatibility, but now
        // carry the same LoL values as the explicit fields below.
        pace: mechanics,
        shooting: laning,
        passing: teamfighting,
        dribbling: macro_,
        defending: champion_pool,
        physical: discipline,
        mechanics,
        laning,
        teamfighting,
        macro_,
        champion_pool,
        discipline,
        condition: reported_condition,
        morale: reported_morale,
        avg_rating: Some(avg_attrs),
        rating_key: rating_key.to_string(),
        potential_key: potential_key.to_string(),
        confidence_key: confidence_key.to_string(),
    };

    // Fallback body text (used when i18n key is not found)
    let body = format!(
        "Scout report on {} completed by {}.",
        player_name, scout_name
    );

    let msg_id = format!("scout_report_{}", assignment_id);

    InboxMessage::new(
        msg_id,
        format!("Scout Report — {}", player_name),
        body,
        scout_name.to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::ScoutReport)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Scout")
    .with_action(MessageAction {
        id: "ack".to_string(),
        label: "Noted".to_string(),
        action_type: ActionType::Acknowledge,
        resolved: false,
        label_key: Some("be.msg.event.ack".to_string()),
    })
    .with_context(MessageContext {
        player_id: Some(player_id.to_string()),
        scout_report: Some(report_data),
        ..Default::default()
    })
    .with_i18n("be.msg.scoutReport.subject", "be.msg.scoutReport.body", {
        let mut p = params(&[("player", player_name), ("scout", scout_name)]);
        p.insert("ratingDesc".to_string(), rating_key.to_string());
        p.insert("potentialDesc".to_string(), potential_key.to_string());
        p.insert("confidence".to_string(), confidence_key.to_string());
        p
    })
    .with_sender_i18n("be.sender.scout", "be.role.scout")
}
