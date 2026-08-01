mod fitness_warnings;
pub use fitness_warnings::check_squad_fitness_warnings;

use crate::domain::message::{InboxMessage, MessageCategory, MessagePriority};
use crate::domain::player::LolRole;
use crate::domain::team::{
    MainFacilityModuleKind, ScrimChampionPick, ScrimFocus, ScrimIssue, ScrimReport, ScrimStatus,
    TrainingFocus, TrainingIntensity, TrainingSchedule,
};
use crate::game::Game;
use crate::potential::{calculate_lol_ovr, effective_potential_cap};
use crate::staff_effects::LolStaffEffects;
use chrono::Datelike;
use rand::SeedableRng;
use std::collections::HashMap;

fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Computed coaching quality for a team's staff.
pub struct TeamCoachingBonus {
    pub coaching_mult: f64, // Overall coaching quality multiplier (1.0 = no staff)
    pub specialization_mult: f64, // Extra bonus if a coach specializes in the current focus
    pub physio_mult: f64,   // Recovery bonus from physio staff
}

/// Compute coaching bonuses from a team's staff.
fn compute_coaching_bonus(game: &Game, team_id: &str, _focus: &TrainingFocus) -> TeamCoachingBonus {
    let effects = LolStaffEffects::for_team(&game.staff, team_id);

    TeamCoachingBonus {
        coaching_mult: effects.coaching,
        specialization_mult: 1.0,
        physio_mult: effects.recovery,
    }
}

/// Per-team data collected before mutating players.
struct TeamTrainingPlan {
    team_id: String,
    default_focus: TrainingFocus,
    intensity: TrainingIntensity,
    schedule: TrainingSchedule,
    bonus: TeamCoachingBonus,
    medical_facility_mult: f64,
    training_facility_mult: f64,
}

#[derive(Clone)]
struct TeamScrimDayOutcome {
    gain_mult: f64,
    morale_penalty: u8,
    next_loss_streak: u8,
    played: u8,
    wins: u8,
    losses: u8,
    slot_results: Vec<(u8, u8, String, bool)>,
    reports: Vec<ScrimReport>,
}

fn lol_role_for_lol_role(role: &LolRole) -> &'static str {
    match role {
        LolRole::Top => "TOP",
        LolRole::Jungle => "JUNGLE",
        LolRole::Mid => "MID",
        LolRole::Adc => "ADC",
        LolRole::Support => "SUPPORT",
        LolRole::Unknown => "MID",
    }
}

fn fallback_champion_for_role(role: &str) -> String {
    match role {
        "TOP" => "Gnar",
        "JUNGLE" => "LeeSin",
        "MID" => "Azir",
        "ADC" => "Kaisa",
        "SUPPORT" => "Nautilus",
        _ => "Azir",
    }
    .to_string()
}

fn scrim_champion_picks_for_team(game: &Game, team_id: &str) -> Vec<ScrimChampionPick> {
    let starting_ids = game
        .teams
        .iter()
        .find(|team| team.id == team_id)
        .map(|team| team.active_lineup_ids.clone())
        .unwrap_or_default();

    let mut players: Vec<_> = if starting_ids.is_empty() {
        Vec::new()
    } else {
        starting_ids
            .iter()
            .filter_map(|player_id| game.players.iter().find(|player| player.id == *player_id))
            .filter(|player| player.team_id.as_deref() == Some(team_id))
            .take(5)
            .collect()
    };

    if players.len() < 5 {
        let mut fallback: Vec<_> = game
            .players
            .iter()
            .filter(|player| player.team_id.as_deref() == Some(team_id))
            .filter(|player| !players.iter().any(|selected| selected.id == player.id))
            .collect();
        fallback.sort_by_key(|player| std::cmp::Reverse(calculate_lol_ovr(player)));
        players.extend(fallback.into_iter().take(5 - players.len()));
    }

    players
        .into_iter()
        .map(|player| {
            let role = lol_role_for_lol_role(&player.natural_position).to_string();
            let champion_id = crate::champions::training_targets_for_player(player)
                .into_iter()
                .find(|target| !target.trim().is_empty())
                .unwrap_or_else(|| fallback_champion_for_role(&role));

            ScrimChampionPick {
                player_id: player.id.clone(),
                champion_id,
                role,
            }
        })
        .collect()
}

fn scrim_issue_from_result(
    won: bool,
    own_strength: f64,
    opponent_strength: f64,
) -> Option<ScrimIssue> {
    if won {
        return None;
    }

    let diff = own_strength - opponent_strength;
    if diff >= 6.0 {
        Some(ScrimIssue::Tilt)
    } else if diff >= 2.0 {
        Some(ScrimIssue::DraftGap)
    } else if diff <= -6.0 {
        Some(ScrimIssue::TeamfightExecution)
    } else if diff <= -2.0 {
        Some(ScrimIssue::ObjectiveSetup)
    } else {
        Some(ScrimIssue::ChampionComfort)
    }
}

fn scrim_focus_for_issue(issue: &Option<ScrimIssue>) -> ScrimFocus {
    match issue {
        Some(ScrimIssue::DraftGap) => ScrimFocus::DraftPrep,
        Some(ScrimIssue::LanePressure) => ScrimFocus::EarlyGame,
        Some(ScrimIssue::ObjectiveSetup) => ScrimFocus::Macro,
        Some(ScrimIssue::TeamfightExecution) => ScrimFocus::Teamfighting,
        Some(ScrimIssue::ChampionComfort) => ScrimFocus::ChampionPool,
        Some(ScrimIssue::Tilt) => ScrimFocus::Mental,
        None => ScrimFocus::ChampionPool,
    }
}

fn scrim_quality(own_strength: f64, opponent_strength: f64, gain_mult: f64) -> u8 {
    (58.0 + (opponent_strength - own_strength) * 1.8 + (gain_mult - 1.0) * 28.0)
        .round()
        .clamp(30.0, 95.0) as u8
}

fn scrim_severity(won: bool, own_strength: f64, opponent_strength: f64) -> u8 {
    if won {
        return 1;
    }

    let underperformance = (own_strength - opponent_strength).max(0.0);
    if underperformance >= 6.0 {
        4
    } else if underperformance >= 2.0 {
        3
    } else {
        2
    }
}

fn scrims_per_week_for_schedule(schedule: &TrainingSchedule) -> usize {
    match schedule {
        TrainingSchedule::Intense => 6,
        TrainingSchedule::Balanced => 4,
        TrainingSchedule::Light => 2,
    }
}

fn effective_scrim_slots(raw_slots: u8, schedule: &TrainingSchedule) -> usize {
    if raw_slots == 0 {
        return scrims_per_week_for_schedule(schedule);
    }

    match raw_slots.clamp(2, 6) {
        0..=2 => 2,
        3..=4 => 4,
        _ => 6,
    }
}

fn scrim_slot_weekdays_for_slots(slots: usize) -> &'static [u32] {
    match slots {
        0 | 1 | 2 => &[2, 2],
        3 | 4 => &[2, 2, 3, 3],
        _ => &[2, 2, 3, 3, 4, 4],
    }
}

fn scrim_slots_for_day(slots: usize, weekday_num: u32) -> Vec<usize> {
    scrim_slot_weekdays_for_slots(slots)
        .iter()
        .enumerate()
        .filter_map(|(index, day)| {
            if *day == weekday_num {
                Some(index)
            } else {
                None
            }
        })
        .take(slots)
        .collect()
}

fn team_lol_strength(game: &Game, team_id: &str) -> f64 {
    let starting_ids: Vec<String> = game
        .teams
        .iter()
        .find(|team| team.id == team_id)
        .map(|team| team.active_lineup_ids.clone())
        .unwrap_or_default();

    let mut values: Vec<f64> = if !starting_ids.is_empty() {
        let mut from_starting: Vec<f64> = starting_ids
            .iter()
            .filter_map(|pid| game.players.iter().find(|p| p.id == *pid))
            .filter(|player| player.team_id.as_deref() == Some(team_id))
            .take(5)
            .map(|player| f64::from(calculate_lol_ovr(player)))
            .collect();

        if from_starting.len() < 5 {
            let mut fallback: Vec<f64> = game
                .players
                .iter()
                .filter(|player| player.team_id.as_deref() == Some(team_id))
                .map(|player| f64::from(calculate_lol_ovr(player)))
                .collect();
            fallback.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
            for candidate in fallback {
                if from_starting.len() >= 5 {
                    break;
                }
                from_starting.push(candidate);
            }
        }
        from_starting
    } else {
        game.players
            .iter()
            .filter(|player| player.team_id.as_deref() == Some(team_id))
            .map(|player| f64::from(calculate_lol_ovr(player)))
            .collect()
    };

    if values.is_empty() {
        return 74.0;
    }
    values.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let sample = values.iter().take(5).copied().collect::<Vec<_>>();
    sample.iter().sum::<f64>() / sample.len() as f64
}

fn compute_scrim_gain_multiplier(own_strength: f64, opponent_strength: f64) -> f64 {
    let diff = (opponent_strength - own_strength).clamp(-12.0, 12.0);
    (1.0 + diff * 0.016).clamp(0.85, 1.25)
}

fn stable_roll(seed: &str) -> f64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    (hasher.finish() % 10_000) as f64 / 10_000.0
}

fn scrim_request_accepted(own_reputation: u32, opponent_reputation: u32, seed: &str) -> bool {
    let diff = own_reputation as f64 - opponent_reputation as f64;
    let chance = (0.52 + diff * 0.006).clamp(0.08, 0.88);
    stable_roll(seed) <= chance
}

fn current_week_key(game: &Game) -> String {
    format!(
        "{}-W{}",
        game.clock.current_date.iso_week().year(),
        game.clock.current_date.iso_week().week()
    )
}

fn scrim_focus_label(focus: &ScrimFocus) -> &'static str {
    match focus {
        ScrimFocus::DraftPrep => "Draft prep",
        ScrimFocus::ChampionPool => "Champion pool",
        ScrimFocus::EarlyGame => "Early game",
        ScrimFocus::Teamfighting => "Teamfighting",
        ScrimFocus::Macro => "Macro",
        ScrimFocus::Mental => "Mental",
    }
}

fn scrim_focus_i18n_key(focus: &ScrimFocus) -> &'static str {
    match focus {
        ScrimFocus::DraftPrep => "be.msg.scrimWeekly.focus.draftPrep",
        ScrimFocus::ChampionPool => "be.msg.scrimWeekly.focus.championPool",
        ScrimFocus::EarlyGame => "be.msg.scrimWeekly.focus.earlyGame",
        ScrimFocus::Teamfighting => "be.msg.scrimWeekly.focus.teamfighting",
        ScrimFocus::Macro => "be.msg.scrimWeekly.focus.macro",
        ScrimFocus::Mental => "be.msg.scrimWeekly.focus.mental",
    }
}

fn scrim_issue_label(issue: &ScrimIssue) -> &'static str {
    match issue {
        ScrimIssue::DraftGap => "Draft gap",
        ScrimIssue::LanePressure => "Lane pressure",
        ScrimIssue::ObjectiveSetup => "Objective setup",
        ScrimIssue::TeamfightExecution => "Teamfight execution",
        ScrimIssue::ChampionComfort => "Champion comfort",
        ScrimIssue::Tilt => "Tilt",
    }
}

fn scrim_issue_i18n_key(issue: &ScrimIssue) -> &'static str {
    match issue {
        ScrimIssue::DraftGap => "be.msg.scrimWeekly.issues.draftGap",
        ScrimIssue::LanePressure => "be.msg.scrimWeekly.issues.lanePressure",
        ScrimIssue::ObjectiveSetup => "be.msg.scrimWeekly.issues.objectiveSetup",
        ScrimIssue::TeamfightExecution => "be.msg.scrimWeekly.issues.teamfightExecution",
        ScrimIssue::ChampionComfort => "be.msg.scrimWeekly.issues.championComfort",
        ScrimIssue::Tilt => "be.msg.scrimWeekly.issues.tilt",
    }
}

fn most_common_label<T, F>(items: impl Iterator<Item = T>, label: F) -> String
where
    F: Fn(&T) -> String,
{
    let mut counts: HashMap<String, u16> = HashMap::new();
    for item in items {
        let key = label(&item);
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by(|(left_label, left_count), (right_label, right_count)| {
            left_count
                .cmp(right_count)
                .then_with(|| right_label.cmp(left_label))
        })
        .map(|(label, _)| label)
        .unwrap_or_else(|| "N/A".to_string())
}

struct WeeklyScrimRecommendation {
    key: &'static str,
    fallback: String,
}

fn weekly_scrim_recommendation(
    played: u8,
    losses: u8,
    loss_streak: u8,
    cancellations: u8,
    avg_quality: u8,
    recurring_issue: &str,
    _top_focus: &str,
) -> WeeklyScrimRecommendation {
    if played == 0 {
        return WeeklyScrimRecommendation {
            key: "be.msg.scrimWeekly.recommendations.lockPlans",
            fallback: "Lock Plan A/B/C earlier next week so the staff has usable prep data."
                .to_string(),
        };
    }
    if cancellations >= 2 {
        return WeeklyScrimRecommendation {
            key: "be.msg.scrimWeekly.recommendations.reduceCancellations",
            fallback: "Reduce cancellations next week; scrim reputation is part of your competitive infrastructure.".to_string(),
        };
    }
    if loss_streak >= 3 || losses >= played.saturating_sub(1).max(1) {
        return WeeklyScrimRecommendation {
            key: "be.msg.scrimWeekly.recommendations.resetBeforeVolume",
            fallback: "Open next week with Mental Reset or VOD Review before adding more volume."
                .to_string(),
        };
    }
    if avg_quality < 55 {
        return WeeklyScrimRecommendation {
            key: "be.msg.scrimWeekly.recommendations.narrowFocus",
            fallback: format!(
                "Keep the volume but narrow the focus around {}; quality is too noisy right now.",
                recurring_issue
            ),
        };
    }
    if recurring_issue != "N/A" {
        return WeeklyScrimRecommendation {
            key: "be.msg.scrimWeekly.recommendations.targetedDrills",
            fallback: "Schedule Targeted Drills for the recurring issue and keep the main prep block stable.".to_string(),
        };
    }
    WeeklyScrimRecommendation {
        key: "be.msg.scrimWeekly.recommendations.keepPlan",
        fallback:
            "Keep the current plan: it is producing useful reps without overloading the roster."
                .to_string(),
    }
}

fn build_weekly_scrim_staff_report(
    team: &crate::domain::team::Team,
    week_key: &str,
) -> (String, HashMap<String, String>) {
    let played_reports: Vec<&ScrimReport> = team
        .scrim_reports
        .iter()
        .filter(|report| report.week_key == week_key && report.status == ScrimStatus::Played)
        .collect();

    let avg_quality = if played_reports.is_empty() {
        0
    } else {
        (played_reports
            .iter()
            .map(|report| u16::from(report.quality))
            .sum::<u16>()
            / played_reports.len() as u16) as u8
    };
    let top_focus = most_common_label(
        played_reports.iter().map(|report| report.focus.clone()),
        |focus| scrim_focus_label(focus).to_string(),
    );
    let top_focus_key = most_common_label(
        played_reports.iter().map(|report| report.focus.clone()),
        |focus| scrim_focus_i18n_key(focus).to_string(),
    );
    let recurring_issue = most_common_label(
        played_reports
            .iter()
            .filter_map(|report| report.issue.clone()),
        |issue| scrim_issue_label(issue).to_string(),
    );
    let recurring_issue_key = most_common_label(
        played_reports
            .iter()
            .filter_map(|report| report.issue.clone()),
        |issue| scrim_issue_i18n_key(issue).to_string(),
    );
    let top_champion = most_common_label(
        played_reports.iter().flat_map(|report| {
            report
                .player_champion_picks
                .iter()
                .map(|pick| pick.champion_id.clone())
        }),
        |champion_id| champion_id.clone(),
    );
    let recommendation = weekly_scrim_recommendation(
        team.scrim_weekly_played,
        team.scrim_weekly_losses,
        team.scrim_loss_streak,
        team.scrim_weekly_cancellations,
        avg_quality,
        &recurring_issue,
        &top_focus,
    );

    let body = format!(
        "Weekly scrim report:\n\nPlayed: {}\nWins: {}\nLosses: {}\nCancellations: {}\nAverage quality: {}\nCurrent loss streak: {}\n\nMain focus: {}\nRecurring issue: {}\nMost practiced champion: {}\n\nRecommendation: {}",
        team.scrim_weekly_played,
        team.scrim_weekly_wins,
        team.scrim_weekly_losses,
        team.scrim_weekly_cancellations,
        avg_quality,
        team.scrim_loss_streak,
        top_focus,
        recurring_issue,
        top_champion,
        &recommendation.fallback,
    );

    let params = params(&[
        ("played", &team.scrim_weekly_played.to_string()),
        ("wins", &team.scrim_weekly_wins.to_string()),
        ("losses", &team.scrim_weekly_losses.to_string()),
        (
            "cancellations",
            &team.scrim_weekly_cancellations.to_string(),
        ),
        ("avgQuality", &avg_quality.to_string()),
        ("lossStreak", &team.scrim_loss_streak.to_string()),
        ("topFocus", &top_focus_key),
        ("recurringIssue", &recurring_issue_key),
        ("topChampion", &top_champion),
        ("recommendation", recommendation.key),
    ]);

    (body, params)
}

fn resolve_scrim_outcomes_for_day(
    game: &Game,
    weekday_num: u32,
    week_seed: &str,
) -> HashMap<String, TeamScrimDayOutcome> {
    let strength_by_team: HashMap<String, f64> = game
        .teams
        .iter()
        .map(|team| (team.id.clone(), team_lol_strength(game, &team.id)))
        .collect();
    let reputation_by_team: HashMap<String, u32> = game
        .teams
        .iter()
        .map(|team| (team.id.clone(), u32::from(team.scrim_reputation)))
        .collect();

    let mut scrim_outcome_by_team: HashMap<String, TeamScrimDayOutcome> = HashMap::new();

    for team in game.teams.iter() {
        let weekly_scrim_slots =
            effective_scrim_slots(team.scrim_weekly_slots, &team.training_schedule);
        let day_slots = scrim_slots_for_day(weekly_scrim_slots, weekday_num);
        if day_slots.is_empty() {
            continue;
        }

        let own_strength = *strength_by_team.get(&team.id).unwrap_or(&74.0);
        let staff_effects = LolStaffEffects::for_team(&game.staff, &team.id);
        let mut gain_sum = 0.0;
        let mut played: u8 = 0;
        let mut wins: u8 = 0;
        let mut losses: u8 = 0;
        let mut next_loss_streak = team.scrim_loss_streak;
        let mut slot_results: Vec<(u8, u8, String, bool)> = Vec::new();
        let mut reports: Vec<ScrimReport> = Vec::new();
        let today = game.clock.current_date.format("%Y-%m-%d").to_string();

        // E10/E11: never resolve another daily block while there is an unresolved block decision.
        let has_unresolved_today = team
            .scrim_reports
            .iter()
            .any(|entry| entry.date == today && entry.post_decision.is_none());
        if has_unresolved_today {
            continue;
        }

        for slot_idx in day_slots {
            let already_resolved =
                team.scrim_reports
                    .iter()
                    .any(|entry| entry.week_key == week_seed && entry.slot_index == slot_idx as u8)
                    || team.scrim_slot_results.iter().any(|entry| {
                        entry.week_key == week_seed && entry.slot_index == slot_idx as u8
                    });
            if already_resolved {
                continue;
            }

            let configured_plan = team
                .weekly_scrim_plan_team_ids
                .get(slot_idx)
                .cloned()
                .unwrap_or_else(|| {
                    team.weekly_scrim_opponent_ids
                        .get(slot_idx)
                        .cloned()
                        .filter(|team_id| !team_id.is_empty())
                        .map(|team_id| vec![team_id])
                        .unwrap_or_default()
                });

            let used_opponents_this_week: std::collections::HashSet<String> = team
                .scrim_reports
                .iter()
                .filter(|entry| entry.week_key == week_seed)
                .map(|entry| entry.opponent_team_id.clone())
                .chain(
                    team.scrim_slot_results
                        .iter()
                        .filter(|entry| entry.week_key == week_seed)
                        .map(|entry| entry.opponent_team_id.clone()),
                )
                .collect();

            let planned_opponent = configured_plan
                .iter()
                .filter(|candidate| candidate.as_str() != team.id.as_str())
                .filter(|candidate| strength_by_team.contains_key(candidate.as_str()))
                .filter(|candidate| !used_opponents_this_week.contains(candidate.as_str()))
                .enumerate()
                .find_map(|(priority_index, candidate)| {
                    let requires_acceptance = configured_plan.len() > 1;
                    let accepted = !requires_acceptance
                        || scrim_request_accepted(
                            u32::from(team.scrim_reputation),
                            *reputation_by_team
                                .get(candidate)
                                .unwrap_or(&u32::from(team.scrim_reputation)),
                            &format!(
                                "scrim-request:{}:{}:{}:{}:{}",
                                week_seed, team.id, candidate, slot_idx, priority_index
                            ),
                        );

                    if accepted {
                        Some(candidate.clone())
                    } else {
                        None
                    }
                });

            let configured = team
                .weekly_scrim_opponent_ids
                .get(slot_idx)
                .cloned()
                .unwrap_or_default();
            let opponent_id = if let Some(candidate) = planned_opponent {
                candidate
            } else if configured.is_empty()
                || configured == team.id
                || !strength_by_team.contains_key(&configured)
            {
                continue;
            } else {
                configured
            };

            // E10: resolve only the earliest selected unresolved block; later blocks wait for manager decision.
            if played > 0 {
                break;
            }

            let opponent_strength = *strength_by_team.get(&opponent_id).unwrap_or(&own_strength);
            let gain_mult = compute_scrim_gain_multiplier(own_strength, opponent_strength)
                * ((staff_effects.tactics * 0.55) + (staff_effects.analysis * 0.45))
                    .clamp(0.90, 1.15);
            gain_sum += gain_mult;
            played = played.saturating_add(1);

            let diff = (own_strength - opponent_strength).clamp(-14.0, 14.0);
            let win_prob = (0.5 + diff * 0.022).clamp(0.2, 0.8);
            let seed = format!(
                "scrim:{}:{}:{}:{}:{}",
                week_seed, team.id, opponent_id, weekday_num, slot_idx
            );
            let roll = stable_roll(&seed);
            let won_scrim = roll <= win_prob;

            if won_scrim {
                wins = wins.saturating_add(1);
                next_loss_streak = 0;
            } else {
                losses = losses.saturating_add(1);
                next_loss_streak = next_loss_streak.saturating_add(1);
            }

            slot_results.push((
                slot_idx as u8,
                weekday_num as u8,
                opponent_id.clone(),
                won_scrim,
            ));

            let issue = scrim_issue_from_result(won_scrim, own_strength, opponent_strength);
            let focus = team
                .scrim_weekly_objective
                .clone()
                .unwrap_or_else(|| scrim_focus_for_issue(&issue));
            reports.push(ScrimReport {
                date: game.clock.current_date.format("%Y-%m-%d").to_string(),
                week_key: week_seed.to_string(),
                slot_index: slot_idx as u8,
                weekday: weekday_num as u8,
                team_id: team.id.clone(),
                opponent_team_id: opponent_id,
                status: ScrimStatus::Played,
                won: Some(won_scrim),
                focus,
                issue,
                severity: scrim_severity(won_scrim, own_strength, opponent_strength),
                quality: scrim_quality(own_strength, opponent_strength, gain_mult),
                player_champion_picks: scrim_champion_picks_for_team(game, &team.id),
                post_decision: None,
                created_on: game.clock.current_date.format("%Y-%m-%d").to_string(),
            });
        }

        if played == 0 {
            continue;
        }

        let base_morale_penalty = if next_loss_streak >= 5 {
            4
        } else if next_loss_streak >= 4 {
            3
        } else if next_loss_streak >= 3 {
            2
        } else {
            0
        };
        let morale_softening = ((staff_effects.morale - 1.0).max(0.0)
            + (staff_effects.recovery - 1.0).max(0.0))
        .clamp(0.0, 0.35);
        let morale_penalty =
            ((f64::from(base_morale_penalty)) * (1.0 - morale_softening)).round() as u8;

        scrim_outcome_by_team.insert(
            team.id.clone(),
            TeamScrimDayOutcome {
                gain_mult: (gain_sum / f64::from(played.max(1))).clamp(0.80, 1.30),
                morale_penalty,
                next_loss_streak,
                played,
                wins,
                losses,
                slot_results,
                reports,
            },
        );
    }

    scrim_outcome_by_team
}

fn apply_scrim_outcomes(
    game: &mut Game,
    scrim_outcome_by_team: &HashMap<String, TeamScrimDayOutcome>,
    week_seed: &str,
) {
    for team in game.teams.iter_mut() {
        if let Some(outcome) = scrim_outcome_by_team.get(&team.id) {
            team.scrim_loss_streak = outcome.next_loss_streak;
            team.scrim_weekly_played = team.scrim_weekly_played.saturating_add(outcome.played);
            team.scrim_weekly_wins = team.scrim_weekly_wins.saturating_add(outcome.wins);
            team.scrim_weekly_losses = team.scrim_weekly_losses.saturating_add(outcome.losses);

            for (slot_index, weekday, opponent_team_id, won) in &outcome.slot_results {
                let already_exists = team
                    .scrim_slot_results
                    .iter()
                    .any(|entry| entry.week_key == week_seed && entry.slot_index == *slot_index);
                if already_exists {
                    continue;
                }

                team.scrim_slot_results
                    .push(crate::domain::team::ScrimSlotResult {
                        week_key: week_seed.to_string(),
                        slot_index: *slot_index,
                        weekday: *weekday,
                        opponent_team_id: opponent_team_id.clone(),
                        won: *won,
                        simulated_on: game.clock.current_date.format("%Y-%m-%d").to_string(),
                    });
            }

            for report in &outcome.reports {
                let already_exists = team.scrim_reports.iter().any(|entry| {
                    entry.week_key == week_seed && entry.slot_index == report.slot_index
                });
                if !already_exists {
                    team.scrim_reports.push(report.clone());
                }
            }

            if team.scrim_slot_results.len() > 96 {
                let start = team.scrim_slot_results.len().saturating_sub(96);
                team.scrim_slot_results = team.scrim_slot_results.split_off(start);
            }
            if team.scrim_reports.len() > 96 {
                let start = team.scrim_reports.len().saturating_sub(96);
                team.scrim_reports = team.scrim_reports.split_off(start);
            }
        }
    }
}

fn apply_scrim_morale(
    game: &mut Game,
    scrim_outcome_by_team: &HashMap<String, TeamScrimDayOutcome>,
) {
    for player in game.players.iter_mut() {
        let Some(team_id) = player.team_id.as_ref() else {
            continue;
        };
        let Some(outcome) = scrim_outcome_by_team.get(team_id) else {
            continue;
        };
        if outcome.morale_penalty == 0 {
            continue;
        }

        player.morale = player.morale.saturating_sub(outcome.morale_penalty);
    }
}

fn scrim_report_gain_mult(
    game: &Game,
    team_id: &str,
    week_seed: &str,
    weekday_num: u32,
) -> Option<f64> {
    let reports: Vec<_> = game
        .teams
        .iter()
        .find(|team| team.id == team_id)?
        .scrim_reports
        .iter()
        .filter(|report| report.week_key == week_seed && report.weekday == weekday_num as u8)
        .collect();

    if reports.is_empty() {
        return None;
    }

    let avg_quality = reports
        .iter()
        .map(|report| f64::from(report.quality))
        .sum::<f64>()
        / reports.len() as f64;
    Some((0.85 + (avg_quality / 100.0) * 0.45).clamp(0.80, 1.30))
}

pub fn process_scrim_block(game: &mut Game, weekday_num: u32) -> bool {
    let week_seed = current_week_key(game);
    let outcomes = resolve_scrim_outcomes_for_day(game, weekday_num, &week_seed);
    let resolved_any = !outcomes.is_empty();
    apply_scrim_outcomes(game, &outcomes, &week_seed);
    apply_scrim_morale(game, &outcomes);
    resolved_any
}

/// Process daily training for all teams.
/// On non-match days each team's players train according to the team's
/// current focus, intensity, and schedule. Rest days (determined by the
/// weekly schedule) give full condition recovery with no training cost.
/// Scrims focus can gain extra efficiency from stronger weekly scrim opponents.
/// `weekday_num` is 0=Mon .. 6=Sun (chrono Weekday::num_days_from_monday()).
pub fn process_training(game: &mut Game, weekday_num: u32) {
    let manager_team_id = game.manager.team_id.clone();
    let rival_player_ids: Vec<String> = game
        .players
        .iter()
        .filter(|player| {
            player.team_id.as_ref().is_some_and(|team_id| {
                manager_team_id
                    .as_ref()
                    .is_none_or(|manager_id| team_id != manager_id)
            })
        })
        .map(|player| player.id.clone())
        .collect();
    for player_id in rival_player_ids {
        crate::champions::ensure_training_targets_from_mastery(game, &player_id);
    }

    // Collect plans for all teams (immutable borrow)
    let team_plans: Vec<TeamTrainingPlan> = game
        .teams
        .iter()
        .map(|t| {
            let bonus = compute_coaching_bonus(game, &t.id, &t.training_focus);
            let medical_facility_mult = t.facilities.recovery_suite_condition_multiplier();
            TeamTrainingPlan {
                team_id: t.id.clone(),
                default_focus: t.training_focus.clone(),
                intensity: t.training_intensity.clone(),
                schedule: t.training_schedule.clone(),
                bonus,
                medical_facility_mult,
                training_facility_mult: 1.0
                    + f64::from(
                        t.facilities
                            .module_level(MainFacilityModuleKind::ScrimsRoom)
                            .saturating_sub(1),
                    ) * 0.03,
            }
        })
        .collect();

    let week_seed = current_week_key(game);
    let scrim_outcome_by_team = resolve_scrim_outcomes_for_day(game, weekday_num, &week_seed);
    let scrim_report_gain_by_team: HashMap<String, f64> = game
        .teams
        .iter()
        .filter_map(|team| {
            scrim_report_gain_mult(game, &team.id, &week_seed, weekday_num)
                .map(|gain_mult| (team.id.clone(), gain_mult))
        })
        .collect();
    let scrim_focus_gain_by_team: HashMap<String, (ScrimFocus, f64)> = scrim_outcome_by_team
        .iter()
        .filter_map(|(team_id, outcome)| {
            let report = outcome.reports.first()?;
            let team = game
                .teams
                .iter()
                .find(|candidate| candidate.id == *team_id)?;
            let effective_focus = team
                .scrim_weekly_objective
                .clone()
                .unwrap_or_else(|| report.focus.clone());
            let quality_mult = (f64::from(report.quality) / 100.0).clamp(0.45, 0.95);
            Some((team_id.clone(), (effective_focus, quality_mult)))
        })
        .collect();

    let mut mastery_training_ticks: Vec<(String, String, f64, u8)> = Vec::new();

    for plan in &team_plans {
        let is_training_day = plan.schedule.is_training_day(weekday_num);

        let intensity_mult = match &plan.intensity {
            TrainingIntensity::Low => 0.5,
            TrainingIntensity::Medium => 1.0,
            TrainingIntensity::High => 1.5,
        };

        for player in game.players.iter_mut() {
            if player.team_id.as_deref() != Some(&plan.team_id) {
                continue;
            }

            // Determine this player's effective focus:
            // player override > team default
            let player_focus = player
                .training_focus
                .as_ref()
                .unwrap_or(&plan.default_focus);

            // On rest days or recovery-focused plans: no training cost
            let condition_cost: u8 = if !is_training_day {
                0
            } else {
                match (player_focus, &plan.intensity) {
                    (focus, _) if focus.is_recovery_plan() => 0,
                    (_, TrainingIntensity::Low) => 3,
                    (_, TrainingIntensity::Medium) => 6,
                    (_, TrainingIntensity::High) => 10,
                }
            };

            // Recovery amount: rest days get boosted recovery (like mental reset days)
            let recovery_base: f64 = if !is_training_day {
                9.0 * plan.bonus.physio_mult * plan.medical_facility_mult
            } else {
                match player_focus {
                    TrainingFocus::MentalResetRecovery => {
                        9.0 * plan.bonus.physio_mult * plan.medical_facility_mult
                    }
                    _ => 4.0 * plan.bonus.physio_mult * plan.medical_facility_mult,
                }
            };

            // Age, morale, and current condition all affect recovery rate.
            // Older players recover more slowly; high morale aids recovery;
            // severely fatigued players have a harder time bouncing back.
            let age = estimate_age(&player.date_of_birth);
            let age_rec = recovery_factor_from_age(age);
            let morale_rec = recovery_factor_from_morale(player.morale);
            let condition_rec = recovery_factor_from_condition(player.condition);
            let fitness_rec = recovery_factor_from_fitness(player.fitness);

            // On rest days: only recovery, no attribute gains
            if !is_training_day {
                let stamina_factor = player.attributes.mental_resilience as f64 / 100.0;
                let recovery = (recovery_base
                    * (0.5 + stamina_factor * 0.5)
                    * age_rec
                    * morale_rec
                    * condition_rec
                    * fitness_rec) as u8;
                player.condition = (player.condition + recovery).min(100);
                continue;
            }

            // Age factor for attribute gains: younger players grow faster, older players slower
            let age_factor = if age <= 21 {
                1.5
            } else if age <= 25 {
                1.2
            } else if age <= 29 {
                1.0
            } else if age <= 33 {
                0.6
            } else {
                0.3
            };

            // Base gain per session for the underlying model, boosted by coaching staff.
            // The selected attributes are tuned so the LoL-facing roster/profile stats
            // shown to the user move in the expected direction without rewriting the
            // whole legacy player model.
            let gain = 0.075
                * intensity_mult
                * age_factor
                * plan.bonus.coaching_mult
                * plan.bonus.specialization_mult
                * plan.training_facility_mult;

            let scrim_gain_mult = if matches!(player_focus, TrainingFocus::Scrims) {
                scrim_outcome_by_team
                    .get(&plan.team_id)
                    .map(|outcome| outcome.gain_mult)
                    .or_else(|| scrim_report_gain_by_team.get(&plan.team_id).copied())
                    .unwrap_or(1.0)
            } else {
                1.0
            };
            let gain = gain * scrim_gain_mult;

            // Apply LoL stat gains only when the player's current LoL OVR is below potential cap.
            let capped = is_lol_training_capped(player);
            apply_focus_gains(&mut player.attributes, player_focus, gain, capped);
            if let Some((focus, focus_mult)) = scrim_focus_gain_by_team.get(&plan.team_id) {
                apply_scrim_plan_focus_gains(
                    &mut player.attributes,
                    focus,
                    gain * 1.9 * *focus_mult,
                    capped,
                );
            }

            if is_training_day && !player_focus.is_recovery_plan() {
                let targets = crate::champions::training_targets_for_player(player);
                let (focus_mult, attempts): (f64, u8) = match player_focus {
                    TrainingFocus::ChampionPoolPractice => (1.4, 4),
                    TrainingFocus::IndividualCoaching => (1.15, 3),
                    TrainingFocus::Scrims => (1.0, 3),
                    TrainingFocus::MacroSystems => (0.9, 2),
                    TrainingFocus::VODReview => (0.85, 2),
                    TrainingFocus::MentalResetRecovery => (0.0, 0),
                };

                if attempts > 0 && !targets.is_empty() {
                    let priority_weights: [f64; 3] = [1.0, 0.65, 0.4];
                    for (index, champion_id) in targets.iter().enumerate() {
                        let weight = priority_weights.get(index).copied().unwrap_or(0.3);
                        let weighted_attempts = ((attempts as f64) * weight).round() as u8;
                        if weighted_attempts == 0 {
                            continue;
                        }
                        let mastery_gain_factor = gain * focus_mult * (0.85 + weight * 0.35);
                        mastery_training_ticks.push((
                            player.id.clone(),
                            champion_id.clone(),
                            mastery_gain_factor,
                            weighted_attempts.max(1),
                        ));
                    }
                }
            }

            // Apply fitness changes based on training focus.
            // Scrims best preserve fitness; recovery plans give a tiny boost.
            apply_fitness_change(&mut player.fitness, player_focus, intensity_mult);

            // Apply condition: deplete from training, then recover.
            // Recalculate condition_rec AFTER depletion so the recovery factor
            // reflects the player's actual post-training fatigue state.
            player.condition = player.condition.saturating_sub(condition_cost);
            let post_training_condition_rec = recovery_factor_from_condition(player.condition);
            let stamina_factor = player.attributes.mental_resilience as f64 / 100.0;
            let recovery = (recovery_base
                * (0.5 + stamina_factor * 0.5)
                * age_rec
                * morale_rec
                * post_training_condition_rec
                * fitness_rec) as u8;
            player.condition = (player.condition + recovery).min(100);
        }
    }

    apply_scrim_outcomes(game, &scrim_outcome_by_team, &week_seed);
    apply_scrim_morale(game, &scrim_outcome_by_team);

    crate::champions::ensure_patch_seed(&mut game.champion_patch);
    let mut mastery_rng = rand::rngs::StdRng::seed_from_u64(crate::champions::derived_seed(
        game.champion_patch.rng_seed,
        &week_seed,
    ));
    for (player_id, champion_id, gain, attempts) in mastery_training_ticks {
        let soloq_mult = crate::champions::mastery_gain_multiplier_for_player(game, &player_id);
        let effective_gain = gain * soloq_mult;
        for _ in 0..attempts {
            crate::champions::apply_training_mastery_progress(
                game,
                &player_id,
                &champion_id,
                effective_gain,
                &mut mastery_rng,
            );
        }
    }

    if weekday_num == 6
        && let Some(manager_team_id) = game.manager.team_id.clone()
        && let Some(team) = game
            .teams
            .iter_mut()
            .find(|candidate| candidate.id == manager_team_id)
    {
        if team.scrim_weekly_played > 0 {
            let (body, i18n_params) = build_weekly_scrim_staff_report(team, &week_seed);

            let msg = InboxMessage::new(
                format!("msg_scrim_weekly_{}", uuid::Uuid::new_v4()),
                "Weekly Scrim Staff Report".to_string(),
                body,
                "Coaching Staff".to_string(),
                game.clock.current_date.to_rfc3339(),
            )
            .with_category(MessageCategory::System)
            .with_priority(MessagePriority::Normal)
            .with_sender_role("Coaching Staff")
            .with_i18n(
                "be.msg.scrimWeekly.subject",
                "be.msg.scrimWeekly.body",
                i18n_params,
            )
            .with_sender_i18n("be.sender.coachingStaff", "be.role.coachingStaff");

            game.messages.push(msg);
        }

        team.scrim_weekly_played = 0;
        team.scrim_weekly_wins = 0;
        team.scrim_weekly_losses = 0;
        team.scrim_weekly_cancellations = 0;
    }
}

/// Apply fitness changes based on training focus.
/// Scrims best preserve fitness, while recovery plans give a tiny boost.
/// Other plans slowly decay very high fitness if not maintained.
fn apply_fitness_change(fitness: &mut u8, focus: &TrainingFocus, intensity_mult: f64) {
    use rand::RngExt;
    let mut rng = rand::rng();
    match focus {
        TrainingFocus::Scrims => {
            // Scrims are the closest MVP equivalent to high-load team practice.
            // Higher intensity → higher gain probability.
            let gain_prob = 0.025 * intensity_mult; // 0.0125–0.0375 per session
            let roll: f64 = rng.random_range(0.0..1.0);
            if roll < gain_prob && *fitness < 100 {
                *fitness = fitness.saturating_add(1);
            }
        }
        TrainingFocus::MentalResetRecovery => {
            // Recovery-focused days give a tiny fitness nudge.
            let roll: f64 = rng.random_range(0.0..1.0);
            if roll < 0.05 && *fitness < 100 {
                *fitness = fitness.saturating_add(1);
            }
        }
        _ => {
            // Non-physical training: very slight decay if player is already very fit
            // (fitness above 85 needs active maintenance).
            if *fitness > 85 {
                let roll: f64 = rng.random_range(0.0..1.0);
                if roll < 0.05 {
                    *fitness = fitness.saturating_sub(1);
                }
            }
        }
    }
}

fn try_gain(current: &mut u8, gain: f64) {
    use rand::RngExt;
    if *current >= 99 {
        return;
    }
    let mut rng = rand::rng();
    let roll: f64 = rng.random_range(0.0..1.0);
    if roll < gain {
        *current = (*current + 1).min(99);
    }
}

/// Apply attribute gains based on training focus.
/// We still mutate the legacy core attributes, but we prioritize the combinations
/// that feed the LoL-facing profile/roster stats the player actually sees.
fn apply_focus_gains(
    attrs: &mut crate::domain::player::PlayerAttributes,
    focus: &TrainingFocus,
    gain: f64,
    capped: bool,
) {
    if capped {
        return;
    }

    // LoL-native stat mapping (1:1 over legacy fields):
    // mechanics -> dribbling
    // laning -> shooting
    // teamfighting -> teamwork
    // macro -> vision
    // consistency -> decisions
    // shotcalling -> leadership
    // champion pool -> agility
    // discipline -> composure
    // mental resilience -> stamina
    match focus {
        TrainingFocus::Scrims => {
            try_gain(&mut attrs.consistency, gain);
            try_gain(&mut attrs.teamfighting, gain);
            try_gain(&mut attrs.discipline, gain * 0.85);
            try_gain(&mut attrs.mental_resilience, gain * 0.65);
            try_gain(&mut attrs.macro_play, gain * 0.55);
        }
        TrainingFocus::VODReview => {
            try_gain(&mut attrs.macro_play, gain);
            try_gain(&mut attrs.consistency, gain);
            try_gain(&mut attrs.discipline, gain * 0.75);
            try_gain(&mut attrs.shotcalling, gain * 0.6);
        }
        TrainingFocus::IndividualCoaching => {
            try_gain(&mut attrs.laning, gain);
            try_gain(&mut attrs.mechanics, gain);
            try_gain(&mut attrs.champion_pool, gain);
            try_gain(&mut attrs.discipline, gain * 0.8);
            try_gain(&mut attrs.teamfighting, gain * 0.4);
        }
        TrainingFocus::ChampionPoolPractice => {
            try_gain(&mut attrs.mechanics, gain);
            try_gain(&mut attrs.champion_pool, gain);
            try_gain(&mut attrs.macro_play, gain * 0.8);
            try_gain(&mut attrs.laning, gain * 0.7);
            try_gain(&mut attrs.consistency, gain * 0.65);
        }
        TrainingFocus::MacroSystems => {
            try_gain(&mut attrs.macro_play, gain);
            try_gain(&mut attrs.consistency, gain);
            try_gain(&mut attrs.teamfighting, gain * 0.8);
            try_gain(&mut attrs.shotcalling, gain * 0.7);
        }
        TrainingFocus::MentalResetRecovery => {
            // No attribute gains on recovery days
        }
    }
}

fn apply_scrim_plan_focus_gains(
    attrs: &mut crate::domain::player::PlayerAttributes,
    focus: &ScrimFocus,
    gain: f64,
    capped: bool,
) {
    if capped {
        return;
    }

    match focus {
        ScrimFocus::DraftPrep => {
            try_gain(&mut attrs.macro_play, gain);
            try_gain(&mut attrs.consistency, gain * 0.9);
            try_gain(&mut attrs.shotcalling, gain * 0.7);
        }
        ScrimFocus::ChampionPool => {
            try_gain(&mut attrs.mechanics, gain);
            try_gain(&mut attrs.champion_pool, gain);
            try_gain(&mut attrs.laning, gain * 0.7);
        }
        ScrimFocus::EarlyGame => {
            try_gain(&mut attrs.laning, gain);
            try_gain(&mut attrs.consistency, gain * 0.85);
            try_gain(&mut attrs.macro_play, gain * 0.75);
        }
        ScrimFocus::Teamfighting => {
            try_gain(&mut attrs.teamfighting, gain);
            try_gain(&mut attrs.discipline, gain * 0.9);
            try_gain(&mut attrs.consistency, gain * 0.75);
        }
        ScrimFocus::Macro => {
            try_gain(&mut attrs.macro_play, gain);
            try_gain(&mut attrs.consistency, gain);
            try_gain(&mut attrs.teamfighting, gain * 0.7);
        }
        ScrimFocus::Mental => {
            try_gain(&mut attrs.discipline, gain);
            try_gain(&mut attrs.mental_resilience, gain * 0.85);
            try_gain(&mut attrs.shotcalling, gain * 0.65);
        }
    }
}

fn is_lol_training_capped(player: &crate::domain::player::Player) -> bool {
    calculate_lol_ovr(player) >= effective_potential_cap(player)
}

#[cfg(test)]
mod tests {
    use super::{apply_focus_gains, is_lol_training_capped};
    use crate::domain::player::{LolRole, Player, PlayerAttributes};
    use crate::domain::team::TrainingFocus;

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

    #[test]
    fn potential_cap_blocks_lol_stat_gains_when_ovr_reaches_cap() {
        let mut player = Player::new(
            "p-1".to_string(),
            "Cap".to_string(),
            "Cap".to_string(),
            "2002-01-01".to_string(),
            "GB".to_string(),
            LolRole::Mid,
            attrs(90),
        );
        player.potential_base = 90;

        assert!(is_lol_training_capped(&player));

        let before = player.attributes.clone();
        apply_focus_gains(
            &mut player.attributes,
            &TrainingFocus::IndividualCoaching,
            1.0,
            true,
        );
        assert_eq!(player.attributes.mechanics, before.mechanics);
        assert_eq!(player.attributes.laning, before.laning);
        assert_eq!(player.attributes.champion_pool, before.champion_pool);
    }
}

/// Apply a post-scrim decision (simplified — marks the decision on the report).
pub fn apply_post_scrim_decision(
    game: &mut crate::game::Game,
    tid: &str,
    slot_index: u8,
    decision: &str,
) -> Result<(), String> {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == tid) {
        if let Some(report) = team
            .scrim_reports
            .iter_mut()
            .find(|r| r.date == today && r.slot_index == slot_index && r.post_decision.is_none())
        {
            let d = parse_post_scrim_decision(decision)?;
            report.post_decision = Some(d);
            return Ok(());
        }
    }
    Err("No unresolved scrim report found for this slot".to_string())
}

/// Estimate player age from date_of_birth string ("YYYY-MM-DD").
fn estimate_age(dob: &str) -> u32 {
    let parts: Vec<&str> = dob.split('-').collect();
    if parts.is_empty() {
        return 25; // fallback
    }
    let birth_year: u32 = parts[0].parse().unwrap_or(2000);
    // Use a rough estimate — the game clock year would be ideal but
    // this is close enough for growth factor purposes.
    let current_year: u32 = 2026;
    current_year.saturating_sub(birth_year)
}

/// Recovery multiplier from age: younger players bounce back faster.
fn recovery_factor_from_age(age: u32) -> f64 {
    if age <= 21 {
        1.10
    } else if age <= 25 {
        1.05
    } else if age <= 29 {
        1.00
    } else if age <= 33 {
        0.85
    } else {
        0.70
    }
}

/// Recovery multiplier from morale: players in good spirits recover better.
fn recovery_factor_from_morale(morale: u8) -> f64 {
    if morale >= 70 {
        1.10
    } else if morale >= 40 {
        1.00
    } else {
        0.90
    }
}

/// Recovery multiplier from current condition: severely fatigued players recover more slowly.
fn recovery_factor_from_condition(condition: u8) -> f64 {
    if condition < 30 {
        0.90
    } else if condition < 50 {
        0.95
    } else {
        1.00
    }
}

/// Recovery multiplier from fitness: fitter players recover condition faster.
fn recovery_factor_from_fitness(fitness: u8) -> f64 {
    if fitness < 30 {
        0.75
    } else if fitness < 50 {
        0.88
    } else if fitness < 70 {
        1.00
    } else if fitness < 90 {
        1.12
    } else {
        1.20
    }
}

// ── Scrim scheduling helpers (moved from Tauri squad.rs) ─────

/// Parse a post-scrim decision string into a PostScrimDecision.
pub fn parse_post_scrim_decision(
    value: &str,
) -> Result<crate::domain::team::PostScrimDecision, String> {
    match value {
        "ContinuePlan" => Ok(crate::domain::team::PostScrimDecision::ContinuePlan),
        "VodReview" => Ok(crate::domain::team::PostScrimDecision::VodReview),
        "MentalReset" => Ok(crate::domain::team::PostScrimDecision::MentalReset),
        "TargetedDrills" => Ok(crate::domain::team::PostScrimDecision::TargetedDrills),
        "PushThrough" => Ok(crate::domain::team::PostScrimDecision::PushThrough),
        "DayOff" => Ok(crate::domain::team::PostScrimDecision::DayOff),
        _ => Err(format!("Unknown post-scrim decision: {value}")),
    }
}

/// Parse a scrim focus string into a ScrimFocus.
pub fn parse_scrim_focus(value: &str) -> Result<crate::domain::team::ScrimFocus, String> {
    match value {
        "DraftPrep" => Ok(crate::domain::team::ScrimFocus::DraftPrep),
        "ChampionPool" => Ok(crate::domain::team::ScrimFocus::ChampionPool),
        "EarlyGame" => Ok(crate::domain::team::ScrimFocus::EarlyGame),
        "Teamfighting" => Ok(crate::domain::team::ScrimFocus::Teamfighting),
        "Macro" => Ok(crate::domain::team::ScrimFocus::Macro),
        "Mental" => Ok(crate::domain::team::ScrimFocus::Mental),
        _ => Err(format!("Unknown scrim objective: {value}")),
    }
}

/// Alias that returns u8 instead of usize (for Tauri compatibility).
pub fn scrims_per_week_as_u8(schedule: &crate::domain::team::TrainingSchedule) -> u8 {
    crate::training::scrims_per_week_for_schedule(schedule) as u8
}

/// Alias that returns u8 instead of usize.
pub fn effective_scrim_slots_u8(
    raw_slots: u8,
    schedule: &crate::domain::team::TrainingSchedule,
) -> u8 {
    crate::training::effective_scrim_slots(raw_slots, schedule) as u8
}

/// Alias that returns Vec<u8> instead of Vec<u32>.
pub fn scrim_slot_weekdays_u8(slots: u8) -> Vec<u8> {
    let slots_usize = if slots == 0 {
        4
    } else {
        slots.clamp(2, 6) as usize
    };
    crate::training::scrim_slot_weekdays_for_slots(slots_usize)
        .iter()
        .map(|&d| d as u8)
        .collect()
}

/// Returns the label parts (day, suffix) for a slot index.
pub fn slot_label_parts(weekdays: &[u8], slot_index: usize) -> (u8, String) {
    let day = weekdays.get(slot_index).copied().unwrap_or(0);
    let previous_same_day = weekdays
        .iter()
        .take(slot_index)
        .filter(|&&c| c == day)
        .count();
    let total_same_day = weekdays.iter().filter(|&&c| c == day).count();
    let suffix = if total_same_day > 1 {
        ((b'A' + previous_same_day as u8) as char).to_string()
    } else {
        String::new()
    };
    (day, suffix)
}

/// Check whether the weekly scrim setup is locked for a team.
pub fn weekly_scrim_setup_lock_state(
    team: &crate::domain::team::Team,
    week_key: &str,
    current_weekday: u8,
    day_phase: crate::game::DayPhase,
) -> (bool, Option<String>) {
    if team.scrim_setup_locked_week_key.as_deref() == Some(week_key) {
        return (true, Some("manual".to_string()));
    }
    let started_week = team.scrim_reports.iter().any(|e| e.week_key == week_key)
        || team
            .scrim_slot_results
            .iter()
            .any(|e| e.week_key == week_key);
    if started_week {
        return (true, Some("week_started".to_string()));
    }
    let days = scrim_slot_weekdays_u8(team.scrim_weekly_slots);
    let first_scrim_weekday = days.into_iter().min().unwrap_or(2);

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
        || (current_weekday == first_scrim_weekday && day_phase != crate::game::DayPhase::Morning)
    {
        return (true, Some("first_scrim_window".to_string()));
    }
    (false, None)
}

/// Whether pushing through despite bad scrim results is recommended.
pub fn is_push_through_recommended(
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

/// Get the position (0 or 1) of a slot within the current weekday.
pub fn daily_slot_position(
    team: &crate::domain::team::Team,
    current_weekday: u8,
    slot_index: u8,
) -> Option<usize> {
    let slot_days = scrim_slot_weekdays_u8(team.scrim_weekly_slots);
    let mut todays: Vec<usize> = Vec::new();
    for (i, day) in slot_days.iter().enumerate() {
        if *day == current_weekday {
            todays.push(i);
        }
    }
    todays.iter().position(|i| *i as u8 == slot_index)
}

/// Classify a scrim report as Good or Bad.
pub fn quality_from_report(
    report: &crate::domain::team::ScrimReport,
) -> crate::scrim_flow::ScrimResultQuality {
    if report.won.unwrap_or(false) {
        crate::scrim_flow::ScrimResultQuality::Good
    } else {
        crate::scrim_flow::ScrimResultQuality::Bad
    }
}

/// Estimate the average LoL OVR of the top 5 players on a team.
pub fn estimate_team_lol_ovr(game: &crate::game::Game, team_id: &str) -> u8 {
    let mut ovrs: Vec<u8> = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(team_id))
        .map(|p| crate::potential::calculate_lol_ovr(p))
        .collect();
    if ovrs.is_empty() {
        return 74;
    }
    ovrs.sort_by(|a, b| b.cmp(a));
    let sample: Vec<u8> = ovrs.iter().take(5).copied().collect();
    let sum: u32 = sample.iter().map(|&v| u32::from(v)).sum();
    (sum / sample.len() as u32) as u8
}
