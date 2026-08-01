use crate::domain::team::{TrainingIntensity, TrainingSchedule};
use crate::game::Game;
use std::collections::HashMap;

/// Check squad fitness and generate staff warning messages when players are exhausted.
/// Called after training processing on each day.
pub fn check_squad_fitness_warnings(game: &mut Game) {
    let user_team_id = match &game.manager.team_id {
        Some(id) => id.clone(),
        None => return,
    };

    let date = game.clock.current_date.to_rfc3339();
    let today_str = game.clock.current_date.format("%Y-%m-%d").to_string();

    // Collect fitness data for user's team
    let team_players: Vec<_> = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(&user_team_id))
        .collect();

    if team_players.is_empty() {
        return;
    }

    let avg_condition =
        team_players.iter().map(|p| p.condition as f64).sum::<f64>() / team_players.len() as f64;
    let exhausted_count = team_players.iter().filter(|p| p.condition < 40).count();
    let critical_count = team_players.iter().filter(|p| p.condition < 25).count();

    // Deduplicate: only one warning per day
    let msg_id = format!("fitness_warn_{}", today_str);
    if game.messages.iter().any(|m| m.id == msg_id) {
        return;
    }

    // Get team schedule for context
    let schedule = game
        .teams
        .iter()
        .find(|t| t.id == user_team_id)
        .map(|t| t.training_schedule.clone())
        .unwrap_or_default();

    let intensity = game
        .teams
        .iter()
        .find(|t| t.id == user_team_id)
        .map(|t| t.training_intensity.clone())
        .unwrap_or_default();

    // Determine if we need a physio/staff role sender
    let has_physio = game.staff.iter().any(|s| {
        s.team_id.as_deref() == Some(&user_team_id)
            && matches!(s.role, crate::domain::staff::StaffRole::Physio)
    });

    let sender = if has_physio {
        "Head Physio"
    } else {
        "Assistant Manager"
    };
    let sender_name = if has_physio {
        game.staff
            .iter()
            .find(|s| {
                s.team_id.as_deref() == Some(&user_team_id)
                    && matches!(s.role, crate::domain::staff::StaffRole::Physio)
            })
            .map(|s| format!("{} {}", s.first_name, s.last_name))
            .unwrap_or_else(|| "Medical Staff".to_string())
    } else {
        game.staff
            .iter()
            .find(|s| {
                s.team_id.as_deref() == Some(&user_team_id)
                    && matches!(s.role, crate::domain::staff::StaffRole::AssistantManager)
            })
            .map(|s| format!("{} {}", s.first_name, s.last_name))
            .unwrap_or_else(|| "Assistant Manager".to_string())
    };

    use crate::domain::message::*;

    // Critical: multiple players below 25 condition
    if critical_count >= 3 {
        let exhausted_names: Vec<String> = team_players
            .iter()
            .filter(|p| p.condition < 25)
            .take(5)
            .map(|p| format!("{} ({}%)", p.match_name, p.condition))
            .collect();

        let schedule_advice = match schedule {
            TrainingSchedule::Intense => {
                "I strongly recommend switching to a Balanced or Light training schedule immediately. \
                The Intense schedule is running the squad into the ground."
            }
            TrainingSchedule::Balanced => {
                "Consider switching to a Light schedule or setting the focus to Mental Reset / Recovery \
                until fitness levels improve."
            }
            TrainingSchedule::Light => {
                "Even on the Light schedule, the squad is struggling. Please set the training focus \
                to Mental Reset / Recovery — the lads need proper rest."
            }
        };

        let intensity_advice = match intensity {
            TrainingIntensity::High => {
                " Also, reducing training intensity from High would help significantly."
            }
            TrainingIntensity::Medium => "",
            TrainingIntensity::Low => "",
        };

        let body = format!(
            "Boss, we have a serious fitness crisis. {} players are in critical condition:\n\n\
            {}\n\n\
            Average squad fitness is at {:.0}%. {}{}\n\n\
            If we push them further without rest, performance and morale will suffer.",
            critical_count,
            exhausted_names.join("\n"),
            avg_condition,
            schedule_advice,
            intensity_advice,
        );

        let mut msg = InboxMessage::new(
            msg_id,
            "URGENT: Squad Fitness Crisis".to_string(),
            body,
            sender_name,
            date,
        )
        .with_category(MessageCategory::Training)
        .with_priority(MessagePriority::Urgent)
        .with_sender_role(sender)
        .with_action(MessageAction {
            id: "go_training".to_string(),
            label: "Adjust Training".to_string(),
            action_type: ActionType::NavigateTo {
                route: "/dashboard?tab=Training".to_string(),
            },
            resolved: false,
            label_key: Some("be.msg.fitness.actionAdjust".to_string()),
        })
        .with_context(MessageContext {
            team_id: Some(user_team_id),
            ..Default::default()
        })
        .with_i18n(
            "be.msg.fitness.critical.subject",
            &format!(
                "be.msg.fitness.critical.body.{}",
                match schedule {
                    TrainingSchedule::Intense => "intense",
                    TrainingSchedule::Balanced => "balanced",
                    TrainingSchedule::Light => "light",
                }
            ),
            {
                let mut p = HashMap::new();
                p.insert("criticalCount".to_string(), critical_count.to_string());
                p.insert("players".to_string(), exhausted_names.join("\n"));
                p.insert("avgCondition".to_string(), format!("{:.0}", avg_condition));
                let sched_key = match schedule {
                    TrainingSchedule::Intense => "intense",
                    TrainingSchedule::Balanced => "balanced",
                    TrainingSchedule::Light => "light",
                };
                p.insert("schedule".to_string(), sched_key.to_string());
                let int_key = match intensity {
                    TrainingIntensity::High => "high",
                    _ => "",
                };
                p.insert("intensity".to_string(), int_key.to_string());
                p
            },
        );

        if has_physio {
            msg = msg.with_sender_i18n("be.sender.headPhysio", "be.role.headPhysio");
        } else {
            msg = msg.with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager");
        }

        game.messages.push(msg);
        return;
    }

    // Warning: average condition below 50 or many exhausted players
    if avg_condition < 50.0 || exhausted_count >= 4 {
        let schedule_advice = match schedule {
            TrainingSchedule::Intense => {
                "Switching to a Balanced schedule would give the squad more recovery time."
            }
            TrainingSchedule::Balanced => {
                "A Light schedule for a few days could help the squad bounce back."
            }
            TrainingSchedule::Light => {
                "Setting the training focus to Mental Reset / Recovery would maximise fitness gains."
            }
        };

        let body = format!(
            "Boss, the squad is looking tired. Average fitness is {:.0}% and {} players are below 40% condition.\n\n\
            {}\n\n\
            We should consider giving the lads some rest before the next match.",
            avg_condition, exhausted_count, schedule_advice,
        );

        let mut msg = InboxMessage::new(
            msg_id,
            "Squad Fitness Warning".to_string(),
            body,
            sender_name,
            date,
        )
        .with_category(MessageCategory::Training)
        .with_priority(MessagePriority::High)
        .with_sender_role(sender)
        .with_action(MessageAction {
            id: "go_training".to_string(),
            label: "Adjust Training".to_string(),
            action_type: ActionType::NavigateTo {
                route: "/dashboard?tab=Training".to_string(),
            },
            resolved: false,
            label_key: Some("be.msg.fitness.actionAdjust".to_string()),
        })
        .with_context(MessageContext {
            team_id: Some(user_team_id),
            ..Default::default()
        })
        .with_i18n(
            "be.msg.fitness.warning.subject",
            &format!(
                "be.msg.fitness.warning.body.{}",
                match schedule {
                    TrainingSchedule::Intense => "intense",
                    TrainingSchedule::Balanced => "balanced",
                    TrainingSchedule::Light => "light",
                }
            ),
            {
                let mut p = HashMap::new();
                p.insert("avgCondition".to_string(), format!("{:.0}", avg_condition));
                p.insert("exhaustedCount".to_string(), exhausted_count.to_string());
                let sched_key = match schedule {
                    TrainingSchedule::Intense => "intense",
                    TrainingSchedule::Balanced => "balanced",
                    TrainingSchedule::Light => "light",
                };
                p.insert("schedule".to_string(), sched_key.to_string());
                p
            },
        );

        if has_physio {
            msg = msg.with_sender_i18n("be.sender.headPhysio", "be.role.headPhysio");
        } else {
            msg = msg.with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager");
        }

        game.messages.push(msg);
    }
}
