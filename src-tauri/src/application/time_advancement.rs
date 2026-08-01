use chrono::Datelike;
use log::info;
use serde::{Deserialize, Serialize};

use crate::commands::round_summary::{build_round_summary_dto, RoundSummaryDto};
use olm_core::game::{DayPhase, Game};
use olm_core::live_match_manager::{self, MatchMode};
use olm_core::state::StateManager;

fn has_unresolved_scrim_review_today(game: &Game) -> bool {
    let Some(team_id) = game.manager.team_id.as_ref() else {
        return false;
    };
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    game.teams
        .iter()
        .find(|team| &team.id == team_id)
        .map(|team| {
            team.scrim_reports
                .iter()
                .any(|report| report.date == today && report.post_decision.is_none())
        })
        .unwrap_or(false)
}

fn first_scrim_weekday_for_team(team: &olm_core::domain::team::Team) -> u8 {
    let raw_slots = if team.scrim_weekly_slots > 0 {
        team.scrim_weekly_slots
    } else {
        match team.training_schedule {
            olm_core::domain::team::TrainingSchedule::Intense => 6,
            olm_core::domain::team::TrainingSchedule::Balanced => 4,
            olm_core::domain::team::TrainingSchedule::Light => 2,
        }
    };
    let slots = if raw_slots <= 2 {
        2
    } else if raw_slots <= 4 {
        4
    } else {
        6
    };
    let all = match slots {
        0..=2 => vec![2_u8, 2_u8],
        3..=4 => vec![2_u8, 2_u8, 3_u8, 3_u8],
        _ => vec![2_u8, 2_u8, 3_u8, 3_u8, 4_u8, 4_u8],
    };
    all.into_iter().min().unwrap_or(2)
}

fn has_no_weekly_scrim_setup(game: &Game) -> bool {
    let Some(team_id) = game.manager.team_id.as_ref() else {
        return false;
    };
    let week_key = format!(
        "{}-W{}",
        game.clock.current_date.iso_week().year(),
        game.clock.current_date.iso_week().week()
    );
    let current_weekday = game.clock.current_date.weekday().num_days_from_monday() as u8;
    game.teams
        .iter()
        .find(|team| &team.id == team_id)
        .map(|team| {
            let first_day = first_scrim_weekday_for_team(team);
            let in_scrim_start_window =
                current_weekday == first_day && game.day_phase == DayPhase::Morning;
            if !in_scrim_start_window {
                return false;
            }
            if team.scrim_setup_locked_week_key.as_deref() == Some(week_key.as_str()) {
                return false;
            }
            let has_objective = team.scrim_weekly_objective.is_some();
            let has_plans = team
                .weekly_scrim_plan_team_ids
                .iter()
                .any(|plan| plan.iter().any(|entry| !entry.is_empty()));
            !(has_objective || has_plans)
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceTimeWithModeResponse {
    pub action: String,
    pub game: Option<Game>,
    pub snapshot: Option<olm_core::engine::MatchSnapshot>,
    pub fixture_index: Option<usize>,
    pub mode: Option<String>,
    pub round_summary: Option<RoundSummaryDto>,
}

fn round_context_for_today(
    game: &Game,
    today: &str,
) -> Option<(u32, Vec<olm_core::domain::league::StandingEntry>)> {
    let league = game.active_league()?;
    let matchday = league
        .fixtures
        .iter()
        .find(|fixture| fixture.date == today)
        .map(|fixture| fixture.matchday)?;

    Some((matchday, league.standings.clone()))
}

fn scheduled_user_fixture_index(game: &Game, today: &str) -> Option<usize> {
    let user_team_id = game.manager.team_id.as_ref()?;
    let league = game.active_league()?;

    league
        .fixtures
        .iter()
        .enumerate()
        .find_map(|(index, fixture)| {
            if fixture.date == today
                && fixture.status == olm_core::domain::league::FixtureStatus::Scheduled
                && (fixture.home_team_id == *user_team_id || fixture.away_team_id == *user_team_id)
            {
                Some(index)
            } else {
                None
            }
        })
}

pub fn advance_time_with_mode(
    state: &StateManager,
    mode: &str,
) -> Result<AdvanceTimeWithModeResponse, String> {
    info!("[cmd] advance_time_with_mode: mode={}", mode);
    let mut game = state
        .get_game(|current_game| current_game.clone())
        .ok_or("No active game session")?;

    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let round_context = round_context_for_today(&game, &today);
    let user_fixture_idx = scheduled_user_fixture_index(&game, &today);

    info!(
        "[cmd] advance_time_with_mode: date={}, user_team_id={:?}, user_fixture_idx={:?}",
        today, game.manager.team_id, user_fixture_idx
    );

    match (mode, user_fixture_idx) {
        ("live" | "spectator", Some(index)) => {
            let match_mode = if mode == "live" {
                MatchMode::Live
            } else {
                MatchMode::Spectator
            };
            let session = live_match_manager::create_live_match(&game, index, match_mode, false)?;
            let snapshot = session.snapshot();
            info!(
                "[cmd] advance_time_with_mode: live_match fixture_idx={}, phase={:?}, home_team={}, away_team={}",
                index,
                snapshot.phase,
                snapshot.home_team.name,
                snapshot.away_team.name
            );
            state.set_live_match(session);

            let mut captures = Vec::new();
            olm_core::turn::simulate_other_matches_with_capture(
                &mut game,
                &today,
                Some(index),
                &mut |capture| captures.push(capture),
            );
            for capture in captures {
                state.append_stats_state(capture);
            }
            let round_summary =
                round_context
                    .as_ref()
                    .and_then(|(matchday, previous_standings)| {
                        build_round_summary_dto(&game, *matchday, previous_standings)
                    });
            state.set_game(game);

            Ok(AdvanceTimeWithModeResponse {
                action: "live_match".to_string(),
                game: None,
                snapshot: Some(snapshot),
                fixture_index: Some(index),
                mode: Some(mode.to_string()),
                round_summary,
            })
        }
        ("delegate", Some(index)) => {
            info!(
                "[cmd] advance_time_with_mode: delegate fixture_idx={}, date={}",
                index, today
            );
            let mut session =
                live_match_manager::create_live_match(&game, index, MatchMode::Instant, false)?;
            session.user_side = None;
            session.run_to_completion();

            let home_team_id = session.home_team_id.clone();
            let away_team_id = session.away_team_id.clone();
            let report = session.match_state.into_report();

            let mut captures = Vec::new();
            olm_core::turn::simulate_other_matches_with_capture(
                &mut game,
                &today,
                Some(index),
                &mut |capture| captures.push(capture),
            );

            olm_core::turn::apply_match_report_with_capture(
                &mut game,
                index,
                &home_team_id,
                &away_team_id,
                &report,
                &mut |capture| captures.push(capture),
            );

            for capture in captures {
                state.append_stats_state(capture);
            }

            let round_summary =
                round_context
                    .as_ref()
                    .and_then(|(matchday, previous_standings)| {
                        build_round_summary_dto(&game, *matchday, previous_standings)
                    });

            olm_core::turn::finish_live_match_day(&mut game);
            state.set_game(game.clone());

            Ok(AdvanceTimeWithModeResponse {
                action: "advanced".to_string(),
                game: Some(game),
                snapshot: None,
                fixture_index: None,
                mode: None,
                round_summary,
            })
        }
        _ => {
            if user_fixture_idx.is_none() && game.day_phase != DayPhase::Evening {
                if mode != "delegate" && has_no_weekly_scrim_setup(&game) {
                    state.set_game(game.clone());
                    return Ok(AdvanceTimeWithModeResponse {
                        action: "blocked_scrim_setup".to_string(),
                        game: Some(game),
                        snapshot: None,
                        fixture_index: None,
                        mode: None,
                        round_summary: None,
                    });
                }
                if game.day_phase == DayPhase::Morning {
                    let weekday_num = game.clock.current_date.weekday().num_days_from_monday();
                    olm_core::training::process_scrim_block(&mut game, weekday_num);
                }

                if game.day_phase == DayPhase::ScrimBlock
                    && has_unresolved_scrim_review_today(&game)
                {
                    state.set_game(game.clone());
                    return Ok(AdvanceTimeWithModeResponse {
                        action: "blocked_scrim_decision".to_string(),
                        game: Some(game),
                        snapshot: None,
                        fixture_index: None,
                        mode: None,
                        round_summary: None,
                    });
                }

                game.day_phase = game.day_phase.next();
                state.set_game(game.clone());
                return Ok(AdvanceTimeWithModeResponse {
                    action: "phase_advanced".to_string(),
                    game: Some(game),
                    snapshot: None,
                    fixture_index: None,
                    mode: None,
                    round_summary: None,
                });
            }

            info!(
                "[cmd] advance_time_with_mode: normal_advance date={}, mode={}",
                today, mode
            );
            let mut captures = Vec::new();
            olm_core::turn::process_day_with_capture(&mut game, &mut |capture| {
                captures.push(capture);
            });
            for capture in captures {
                state.append_stats_state(capture);
            }
            let round_summary =
                round_context
                    .as_ref()
                    .and_then(|(matchday, previous_standings)| {
                        build_round_summary_dto(&game, *matchday, previous_standings)
                    });
            state.set_game(game.clone());

            Ok(AdvanceTimeWithModeResponse {
                action: "advanced".to_string(),
                game: Some(game),
                snapshot: None,
                fixture_index: None,
                mode: None,
                round_summary,
            })
        }
    }
}
