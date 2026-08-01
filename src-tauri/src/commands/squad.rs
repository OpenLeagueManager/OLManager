use chrono::Datelike;
use log::info;
use serde::Serialize;
use tauri::State;

use olm_core::champions;
use olm_core::game::{DayPhase, Game};
use olm_core::potential;
use olm_core::scrim_flow::{
    transition_daily_scrim_flow, DailyScrimFlowEvent, DailyScrimFlowState, ScrimResultQuality,
};
use olm_core::state::StateManager;

fn parse_post_scrim_decision(
    value: &str,
) -> Result<olm_core::domain::team::PostScrimDecision, String> {
    match value {
        "ContinuePlan" => Ok(olm_core::domain::team::PostScrimDecision::ContinuePlan),
        "VodReview" => Ok(olm_core::domain::team::PostScrimDecision::VodReview),
        "MentalReset" => Ok(olm_core::domain::team::PostScrimDecision::MentalReset),
        "TargetedDrills" => Ok(olm_core::domain::team::PostScrimDecision::TargetedDrills),
        "PushThrough" => Ok(olm_core::domain::team::PostScrimDecision::PushThrough),
        "DayOff" => Ok(olm_core::domain::team::PostScrimDecision::DayOff),
        _ => Err(format!("Unknown post-scrim decision: {value}")),
    }
}

fn parse_scrim_focus(value: &str) -> Result<olm_core::domain::team::ScrimFocus, String> {
    match value {
        "DraftPrep" => Ok(olm_core::domain::team::ScrimFocus::DraftPrep),
        "ChampionPool" => Ok(olm_core::domain::team::ScrimFocus::ChampionPool),
        "EarlyGame" => Ok(olm_core::domain::team::ScrimFocus::EarlyGame),
        "Teamfighting" => Ok(olm_core::domain::team::ScrimFocus::Teamfighting),
        "Macro" => Ok(olm_core::domain::team::ScrimFocus::Macro),
        "Mental" => Ok(olm_core::domain::team::ScrimFocus::Mental),
        _ => Err(format!("Unknown scrim objective: {value}")),
    }
}

fn scrims_per_week_for_schedule(schedule: &olm_core::domain::team::TrainingSchedule) -> u8 {
    match schedule {
        olm_core::domain::team::TrainingSchedule::Intense => 6,
        olm_core::domain::team::TrainingSchedule::Balanced => 4,
        olm_core::domain::team::TrainingSchedule::Light => 2,
    }
}

fn effective_scrim_slots(raw_slots: u8, schedule: &olm_core::domain::team::TrainingSchedule) -> u8 {
    if raw_slots == 0 {
        return scrims_per_week_for_schedule(schedule);
    }

    match raw_slots.clamp(2, 6) {
        0..=2 => 2,
        3..=4 => 4,
        _ => 6,
    }
}

fn scrim_slot_weekdays(slots: u8) -> Vec<u8> {
    match slots {
        0..=2 => vec![2, 2],
        3..=4 => vec![2, 2, 3, 3],
        _ => vec![2, 2, 3, 3, 4, 4],
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TodayScrimContextResponse {
    pub state: String,
    pub slot_index: Option<u8>,
    pub opponent_team_id: Option<String>,
    pub resolved_opponent_team_id: Option<String>,
    pub objective: Option<olm_core::domain::team::ScrimFocus>,
    pub report: Option<olm_core::domain::team::ScrimReport>,
    pub can_edit_plan: bool,
    pub can_cancel: bool,
    pub can_review: bool,
    pub can_view_weekly_plan: bool,
    pub has_official_match: bool,
    pub primary_action: Option<String>,
    pub push_through_recommended: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeeklyScrimSlotContextResponse {
    pub slot_index: u8,
    pub weekday: u8,
    pub label: String,
    pub label_day: u8,
    pub label_suffix: String,
    pub plan: Vec<String>,
    pub resolved_opponent_team_id: Option<String>,
    pub result_won: Option<bool>,
    pub report: Option<olm_core::domain::team::ScrimReport>,
    pub status: String,
    pub can_edit: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeeklyScrimContextResponse {
    pub week_key: String,
    pub objective: Option<olm_core::domain::team::ScrimFocus>,
    pub capacity: u8,
    pub planned: u8,
    pub reputation: u8,
    pub cancellations: u8,
    pub played: u8,
    pub wins: u8,
    pub losses: u8,
    pub loss_streak: u8,
    pub avg_quality: u8,
    pub top_focus: Option<olm_core::domain::team::ScrimFocus>,
    pub top_issue: Option<olm_core::domain::team::ScrimIssue>,
    pub next_official_rival_team_id: Option<String>,
    pub next_official_rival_competition: Option<olm_core::domain::league::MatchType>,
    pub setup_locked: bool,
    pub setup_locked_reason: Option<String>,
    pub can_finalize_setup: bool,
    pub slots: Vec<WeeklyScrimSlotContextResponse>,
    pub latest_reports: Vec<olm_core::domain::team::ScrimReport>,
}

fn weekly_scrim_setup_lock_state(
    team: &olm_core::domain::team::Team,
    week_key: &str,
    current_weekday: u8,
    day_phase: DayPhase,
) -> (bool, Option<String>) {
    let manual_lock = team.scrim_setup_locked_week_key.as_deref() == Some(week_key);
    if manual_lock {
        return (true, Some("manual".to_string()));
    }

    let started_week = team
        .scrim_reports
        .iter()
        .any(|entry| entry.week_key == week_key)
        || team
            .scrim_slot_results
            .iter()
            .any(|entry| entry.week_key == week_key);
    if started_week {
        return (true, Some("week_started".to_string()));
    }

    let first_scrim_weekday = scrim_slot_weekdays(effective_scrim_slots(
        team.scrim_weekly_slots,
        &team.training_schedule,
    ))
    .into_iter()
    .min()
    .unwrap_or(2);

    let has_any_plan = team
        .weekly_scrim_plan_team_ids
        .iter()
        .any(|plan| !plan.is_empty());
    if !has_any_plan {
        // Allow configuration at any time when no scrims have been planned yet
        // (e.g. first day of the game or start of a new split)
        return (false, None);
    }

    if current_weekday > first_scrim_weekday
        || (current_weekday == first_scrim_weekday && day_phase != DayPhase::Morning)
    {
        return (true, Some("first_scrim_window".to_string()));
    }

    (false, None)
}

#[derive(Debug, Clone, Serialize)]
pub struct ScrimContextResponse {
    pub today: TodayScrimContextResponse,
    pub week: WeeklyScrimContextResponse,
}

fn slot_label_parts(weekdays: &[u8], slot_index: usize) -> (u8, String) {
    let day = weekdays.get(slot_index).copied().unwrap_or(0);
    let previous_same_day = weekdays
        .iter()
        .take(slot_index)
        .filter(|candidate| **candidate == day)
        .count();
    let total_same_day = weekdays
        .iter()
        .filter(|candidate| **candidate == day)
        .count();
    let suffix = if total_same_day > 1 {
        ((b'A' + previous_same_day as u8) as char).to_string()
    } else {
        String::new()
    };
    (day, suffix)
}

fn is_push_through_recommended(
    won: bool,
    severity: u8,
    own_loss_streak: u8,
    own_scrim_reputation: u8,
    opponent_scrim_reputation: u8,
) -> bool {
    !won && (severity >= 3
        || own_loss_streak >= 3
        || own_scrim_reputation >= opponent_scrim_reputation.saturating_add(10))
}

fn daily_slot_position(
    team: &olm_core::domain::team::Team,
    current_weekday: u8,
    slot_index: u8,
) -> Option<usize> {
    let slot_days = scrim_slot_weekdays(effective_scrim_slots(
        team.scrim_weekly_slots,
        &team.training_schedule,
    ));
    let todays_slot_indices: Vec<usize> = slot_days
        .iter()
        .enumerate()
        .filter(|(_, day)| **day == current_weekday)
        .map(|(index, _)| index)
        .collect();
    todays_slot_indices
        .iter()
        .position(|index| *index as u8 == slot_index)
}

fn quality_from_report(report: &olm_core::domain::team::ScrimReport) -> ScrimResultQuality {
    if report.won.unwrap_or(false) {
        ScrimResultQuality::Good
    } else {
        ScrimResultQuality::Bad
    }
}

fn estimate_team_lol_ovr(game: &Game, team_id: &str) -> u8 {
    let mut ovrs: Vec<u8> = game
        .players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(team_id))
        .map(|player| olm_core::potential::calculate_lol_ovr(player))
        .collect();
    if ovrs.is_empty() {
        return 74;
    }
    ovrs.sort_by(|a, b| b.cmp(a));
    let sample = ovrs.iter().take(5).copied().collect::<Vec<_>>();
    let sum: u32 = sample.iter().map(|v| u32::from(*v)).sum();
    (sum / sample.len() as u32) as u8
}

fn apply_post_scrim_decision_internal(
    game: &mut Game,
    manager_team_id: &str,
    slot_index: u8,
    decision: olm_core::domain::team::PostScrimDecision,
) -> Result<(), String> {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let week_key = format!(
        "{}-W{}",
        game.clock.current_date.iso_week().year(),
        game.clock.current_date.iso_week().week()
    );
    let current_weekday = game.clock.current_date.weekday().num_days_from_monday() as u8;
    let (picks, won, severity, quality, opponent_team_id, own_scrim_reputation, own_loss_streak) = {
        let team = game
            .teams
            .iter_mut()
            .find(|team| team.id == manager_team_id)
            .ok_or("Manager team not found".to_string())?;
        let report_index = team
            .scrim_reports
            .iter()
            .position(|report| {
                report.date == today
                    && report.slot_index == slot_index
                    && report.post_decision.is_none()
            })
            .ok_or("No unresolved scrim report found for this slot".to_string())?;

        {
            let report = team
                .scrim_reports
                .get_mut(report_index)
                .ok_or("No unresolved scrim report found for this slot".to_string())?;

            report.post_decision = Some(decision.clone());
            match decision {
                olm_core::domain::team::PostScrimDecision::ContinuePlan => {
                    report.quality = report.quality.saturating_add(2).min(100);
                }
                olm_core::domain::team::PostScrimDecision::VodReview => {
                    report.quality = report.quality.saturating_add(6).min(100);
                    report.severity = report.severity.saturating_sub(1);
                }
                olm_core::domain::team::PostScrimDecision::MentalReset => {
                    report.severity = report.severity.saturating_sub(2);
                }
                olm_core::domain::team::PostScrimDecision::TargetedDrills => {
                    report.quality = report.quality.saturating_add(10).min(100);
                }
                olm_core::domain::team::PostScrimDecision::PushThrough => {
                    report.quality = report.quality.saturating_add(12).min(100);
                }
                olm_core::domain::team::PostScrimDecision::DayOff => {
                    report.severity = report.severity.saturating_sub(2);
                }
            }
        }

        let (report_picks, report_won, report_severity, report_quality, report_opponent_team_id) = {
            let report = team
                .scrim_reports
                .get(report_index)
                .ok_or("No unresolved scrim report found for this slot".to_string())?;
            (
                report.player_champion_picks.clone(),
                report.won.unwrap_or(false),
                report.severity,
                report.quality,
                report.opponent_team_id.clone(),
            )
        };

        // E8: first daily block decisions other than PushThrough cancel the next daily block.
        if decision != olm_core::domain::team::PostScrimDecision::PushThrough
            && decision != olm_core::domain::team::PostScrimDecision::ContinuePlan
        {
            let slot_days = scrim_slot_weekdays(effective_scrim_slots(
                team.scrim_weekly_slots,
                &team.training_schedule,
            ));
            let todays_slot_indices: Vec<usize> = slot_days
                .iter()
                .enumerate()
                .filter(|(_, day)| **day == current_weekday)
                .map(|(index, _)| index)
                .collect();
            let current_position = todays_slot_indices
                .iter()
                .position(|index| *index as u8 == slot_index);

            if let Some(0) = current_position {
                if let Some(next_slot_index) = todays_slot_indices.get(1).copied() {
                    let already_resolved_next = team.scrim_reports.iter().any(|entry| {
                        entry.date == today && entry.slot_index == next_slot_index as u8
                    });
                    if !already_resolved_next {
                        if let Some(next_opponent) =
                            team.weekly_scrim_opponent_ids.get_mut(next_slot_index)
                        {
                            *next_opponent = String::new();
                        }
                        if let Some(next_plan) =
                            team.weekly_scrim_plan_team_ids.get_mut(next_slot_index)
                        {
                            next_plan.clear();
                        }
                        team.scrim_weekly_cancellations =
                            team.scrim_weekly_cancellations.saturating_add(1);
                        team.scrim_reputation = team.scrim_reputation.saturating_sub(5);
                    } else {
                        // If next block was already simulated, convert this choice into a hard cancel of that block.
                        if let Some(remove_index) = team.scrim_reports.iter().position(|entry| {
                            entry.date == today
                                && entry.slot_index == next_slot_index as u8
                                && entry.post_decision.is_none()
                        }) {
                            let removed = team.scrim_reports.remove(remove_index);
                            team.scrim_weekly_played = team.scrim_weekly_played.saturating_sub(1);
                            if removed.won.unwrap_or(false) {
                                team.scrim_weekly_wins = team.scrim_weekly_wins.saturating_sub(1);
                            } else {
                                team.scrim_weekly_losses =
                                    team.scrim_weekly_losses.saturating_sub(1);
                            }
                            team.scrim_slot_results.retain(|entry| {
                                !(entry.week_key == week_key
                                    && entry.slot_index == next_slot_index as u8)
                            });
                            if let Some(next_opponent) =
                                team.weekly_scrim_opponent_ids.get_mut(next_slot_index)
                            {
                                *next_opponent = String::new();
                            }
                            if let Some(next_plan) =
                                team.weekly_scrim_plan_team_ids.get_mut(next_slot_index)
                            {
                                next_plan.clear();
                            }
                            team.scrim_weekly_cancellations =
                                team.scrim_weekly_cancellations.saturating_add(1);
                            team.scrim_reputation = team.scrim_reputation.saturating_sub(5);
                        }
                    }
                }
            }
        }

        (
            report_picks,
            report_won,
            report_severity,
            report_quality,
            report_opponent_team_id,
            team.scrim_reputation,
            team.scrim_loss_streak,
        )
    };

    let opponent_scrim_reputation = game
        .teams
        .iter()
        .find(|team| team.id == opponent_team_id)
        .map(|team| team.scrim_reputation)
        .unwrap_or(50);

    let severe_or_context_push = !won
        && (severity >= 3
            || own_loss_streak >= 3
            || own_scrim_reputation >= opponent_scrim_reputation.saturating_add(10));

    for pick in &picks {
        let Some(player) = game
            .players
            .iter_mut()
            .find(|player| player.id == pick.player_id)
        else {
            continue;
        };

        match decision {
            olm_core::domain::team::PostScrimDecision::ContinuePlan => {
                player.morale = player.morale.saturating_add(1).min(100);
            }
            olm_core::domain::team::PostScrimDecision::VodReview => {
                player.morale = player.morale.saturating_add(1).min(100);
                player.condition = player.condition.saturating_sub(1);
            }
            olm_core::domain::team::PostScrimDecision::MentalReset => {
                player.morale = player.morale.saturating_add(4).min(100);
                player.condition = player.condition.saturating_add(3).min(100);
            }
            olm_core::domain::team::PostScrimDecision::TargetedDrills => {
                player.condition = player.condition.saturating_sub(3);
            }
            olm_core::domain::team::PostScrimDecision::PushThrough => {
                player.condition =
                    player
                        .condition
                        .saturating_sub(if severe_or_context_push { 8 } else { 6 });
                if severe_or_context_push {
                    player.morale = player.morale.saturating_sub(2);
                } else if !won && severity >= 3 {
                    player.morale = player.morale.saturating_sub(1);
                }
            }
            olm_core::domain::team::PostScrimDecision::DayOff => {
                player.morale = player.morale.saturating_add(5).min(100);
                player.condition = player.condition.saturating_add(6).min(100);
            }
        }
    }

    for pick in &picks {
        champions::apply_scrim_mastery_progress(
            game,
            &pick.player_id,
            &pick.champion_id,
            quality,
            won,
            Some(&decision),
        );
    }

    Ok(())
}

#[tauri::command]
pub fn set_active_lineup(
    state: State<'_, StateManager>,
    player_ids: Vec<String>,
) -> Result<Game, String> {
    info!("[cmd] set_active_lineup: {} players", player_ids.len());
    set_active_lineup_internal(&state, player_ids)
}

#[tauri::command]
pub fn set_starting_xi(
    state: State<'_, StateManager>,
    player_ids: Vec<String>,
) -> Result<Game, String> {
    info!("[cmd] set_starting_xi is deprecated; use set_active_lineup");
    set_active_lineup_internal(&state, player_ids)
}

fn set_active_lineup_internal(
    state: &State<'_, StateManager>,
    player_ids: Vec<String>,
) -> Result<Game, String> {
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    apply_active_lineup(&mut game, &team_id, player_ids);

    state.set_game(game.clone());
    Ok(game)
}

fn apply_active_lineup(game: &mut Game, team_id: &str, player_ids: Vec<String>) {
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) {
        team.active_lineup_ids = player_ids;
    }
}

#[tauri::command]
pub fn set_draft_strategy(
    state: State<'_, StateManager>,
    draft_strategy: String,
) -> Result<Game, String> {
    info!("[cmd] set_draft_strategy: {}", draft_strategy);
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let strategy = match draft_strategy.as_str() {
        "Attacking" | "HighPress" => olm_core::domain::team::DraftStrategy::Aggressive,
        "Defensive" => olm_core::domain::team::DraftStrategy::Passive,
        "Possession" => olm_core::domain::team::DraftStrategy::Scaling,
        "Counter" => olm_core::domain::team::DraftStrategy::CounterPick,
        _ => olm_core::domain::team::DraftStrategy::Balanced,
    };

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) {
        team.draft_strategy = strategy;
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn set_lol_tactics(
    state: State<'_, StateManager>,
    lol_tactics: olm_core::domain::team::LolTactics,
) -> Result<Game, String> {
    info!("[cmd] set_lol_tactics");
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) {
        team.lol_tactics = lol_tactics;
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn set_team_roles(
    state: State<'_, StateManager>,
    team_roles: olm_core::domain::team::TeamRoles,
) -> Result<Game, String> {
    info!("[cmd] set_team_roles");
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) {
        team.team_roles = team_roles;
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn set_training(
    state: State<'_, StateManager>,
    focus: String,
    intensity: String,
) -> Result<Game, String> {
    info!(
        "[cmd] set_training: focus={}, intensity={}",
        focus, intensity
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let training_focus = olm_core::domain::team::TrainingFocus::from_id(&focus).unwrap_or_default();

    let training_intensity = match intensity.as_str() {
        "Low" => olm_core::domain::team::TrainingIntensity::Low,
        "Medium" => olm_core::domain::team::TrainingIntensity::Medium,
        "High" => olm_core::domain::team::TrainingIntensity::High,
        _ => olm_core::domain::team::TrainingIntensity::Medium,
    };

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) {
        team.training_focus = training_focus;
        team.training_intensity = training_intensity;
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn set_training_schedule(
    state: State<'_, StateManager>,
    schedule: String,
) -> Result<Game, String> {
    info!("[cmd] set_training_schedule: {}", schedule);
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    olm_core::commands::set_training_schedule(&mut game, &team_id, &schedule);

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn set_training_groups(
    state: State<'_, StateManager>,
    groups: Vec<olm_core::domain::team::TrainingGroup>,
) -> Result<Game, String> {
    info!("[cmd] set_training_groups: {} groups", groups.len());
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) {
        team.training_groups = groups;
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn set_weekly_scrims(
    state: State<'_, StateManager>,
    opponent_team_ids: Vec<String>,
) -> Result<Game, String> {
    info!(
        "[cmd] set_weekly_scrims: {} opponents",
        opponent_team_ids.len()
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let known_team_ids: std::collections::HashSet<String> =
        game.teams.iter().map(|team| team.id.clone()).collect();

    let my_competition_id: Option<String> = game
        .teams
        .iter()
        .find(|t| t.id == manager_team_id)
        .and_then(|t| t.competition_id.clone());
    let same_competition_team_ids: std::collections::HashSet<String> = game
        .teams
        .iter()
        .filter(|t| t.competition_id == my_competition_id)
        .map(|t| t.id.clone())
        .collect();

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == manager_team_id) {
        let slot_days = scrim_slot_weekdays(effective_scrim_slots(
            team.scrim_weekly_slots,
            &team.training_schedule,
        ));
        let current_weekday = game.clock.current_date.weekday().num_days_from_monday() as u8;
        let week_key = format!(
            "{}-W{}",
            game.clock.current_date.iso_week().year(),
            game.clock.current_date.iso_week().week()
        );
        let (setup_locked, _) =
            weekly_scrim_setup_lock_state(team, &week_key, current_weekday, game.day_phase.clone());
        if setup_locked {
            return Err("Weekly scrim setup is locked for this week".to_string());
        }
        let mut next_slots: Vec<String> = vec![String::new(); slot_days.len()];
        let previous_slots = team.weekly_scrim_opponent_ids.clone();

        for (index, day) in slot_days.iter().enumerate() {
            let already_simulated = team
                .scrim_slot_results
                .iter()
                .any(|entry| entry.week_key == week_key && entry.slot_index == index as u8);
            if *day < current_weekday || already_simulated {
                next_slots[index] = previous_slots.get(index).cloned().unwrap_or_default();
                continue;
            }

            let candidate = opponent_team_ids.get(index).cloned().unwrap_or_default();
            if candidate.is_empty() {
                next_slots[index] = String::new();
                continue;
            }
            if candidate == team.id {
                continue;
            }
            if !known_team_ids.contains(&candidate) {
                continue;
            }
            if !same_competition_team_ids.contains(&candidate) {
                continue;
            }
            next_slots[index] = candidate;
        }

        team.weekly_scrim_opponent_ids = next_slots;
        team.weekly_scrim_plan_team_ids = team
            .weekly_scrim_opponent_ids
            .iter()
            .map(|team_id| {
                if team_id.is_empty() {
                    Vec::new()
                } else {
                    vec![team_id.clone()]
                }
            })
            .collect();
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn set_weekly_scrim_plans(
    state: State<'_, StateManager>,
    plans: Vec<Vec<String>>,
) -> Result<Game, String> {
    info!("[cmd] set_weekly_scrim_plans: {} slots", plans.len());
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let known_team_ids: std::collections::HashSet<String> =
        game.teams.iter().map(|team| team.id.clone()).collect();

    let my_competition_id: Option<String> = game
        .teams
        .iter()
        .find(|t| t.id == manager_team_id)
        .and_then(|t| t.competition_id.clone());
    let same_competition_team_ids: std::collections::HashSet<String> = game
        .teams
        .iter()
        .filter(|t| t.competition_id == my_competition_id)
        .map(|t| t.id.clone())
        .collect();

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == manager_team_id) {
        let slot_days = scrim_slot_weekdays(effective_scrim_slots(
            team.scrim_weekly_slots,
            &team.training_schedule,
        ));
        let current_weekday = game.clock.current_date.weekday().num_days_from_monday() as u8;
        let week_key = format!(
            "{}-W{}",
            game.clock.current_date.iso_week().year(),
            game.clock.current_date.iso_week().week()
        );
        let (setup_locked, _) =
            weekly_scrim_setup_lock_state(team, &week_key, current_weekday, game.day_phase.clone());
        if setup_locked {
            return Err("Weekly scrim setup is locked for this week".to_string());
        }
        let previous_plans = team.weekly_scrim_plan_team_ids.clone();
        let mut next_plans: Vec<Vec<String>> = vec![Vec::new(); slot_days.len()];

        for (index, day) in slot_days.iter().enumerate() {
            let already_simulated = team
                .scrim_slot_results
                .iter()
                .any(|entry| entry.week_key == week_key && entry.slot_index == index as u8);
            if *day < current_weekday || already_simulated {
                next_plans[index] = previous_plans.get(index).cloned().unwrap_or_default();
                continue;
            }

            let mut seen = std::collections::HashSet::new();
            next_plans[index] = plans
                .get(index)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|candidate| !candidate.is_empty())
                .filter(|candidate| candidate != &team.id)
                .filter(|candidate| known_team_ids.contains(candidate))
                .filter(|candidate| same_competition_team_ids.contains(candidate))
                .filter(|candidate| seen.insert(candidate.clone()))
                .take(3)
                .collect();
        }

        team.weekly_scrim_opponent_ids = next_plans
            .iter()
            .map(|plan| plan.first().cloned().unwrap_or_default())
            .collect();
        team.weekly_scrim_plan_team_ids = next_plans;
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn set_weekly_scrim_slots(state: State<'_, StateManager>, slots: u8) -> Result<Game, String> {
    info!("[cmd] set_weekly_scrim_slots: {}", slots);
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == manager_team_id) {
        let current_weekday = game.clock.current_date.weekday().num_days_from_monday() as u8;
        let week_key = format!(
            "{}-W{}",
            game.clock.current_date.iso_week().year(),
            game.clock.current_date.iso_week().week()
        );
        let (setup_locked, _) =
            weekly_scrim_setup_lock_state(team, &week_key, current_weekday, game.day_phase.clone());
        if setup_locked {
            return Err("Weekly scrim setup is locked for this week".to_string());
        }
        let effective_slots = effective_scrim_slots(slots, &team.training_schedule);
        team.scrim_weekly_slots = effective_slots;
        team.weekly_scrim_opponent_ids
            .truncate(effective_slots as usize);
        team.weekly_scrim_plan_team_ids
            .truncate(effective_slots as usize);
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn set_weekly_scrim_objective(
    state: State<'_, StateManager>,
    objective: Option<String>,
) -> Result<Game, String> {
    info!("[cmd] set_weekly_scrim_objective: {:?}", objective);
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let parsed = objective
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(parse_scrim_focus)
        .transpose()?;

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == manager_team_id) {
        let current_weekday = game.clock.current_date.weekday().num_days_from_monday() as u8;
        let week_key = format!(
            "{}-W{}",
            game.clock.current_date.iso_week().year(),
            game.clock.current_date.iso_week().week()
        );
        let (setup_locked, _) =
            weekly_scrim_setup_lock_state(team, &week_key, current_weekday, game.day_phase.clone());
        if setup_locked {
            return Err("Weekly scrim setup is locked for this week".to_string());
        }
        team.scrim_weekly_objective = parsed;
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn auto_configure_weekly_scrim_setup(state: State<'_, StateManager>) -> Result<Game, String> {
    info!("[cmd] auto_configure_weekly_scrim_setup");
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let current_weekday = game.clock.current_date.weekday().num_days_from_monday() as u8;
    let week_key = format!(
        "{}-W{}",
        game.clock.current_date.iso_week().year(),
        game.clock.current_date.iso_week().week()
    );

    let known_team_ids: std::collections::HashSet<String> =
        game.teams.iter().map(|team| team.id.clone()).collect();

    let my_competition_id: Option<String> = game
        .teams
        .iter()
        .find(|t| t.id == manager_team_id)
        .and_then(|t| t.competition_id.clone());
    let same_competition_team_ids: std::collections::HashSet<String> = game
        .teams
        .iter()
        .filter(|t| t.competition_id == my_competition_id)
        .map(|t| t.id.clone())
        .collect();

    let own_ovr = estimate_team_lol_ovr(&game, &manager_team_id);
    let mut rivals_by_strength: Vec<(String, u8)> = game
        .teams
        .iter()
        .filter(|team| team.id != manager_team_id)
        .filter(|team| same_competition_team_ids.contains(&team.id))
        .map(|team| (team.id.clone(), estimate_team_lol_ovr(&game, &team.id)))
        .filter(|(id, _)| known_team_ids.contains(id))
        .collect();
    rivals_by_strength.sort_by(|a, b| b.1.cmp(&a.1));

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == manager_team_id) {
        let (setup_locked, _) =
            weekly_scrim_setup_lock_state(team, &week_key, current_weekday, game.day_phase.clone());
        if setup_locked {
            return Ok(game);
        }

        let effective_slots =
            effective_scrim_slots(team.scrim_weekly_slots, &team.training_schedule);
        team.scrim_weekly_slots = effective_slots;

        if team.scrim_weekly_objective.is_none() {
            team.scrim_weekly_objective = Some(if team.scrim_loss_streak >= 3 {
                olm_core::domain::team::ScrimFocus::Mental
            } else if own_ovr >= 80 {
                olm_core::domain::team::ScrimFocus::DraftPrep
            } else if own_ovr >= 77 {
                olm_core::domain::team::ScrimFocus::Macro
            } else {
                olm_core::domain::team::ScrimFocus::ChampionPool
            });
        }

        let objective = team
            .scrim_weekly_objective
            .clone()
            .unwrap_or(olm_core::domain::team::ScrimFocus::ChampionPool);

        let pool: Vec<String> = match objective {
            olm_core::domain::team::ScrimFocus::Mental => rivals_by_strength
                .iter()
                .rev()
                .map(|(id, _)| id.clone())
                .collect(),
            olm_core::domain::team::ScrimFocus::ChampionPool
            | olm_core::domain::team::ScrimFocus::EarlyGame => {
                let split = (rivals_by_strength.len() / 3).max(1);
                rivals_by_strength
                    .iter()
                    .skip(split)
                    .chain(rivals_by_strength.iter().take(split))
                    .map(|(id, _)| id.clone())
                    .collect()
            }
            _ => rivals_by_strength
                .iter()
                .map(|(id, _)| id.clone())
                .collect(),
        };

        let slot_count = effective_slots as usize;
        let mut plans: Vec<Vec<String>> = vec![Vec::new(); slot_count];
        for slot_index in 0..slot_count {
            if pool.is_empty() {
                break;
            }
            let a = pool[slot_index % pool.len()].clone();
            let b = pool[(slot_index + 1) % pool.len()].clone();
            let c = pool[(slot_index + 2) % pool.len()].clone();
            let mut unique: Vec<String> = Vec::new();
            for candidate in [a, b, c] {
                if !unique.contains(&candidate) {
                    unique.push(candidate);
                }
            }
            plans[slot_index] = unique;
        }

        team.weekly_scrim_plan_team_ids = plans;
        team.weekly_scrim_opponent_ids = team
            .weekly_scrim_plan_team_ids
            .iter()
            .map(|plan| plan.first().cloned().unwrap_or_default())
            .collect();
        team.scrim_setup_locked_week_key = Some(week_key);
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn finalize_weekly_scrim_setup(state: State<'_, StateManager>) -> Result<Game, String> {
    info!("[cmd] finalize_weekly_scrim_setup");
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == manager_team_id) {
        let week_key = format!(
            "{}-W{}",
            game.clock.current_date.iso_week().year(),
            game.clock.current_date.iso_week().week()
        );
        team.scrim_setup_locked_week_key = Some(week_key);
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn cancel_todays_scrims(state: State<'_, StateManager>) -> Result<Game, String> {
    info!("[cmd] cancel_todays_scrims");
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    if let Some(team) = game.teams.iter_mut().find(|t| t.id == manager_team_id) {
        let slot_days = scrim_slot_weekdays(effective_scrim_slots(
            team.scrim_weekly_slots,
            &team.training_schedule,
        ));
        let current_weekday = game.clock.current_date.weekday().num_days_from_monday() as u8;
        let week_key = format!(
            "{}-W{}",
            game.clock.current_date.iso_week().year(),
            game.clock.current_date.iso_week().week()
        );
        let mut cancelled = 0_u8;

        for (index, day) in slot_days.iter().enumerate() {
            if *day != current_weekday {
                continue;
            }
            let already_simulated = team
                .scrim_slot_results
                .iter()
                .any(|entry| entry.week_key == week_key && entry.slot_index == index as u8);
            if already_simulated {
                continue;
            }

            if let Some(slot) = team.weekly_scrim_opponent_ids.get_mut(index) {
                *slot = String::new();
            }
            if let Some(plan) = team.weekly_scrim_plan_team_ids.get_mut(index) {
                plan.clear();
            }
            cancelled = cancelled.saturating_add(1);
        }

        if cancelled > 0 {
            team.scrim_weekly_cancellations =
                team.scrim_weekly_cancellations.saturating_add(cancelled);
            team.scrim_reputation = team.scrim_reputation.saturating_sub(5 * cancelled);
        }
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn choose_post_scrim_decision(
    state: State<'_, StateManager>,
    slot_index: u8,
    decision: String,
) -> Result<Game, String> {
    info!(
        "[cmd] choose_post_scrim_decision: slot={}, decision={}",
        slot_index, decision
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    if game.day_phase != DayPhase::ReviewBlock && game.day_phase != DayPhase::ScrimBlock {
        return Err(
            "Post-scrim decisions are only available during ScrimBlock/ReviewBlock".to_string(),
        );
    }

    let decision = parse_post_scrim_decision(&decision)?;
    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    if decision == olm_core::domain::team::PostScrimDecision::DayOff {
        let current_weekday = game.clock.current_date.weekday().num_days_from_monday() as u8;
        let team = game
            .teams
            .iter()
            .find(|team| team.id == manager_team_id)
            .ok_or("Manager team not found".to_string())?;
        let maybe_position = daily_slot_position(team, current_weekday, slot_index);
        if maybe_position != Some(1) {
            return Err("DayOff is only available after the second daily scrim block".to_string());
        }
    }

    apply_post_scrim_decision_internal(&mut game, &manager_team_id, slot_index, decision)?;

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn choose_daily_scrim_action(
    state: State<'_, StateManager>,
    slot_index: u8,
    action: String,
) -> Result<Game, String> {
    info!(
        "[cmd] choose_daily_scrim_action: slot={}, action={}",
        slot_index, action
    );
    let game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;
    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;
    let team = game
        .teams
        .iter()
        .find(|team| team.id == manager_team_id)
        .ok_or("Manager team not found".to_string())?;
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let current_weekday = game.clock.current_date.weekday().num_days_from_monday() as u8;
    let position = daily_slot_position(team, current_weekday, slot_index)
        .ok_or("Invalid slot for current day".to_string())?;
    let report = team
        .scrim_reports
        .iter()
        .find(|report| {
            report.date == today
                && report.slot_index == slot_index
                && report.post_decision.is_none()
        })
        .ok_or("No unresolved scrim report found for this slot".to_string())?;

    let state_for_action = match position {
        0 => match quality_from_report(report) {
            ScrimResultQuality::Good => DailyScrimFlowState::Block1GoodDecision,
            ScrimResultQuality::Bad => DailyScrimFlowState::Block1BadDecision,
        },
        1 => match quality_from_report(report) {
            ScrimResultQuality::Good => DailyScrimFlowState::Block2GoodDecision,
            ScrimResultQuality::Bad => DailyScrimFlowState::Block2BadDecision,
        },
        _ => return Err("Only two daily scrim blocks are supported".to_string()),
    };

    let event = match action.as_str() {
        "ContinueToBlock2" => DailyScrimFlowEvent::ContinueToBlock2,
        "OfferRest" => DailyScrimFlowEvent::OfferRest,
        "DayOff" => DailyScrimFlowEvent::DayOff,
        "PushThrough" => DailyScrimFlowEvent::PushThrough,
        "CancelScrims" => DailyScrimFlowEvent::CancelScrims,
        "VodReview" => DailyScrimFlowEvent::VodReview,
        "MentalReset" => DailyScrimFlowEvent::MentalReset,
        "TargetedDrills" => DailyScrimFlowEvent::TargetedDrills,
        _ => return Err(format!("Unknown daily scrim action: {action}")),
    };
    transition_daily_scrim_flow(state_for_action, event)?;

    if action == "CancelScrims" {
        return Ok(game);
    }

    let decision = match action.as_str() {
        "ContinueToBlock2" => "ContinuePlan",
        "OfferRest" | "DayOff" => "DayOff",
        "PushThrough" => "PushThrough",
        "VodReview" => "VodReview",
        "MentalReset" => "MentalReset",
        "TargetedDrills" => "TargetedDrills",
        _ => return Err(format!("Unknown daily scrim action: {action}")),
    };

    let mut updated = choose_post_scrim_decision(state.clone(), slot_index, decision.to_string())?;

    if action == "ContinueToBlock2" || action == "PushThrough" {
        let weekday_num = updated.clock.current_date.weekday().num_days_from_monday();
        olm_core::training::process_scrim_block(&mut updated, weekday_num);
        state.set_game(updated.clone());
    }

    Ok(updated)
}

#[tauri::command]
pub fn delegate_scrim_decision(state: State<'_, StateManager>) -> Result<Game, String> {
    info!("[cmd] delegate_scrim_decision");
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    if game.day_phase != DayPhase::ReviewBlock && game.day_phase != DayPhase::ScrimBlock {
        return Err("Delegation is only available during ScrimBlock/ReviewBlock".to_string());
    }

    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();

    let (slot_index, won, severity, issue, own_rep, own_loss_streak, opponent_id) = {
        let team = game
            .teams
            .iter()
            .find(|team| team.id == manager_team_id)
            .ok_or("Manager team not found".to_string())?;
        let report = team
            .scrim_reports
            .iter()
            .filter(|report| report.date == today && report.post_decision.is_none())
            .min_by_key(|report| report.slot_index)
            .ok_or("No unresolved scrim report found for delegation".to_string())?;
        (
            report.slot_index,
            report.won.unwrap_or(false),
            report.severity,
            report.issue.clone(),
            team.scrim_reputation,
            team.scrim_loss_streak,
            report.opponent_team_id.clone(),
        )
    };

    let opponent_rep = game
        .teams
        .iter()
        .find(|team| team.id == opponent_id)
        .map(|team| team.scrim_reputation)
        .unwrap_or(50);

    let decision = if !won
        && (severity >= 3 || own_loss_streak >= 3 || own_rep >= opponent_rep.saturating_add(10))
    {
        olm_core::domain::team::PostScrimDecision::MentalReset
    } else if matches!(
        issue,
        Some(
            olm_core::domain::team::ScrimIssue::ObjectiveSetup
                | olm_core::domain::team::ScrimIssue::DraftGap
        )
    ) {
        olm_core::domain::team::PostScrimDecision::VodReview
    } else if matches!(
        issue,
        Some(
            olm_core::domain::team::ScrimIssue::ChampionComfort
                | olm_core::domain::team::ScrimIssue::LanePressure
        )
    ) {
        olm_core::domain::team::PostScrimDecision::TargetedDrills
    } else {
        olm_core::domain::team::PostScrimDecision::PushThrough
    };

    apply_post_scrim_decision_internal(&mut game, &manager_team_id, slot_index, decision)?;
    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn get_scrim_context(state: State<'_, StateManager>) -> Result<ScrimContextResponse, String> {
    info!("[cmd] get_scrim_context");
    let game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let team = game
        .teams
        .iter()
        .find(|candidate| candidate.id == manager_team_id)
        .ok_or("Manager team not found".to_string())?;

    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let day_phase = game.day_phase.as_id();
    let current_weekday = game.clock.current_date.weekday().num_days_from_monday() as u8;
    let capacity = effective_scrim_slots(team.scrim_weekly_slots, &team.training_schedule);
    let weekdays = scrim_slot_weekdays(capacity);
    let slot_index = weekdays
        .iter()
        .position(|weekday| *weekday == current_weekday);
    let week_key = format!(
        "{}-W{}",
        game.clock.current_date.iso_week().year(),
        game.clock.current_date.iso_week().week()
    );

    let has_official_match = game
        .leagues
        .first()
        .map(|league| {
            league.fixtures.iter().any(|fixture| {
                fixture.status == olm_core::domain::league::FixtureStatus::Scheduled
                    && fixture.date.get(0..10).unwrap_or_default() == today
                    && (fixture.home_team_id == team.id || fixture.away_team_id == team.id)
            })
        })
        .unwrap_or(false);

    let mut today_reports: Vec<olm_core::domain::team::ScrimReport> = team
        .scrim_reports
        .iter()
        .filter(|report| report.date == today)
        .cloned()
        .collect();
    today_reports.sort_by(|left, right| left.slot_index.cmp(&right.slot_index));
    let unresolved_report = today_reports
        .iter()
        .find(|report| report.post_decision.is_none())
        .cloned();
    let reviewed_report = today_reports
        .iter()
        .find(|report| report.post_decision.is_some())
        .cloned();

    let today_context = if let Some(report) = unresolved_report.clone() {
        let decision_phase_active = day_phase == "ScrimBlock";
        let report_opponent_team_id = report.opponent_team_id.clone();
        TodayScrimContextResponse {
            state: "PlayedNeedsReview".to_string(),
            slot_index: Some(report.slot_index),
            opponent_team_id: Some(report.opponent_team_id.clone()),
            resolved_opponent_team_id: Some(report.opponent_team_id.clone()),
            objective: team.scrim_weekly_objective.clone(),
            report: Some(report.clone()),
            can_edit_plan: false,
            can_cancel: false,
            can_review: decision_phase_active,
            can_view_weekly_plan: true,
            has_official_match,
            primary_action: Some(if decision_phase_active {
                "Review".to_string()
            } else if has_official_match {
                "Schedule".to_string()
            } else {
                "Training".to_string()
            }),
            push_through_recommended: is_push_through_recommended(
                report.won.unwrap_or(false),
                report.severity,
                team.scrim_loss_streak,
                team.scrim_reputation,
                game.teams
                    .iter()
                    .find(|candidate| candidate.id == report_opponent_team_id)
                    .map(|candidate| candidate.scrim_reputation)
                    .unwrap_or(50),
            ),
        }
    } else if let Some(report) = reviewed_report.clone() {
        TodayScrimContextResponse {
            state: "Reviewed".to_string(),
            slot_index: Some(report.slot_index),
            opponent_team_id: Some(report.opponent_team_id.clone()),
            resolved_opponent_team_id: Some(report.opponent_team_id.clone()),
            objective: team.scrim_weekly_objective.clone(),
            report: Some(report),
            can_edit_plan: false,
            can_cancel: false,
            can_review: false,
            can_view_weekly_plan: true,
            has_official_match,
            primary_action: Some(if has_official_match {
                "Schedule".to_string()
            } else {
                "Training".to_string()
            }),
            push_through_recommended: false,
        }
    } else if let Some(slot_index) = slot_index {
        let plan = team
            .weekly_scrim_plan_team_ids
            .get(slot_index)
            .cloned()
            .unwrap_or_default();
        let opponent = plan
            .iter()
            .find(|candidate| !candidate.is_empty())
            .cloned()
            .or_else(|| {
                team.weekly_scrim_opponent_ids
                    .get(slot_index)
                    .filter(|candidate| !candidate.is_empty())
                    .cloned()
            });
        let is_planned = opponent.is_some() || day_phase == "Morning";
        let can_cancel = is_planned && day_phase == "Morning";

        TodayScrimContextResponse {
            state: if is_planned {
                "Planned".to_string()
            } else {
                "Cancelled".to_string()
            },
            slot_index: Some(slot_index as u8),
            opponent_team_id: opponent,
            resolved_opponent_team_id: None,
            objective: team.scrim_weekly_objective.clone(),
            report: None,
            can_edit_plan: day_phase == "Morning",
            can_cancel,
            can_review: false,
            can_view_weekly_plan: true,
            has_official_match,
            primary_action: Some(if is_planned {
                "OpenPlan".to_string()
            } else if has_official_match {
                "Schedule".to_string()
            } else {
                "Training".to_string()
            }),
            push_through_recommended: false,
        }
    } else {
        TodayScrimContextResponse {
            state: "NoScrimToday".to_string(),
            slot_index: None,
            opponent_team_id: None,
            resolved_opponent_team_id: None,
            objective: team.scrim_weekly_objective.clone(),
            report: None,
            can_edit_plan: false,
            can_cancel: false,
            can_review: false,
            can_view_weekly_plan: true,
            has_official_match,
            primary_action: Some(if has_official_match {
                "Schedule".to_string()
            } else {
                "Training".to_string()
            }),
            push_through_recommended: false,
        }
    };

    let (setup_locked, setup_locked_reason) =
        weekly_scrim_setup_lock_state(team, &week_key, current_weekday, game.day_phase.clone());

    let slots: Vec<WeeklyScrimSlotContextResponse> = (0..capacity as usize)
        .map(|index| {
            let plan = team
                .weekly_scrim_plan_team_ids
                .get(index)
                .cloned()
                .unwrap_or_default();
            let merged_plan = if !plan.is_empty() {
                plan
            } else {
                team.weekly_scrim_opponent_ids
                    .get(index)
                    .filter(|opponent| !opponent.is_empty())
                    .map(|opponent| vec![opponent.clone()])
                    .unwrap_or_default()
            };
            let report = team
                .scrim_reports
                .iter()
                .find(|entry| entry.week_key == week_key && entry.slot_index == index as u8)
                .cloned();
            let result = team
                .scrim_slot_results
                .iter()
                .find(|entry| entry.week_key == week_key && entry.slot_index == index as u8)
                .cloned();
            let has_past_lock = weekdays.get(index).copied().unwrap_or(0) < current_weekday;
            let status = if report
                .as_ref()
                .and_then(|entry| entry.post_decision.as_ref())
                .is_some()
            {
                "Reviewed"
            } else if report.is_some() || result.is_some() {
                "Played"
            } else if merged_plan.is_empty() && has_past_lock {
                "Cancelled"
            } else if has_past_lock {
                "Locked"
            } else {
                "Open"
            };
            let (label_day, label_suffix) = slot_label_parts(&weekdays, index);

            WeeklyScrimSlotContextResponse {
                slot_index: index as u8,
                weekday: weekdays.get(index).copied().unwrap_or(0),
                label: if label_suffix.is_empty() {
                    format!("{}", label_day)
                } else {
                    format!("{} {}", label_day, label_suffix)
                },
                label_day,
                label_suffix,
                plan: merged_plan,
                resolved_opponent_team_id: report
                    .as_ref()
                    .map(|entry| entry.opponent_team_id.clone())
                    .or_else(|| result.as_ref().map(|entry| entry.opponent_team_id.clone())),
                result_won: report
                    .as_ref()
                    .and_then(|entry| entry.won)
                    .or_else(|| result.as_ref().map(|entry| entry.won)),
                report,
                status: status.to_string(),
                can_edit: !setup_locked
                    && !has_past_lock
                    && team.scrim_reports.iter().all(|entry| {
                        !(entry.week_key == week_key && entry.slot_index == index as u8)
                    })
                    && team.scrim_slot_results.iter().all(|entry| {
                        !(entry.week_key == week_key && entry.slot_index == index as u8)
                    }),
            }
        })
        .collect();

    let mut latest_reports: Vec<olm_core::domain::team::ScrimReport> = team
        .scrim_reports
        .iter()
        .filter(|report| report.week_key == week_key)
        .cloned()
        .collect();
    latest_reports.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then(right.slot_index.cmp(&left.slot_index))
    });
    let played_reports: Vec<olm_core::domain::team::ScrimReport> = latest_reports
        .iter()
        .filter(|report| report.status == olm_core::domain::team::ScrimStatus::Played)
        .cloned()
        .collect();

    let mut issue_counts: Vec<(olm_core::domain::team::ScrimIssue, usize)> = Vec::new();
    for report in &played_reports {
        let Some(issue) = report.issue.clone() else {
            continue;
        };
        if let Some((_, count)) = issue_counts
            .iter_mut()
            .find(|(candidate, _)| candidate == &issue)
        {
            *count += 1;
        } else {
            issue_counts.push((issue, 1));
        }
    }
    let top_issue = issue_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(issue, _)| issue);

    let next_official_fixture = game.active_league().and_then(|league| {
        let mut fixtures: Vec<&olm_core::domain::league::Fixture> = league
            .fixtures
            .iter()
            .filter(|fixture| {
                fixture.status == olm_core::domain::league::FixtureStatus::Scheduled
                    && (fixture.home_team_id == team.id || fixture.away_team_id == team.id)
                    && fixture.date >= game.clock.current_date.to_rfc3339()
            })
            .collect();
        fixtures.sort_by(|left, right| left.date.cmp(&right.date));
        fixtures.into_iter().next()
    });

    let weekly_context = WeeklyScrimContextResponse {
        week_key: week_key.clone(),
        objective: team.scrim_weekly_objective.clone(),
        capacity,
        planned: slots
            .iter()
            .filter(|slot| !slot.plan.is_empty() || slot.resolved_opponent_team_id.is_some())
            .count() as u8,
        reputation: team.scrim_reputation,
        cancellations: team.scrim_weekly_cancellations,
        played: team.scrim_weekly_played,
        wins: team.scrim_weekly_wins,
        losses: team.scrim_weekly_losses,
        loss_streak: team.scrim_loss_streak,
        avg_quality: if played_reports.is_empty() {
            0
        } else {
            (played_reports
                .iter()
                .map(|report| report.quality as u32)
                .sum::<u32>()
                / played_reports.len() as u32) as u8
        },
        top_focus: played_reports.first().map(|report| report.focus.clone()),
        top_issue,
        next_official_rival_team_id: next_official_fixture.map(|fixture| {
            if fixture.home_team_id == team.id {
                fixture.away_team_id.clone()
            } else {
                fixture.home_team_id.clone()
            }
        }),
        next_official_rival_competition: next_official_fixture
            .map(|fixture| fixture.match_type.clone()),
        setup_locked,
        setup_locked_reason,
        can_finalize_setup: !setup_locked,
        slots,
        latest_reports,
    };

    Ok(ScrimContextResponse {
        today: today_context,
        week: weekly_context,
    })
}

#[tauri::command]
pub fn set_player_training_focus(
    state: State<'_, StateManager>,
    player_id: String,
    focus: Option<String>,
) -> Result<Game, String> {
    info!(
        "[cmd] set_player_training_focus: player={}, focus={:?}",
        player_id, focus
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let training_focus = focus.and_then(|f| olm_core::domain::team::TrainingFocus::from_id(&f));

    if let Some(player) = game.players.iter_mut().find(|p| p.id == player_id) {
        player.training_focus = training_focus;
    } else {
        return Err(format!("Player not found: {}", player_id));
    }

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn set_player_champion_training_target(
    state: State<'_, StateManager>,
    player_id: String,
    priority_index: u8,
    champion_id: Option<String>,
) -> Result<Game, String> {
    info!(
        "[cmd] set_player_champion_training_target: player={}, priority={}, champion={:?}",
        player_id, priority_index, champion_id
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    champions::set_player_training_target(
        &mut game,
        &player_id,
        usize::from(priority_index),
        champion_id,
    )?;

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn delegate_champion_training(state: State<'_, StateManager>) -> Result<Game, String> {
    info!("[cmd] delegate_champion_training");
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let updated = olm_core::champions::delegate_champion_training_to_coach(&mut game)?;
    info!(
        "[cmd] delegate_champion_training: updated {} players",
        updated
    );

    state.set_game(game.clone());
    Ok(game)
}

/// Current SoloQ standing (tier / LP / daily delta / mastery multiplier) for the
/// manager team's players. Single source of truth so the Meta and Training tabs
/// display the same value the simulation uses for mastery gains.
#[tauri::command]
pub fn get_soloq_statuses(
    state: State<'_, StateManager>,
) -> Result<Vec<champions::SoloQStatus>, String> {
    let game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let manager_team_id = game.manager.team_id.clone();
    let statuses = game
        .players
        .iter()
        .filter(|player| player.team_id == manager_team_id)
        .map(|player| champions::soloq_status_for_player(&game, player))
        .collect();

    Ok(statuses)
}

#[tauri::command]
pub fn start_potential_research(
    state: State<'_, StateManager>,
    player_id: String,
) -> Result<Game, String> {
    info!("[cmd] start_potential_research: player={}", player_id);
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    potential::start_potential_research(&mut game, &player_id)?;

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn reroll_player_lol_role(
    state: State<'_, StateManager>,
    player_id: String,
    role: String,
) -> Result<Game, String> {
    info!(
        "[cmd] reroll_player_lol_role: player={}, role={}",
        player_id, role
    );

    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let next_natural = match role.as_str() {
        "TOP" => olm_core::domain::player::LolRole::Top,
        "JUNGLE" => olm_core::domain::player::LolRole::Jungle,
        "MID" => olm_core::domain::player::LolRole::Mid,
        "ADC" => olm_core::domain::player::LolRole::Adc,
        "SUPPORT" => olm_core::domain::player::LolRole::Support,
        _ => return Err(format!("Unknown LoL role: {}", role)),
    };
    let next_position = next_natural; // In LoL, natural and current position are the same

    let player = game
        .players
        .iter_mut()
        .find(|candidate| candidate.id == player_id)
        .ok_or_else(|| format!("Player not found: {}", player_id))?;

    if player.team_id.as_deref() != Some(manager_team_id.as_str()) {
        return Err("Player does not belong to manager team".to_string());
    }

    let previous_natural = player.natural_position;

    if previous_natural != next_natural
        && !player
            .alternate_positions
            .iter()
            .any(|position| position == &previous_natural)
    {
        player.alternate_positions.push(previous_natural);
        if player.alternate_positions.len() > 4 {
            player.alternate_positions.truncate(4);
        }
    }

    player.natural_position = next_natural;
    player.position = next_position;

    state.set_game(game.clone());
    Ok(game)
}

#[tauri::command]
pub fn auto_select_team_roles(
    state: State<'_, StateManager>,
    player_ids: Vec<String>,
) -> Result<serde_json::Value, String> {
    log::debug!("[cmd] auto_select_team_roles: {} players", player_ids.len());
    let game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let (captain, shotcaller) =
        olm_core::live_match_manager::auto_select_team_roles(&game, &player_ids);

    Ok(serde_json::json!({
        "captain": captain,
        "shotcaller": shotcaller,
    }))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use olm_core::clock::GameClock;
    use olm_core::domain::manager::Manager;
    use olm_core::domain::player::{LolRole, Player, PlayerAttributes};
    use olm_core::domain::staff::{Staff, StaffAttributes, StaffRole};
    use olm_core::domain::team::{Team, TrainingFocus, TrainingIntensity, TrainingSchedule};
    use olm_core::game::Game;

    fn attrs(stat: u8) -> PlayerAttributes {
        PlayerAttributes {
            mechanics: stat,
            laning: stat,
            teamfighting: stat,
            macro_play: stat,
            consistency: stat,
            shotcalling: stat,
            champion_pool: stat,
            discipline: stat,
            mental_resilience: stat,
        }
    }

    fn make_player(id: &str, team_id: &str, stat: u8, potential_base: u8) -> Player {
        let mut player = Player::new(
            id.to_string(),
            format!("{}-name", id),
            format!("{} Full", id),
            "2005-01-01".to_string(),
            "GB".to_string(),
            LolRole::Jungle,
            attrs(stat),
        );
        player.team_id = Some(team_id.to_string());
        player.morale = 80;
        player.potential_base = potential_base;
        player
    }

    fn make_game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 1, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr-1".to_string(),
            "Alex".to_string(),
            "Coach".to_string(),
            "1980-01-01".to_string(),
            "GB".to_string(),
        );
        manager.hire("team-1".to_string());

        let mut team = Team::new(
            "team-1".to_string(),
            "Team One".to_string(),
            "ONE".to_string(),
            "England".to_string(),
            "London".to_string(),
            "Arena".to_string(),
            30_000,
        );
        team.training_focus = TrainingFocus::IndividualCoaching;
        team.training_intensity = TrainingIntensity::High;
        team.training_schedule = TrainingSchedule::Intense;

        let mut coach = Staff::new(
            "coach-1".to_string(),
            "Pat".to_string(),
            "Coach".to_string(),
            "1988-01-01".to_string(),
            StaffRole::Coach,
            StaffAttributes {
                coaching: 99,
                judging_ability: 50,
                judging_potential: 50,
                physiotherapy: 0,
            },
        );
        coach.nationality = "GB".to_string();
        coach.team_id = Some("team-1".to_string());

        Game::new(
            clock,
            manager,
            vec![team],
            vec![
                make_player("p1", "team-1", 82, 84),
                make_player("p2", "team-1", 78, 82),
            ],
            vec![coach],
            vec![],
        )
    }

    #[test]
    fn only_one_active_potential_research_at_a_time() {
        let mut game = make_game();
        olm_core::potential::start_potential_research(&mut game, "p1").unwrap();

        let second = olm_core::potential::start_potential_research(&mut game, "p2");
        assert!(second.is_err());
    }

    #[test]
    fn potential_research_completes_after_seven_days_and_clears_state() {
        let mut game = make_game();
        olm_core::potential::start_potential_research(&mut game, "p1").unwrap();

        for _ in 0..7 {
            olm_core::turn::process_day(&mut game);
        }

        let player = game
            .players
            .iter()
            .find(|player| player.id == "p1")
            .unwrap();
        assert!(player.potential_revealed.is_some());
        assert_eq!(player.potential_research_eta_days, None);
        assert_eq!(player.potential_research_started_on, None);
    }

    #[test]
    fn apply_active_lineup_sets_manager_team_lineup() {
        let mut game = make_game();

        super::apply_active_lineup(
            &mut game,
            "team-1",
            vec!["p2".to_string(), "p1".to_string()],
        );

        assert_eq!(
            game.teams[0].active_lineup_ids,
            vec!["p2".to_string(), "p1".to_string()]
        );
    }

    #[test]
    fn training_does_not_increase_lol_stats_when_player_hits_potential_cap() {
        let mut game = make_game();
        if let Some(player) = game.players.iter_mut().find(|player| player.id == "p1") {
            player.attributes.mechanics = 90;
            player.attributes.laning = 90;
            player.attributes.teamfighting = 90;
            player.attributes.macro_play = 90;
            player.attributes.consistency = 90;
            player.attributes.shotcalling = 90;
            player.attributes.champion_pool = 90;
            player.attributes.discipline = 90;
            player.attributes.mental_resilience = 90;
            player.potential_base = 90;
        }

        let before = game
            .players
            .iter()
            .find(|player| player.id == "p1")
            .unwrap()
            .attributes
            .clone();

        for _ in 0..120 {
            olm_core::training::process_training(&mut game, 1);
        }

        let after = &game
            .players
            .iter()
            .find(|player| player.id == "p1")
            .unwrap()
            .attributes;
        assert_eq!(after.mechanics, before.mechanics);
        assert_eq!(after.laning, before.laning);
        assert_eq!(after.teamfighting, before.teamfighting);
        assert_eq!(after.macro_play, before.macro_play);
        assert_eq!(after.consistency, before.consistency);
        assert_eq!(after.shotcalling, before.shotcalling);
        assert_eq!(after.champion_pool, before.champion_pool);
        assert_eq!(after.discipline, before.discipline);
        assert_eq!(after.mental_resilience, before.mental_resilience);
    }
}
