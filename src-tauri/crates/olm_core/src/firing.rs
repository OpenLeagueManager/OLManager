use crate::domain::message::*;
use crate::game::Game;
use log::info;
use std::collections::HashMap;

const WARN_THRESHOLD: u8 = 25;
const FINAL_WARN_THRESHOLD: u8 = 18;
const FIRE_THRESHOLD: u8 = 10;

const WARNING_ID_PREFIX: &str = "board_warning";
const FINAL_WARNING_ID_PREFIX: &str = "board_final_warning";
const FIRED_ID_PREFIX: &str = "board_fired";

// warning_stage on Manager: 0 = none, 1 = warning issued, 2 = final warning issued.
// Reset on hire/fire so warnings don't carry across clubs.
const STAGE_WARNING: u8 = 1;
const STAGE_FINAL: u8 = 2;

fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Check manager satisfaction and issue warnings or fire.
/// Returns `true` if the manager was fired.
pub fn check_manager_firing(game: &mut Game) -> bool {
    if game.manager.team_id.is_none() {
        return false;
    }

    let satisfaction = game.manager.satisfaction;
    let stage = game.manager.warning_stage;

    if satisfaction <= FIRE_THRESHOLD {
        if stage >= STAGE_WARNING {
            execute_firing(game);
            return true;
        }
        // No prior warning — send the initial warning first (normal progression)
        send_warning(game);
    } else if satisfaction <= FINAL_WARN_THRESHOLD {
        if stage < STAGE_FINAL {
            send_final_warning(game);
        }
    } else if satisfaction <= WARN_THRESHOLD && stage < STAGE_WARNING {
        send_warning(game);
    }

    false
}

fn execute_firing(game: &mut Game) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let team_id = game.manager.team_id.clone().unwrap_or_default();
    let team_name = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map(|t| t.name.clone())
        .unwrap_or_default();

    info!(
        "[firing] Manager {} fired from {} (satisfaction={})",
        game.manager.full_name(),
        team_name,
        game.manager.satisfaction
    );

    // Clear manager from team
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) {
        team.manager_id = None;
    }

    // Close career history and unassign
    game.manager.fire(&today);

    // Send dismissal message (unique ID so it doesn't collide with a future firing at another club)
    let msg = InboxMessage::new(
        format!("{}_{}_{}", FIRED_ID_PREFIX, team_id, today),
        format!("Notice of Dismissal — {}", team_name),
        format!(
            "The board of directors at {} has decided to relieve you of your duties as manager, \
             effective immediately.\n\nWe thank you for your service and wish you well in your future career.",
            team_name
        ),
        "Board of Directors".to_string(),
        today,
    )
    .with_category(MessageCategory::BoardDirective)
    .with_priority(MessagePriority::Urgent)
    .with_sender_role("Chairman")
    .with_i18n(
        "be.msg.boardFired.subject",
        "be.msg.boardFired.body",
        params(&[("team", &team_name)]),
    )
    .with_sender_i18n("be.sender.boardOfDirectors", "be.role.chairman");

    game.messages.push(msg);
}

fn send_warning(game: &mut Game) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let team_id = game.manager.team_id.clone().unwrap_or_default();
    let team_name = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map(|t| t.name.clone())
        .unwrap_or_default();

    info!(
        "[firing] Board warning issued to {} (satisfaction={})",
        game.manager.full_name(),
        game.manager.satisfaction
    );

    game.manager.warning_stage = STAGE_WARNING;

    let msg = InboxMessage::new(
        format!("{}_{}_{}", WARNING_ID_PREFIX, team_id, today),
        "Board Concern — Performance Review".to_string(),
        format!(
            "The board is growing increasingly concerned with recent results at {}. \
             Your position will come under serious review if there is no improvement in the near future.",
            team_name
        ),
        "Board of Directors".to_string(),
        today,
    )
    .with_category(MessageCategory::BoardDirective)
    .with_priority(MessagePriority::High)
    .with_sender_role("Chairman")
    .with_i18n(
        "be.msg.boardWarning.subject",
        "be.msg.boardWarning.body",
        params(&[("team", &team_name)]),
    )
    .with_sender_i18n("be.sender.boardOfDirectors", "be.role.chairman");

    game.messages.push(msg);
}

fn send_final_warning(game: &mut Game) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let team_id = game.manager.team_id.clone().unwrap_or_default();
    let team_name = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map(|t| t.name.clone())
        .unwrap_or_default();

    info!(
        "[firing] Final warning issued to {} (satisfaction={})",
        game.manager.full_name(),
        game.manager.satisfaction
    );

    game.manager.warning_stage = STAGE_FINAL;

    let msg = InboxMessage::new(
        format!("{}_{}_{}", FINAL_WARNING_ID_PREFIX, team_id, today),
        "Final Warning — Immediate Improvement Required".to_string(),
        format!(
            "This is your final warning. The board at {} has lost patience with the current run of results. \
             Unless there is an immediate and significant improvement, we will have no choice but to consider your position.",
            team_name
        ),
        "Board of Directors".to_string(),
        today,
    )
    .with_category(MessageCategory::BoardDirective)
    .with_priority(MessagePriority::Urgent)
    .with_sender_role("Chairman")
    .with_i18n(
        "be.msg.boardFinalWarning.subject",
        "be.msg.boardFinalWarning.body",
        params(&[("team", &team_name)]),
    )
    .with_sender_i18n("be.sender.boardOfDirectors", "be.role.chairman");

    game.messages.push(msg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::domain::manager::{Manager, ManagerCareerEntry};
    use crate::domain::team::Team;
    use chrono::{TimeZone, Utc};

    fn make_game(satisfaction: u8) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr1".to_string(),
            "Alex".to_string(),
            "Boss".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team1".to_string());
        manager.satisfaction = satisfaction;
        manager.career_history.push(ManagerCareerEntry {
            team_id: "team1".to_string(),
            team_name: "Test FC".to_string(),
            start_date: "2026-07-01".to_string(),
            end_date: None,
            matches: 10,
            wins: 2,
            losses: 5,
            best_league_position: Some(12),
        });

        let mut team = Team::new(
            "team1".to_string(),
            "Test FC".to_string(),
            "TST".to_string(),
            "England".to_string(),
            "Testville".to_string(),
            "Test Ground".to_string(),
            20_000,
        );
        team.manager_id = Some("mgr1".to_string());

        Game::new(clock, manager, vec![team], vec![], vec![], vec![])
    }

    #[test]
    fn no_action_when_satisfaction_above_warning_threshold() {
        let mut game = make_game(50);
        let fired = check_manager_firing(&mut game);
        assert!(!fired);
        assert!(game.manager.team_id.is_some());
        assert!(game.messages.is_empty());
    }

    #[test]
    fn warning_sent_at_warning_threshold() {
        let mut game = make_game(25);
        let fired = check_manager_firing(&mut game);
        assert!(!fired);
        assert!(game.manager.team_id.is_some());
        assert_eq!(game.messages.len(), 1);
        assert!(game.messages[0].id.starts_with(WARNING_ID_PREFIX));
        assert_eq!(game.messages[0].priority, MessagePriority::High);
        assert_eq!(game.manager.warning_stage, STAGE_WARNING);
    }

    #[test]
    fn final_warning_sent_at_final_warning_threshold() {
        let mut game = make_game(18);
        let fired = check_manager_firing(&mut game);
        assert!(!fired);
        assert!(game.manager.team_id.is_some());
        assert_eq!(game.messages.len(), 1);
        assert!(game.messages[0].id.starts_with(FINAL_WARNING_ID_PREFIX));
        assert_eq!(game.messages[0].priority, MessagePriority::Urgent);
        assert_eq!(game.manager.warning_stage, STAGE_FINAL);
    }

    #[test]
    fn not_fired_at_fire_threshold_without_prior_warning() {
        let mut game = make_game(5);
        let fired = check_manager_firing(&mut game);
        assert!(!fired);
        assert!(game.manager.team_id.is_some());
        // Should send the initial warning first (normal progression)
        assert_eq!(game.messages.len(), 1);
        assert!(game.messages[0].id.starts_with(WARNING_ID_PREFIX));
    }

    #[test]
    fn fired_at_fire_threshold_with_prior_warning() {
        let mut game = make_game(5);
        game.manager.warning_stage = STAGE_WARNING;

        let fired = check_manager_firing(&mut game);
        assert!(fired);
        assert!(game.manager.team_id.is_none());
        assert_eq!(game.messages.len(), 1);
        assert!(game.messages[0].id.starts_with(FIRED_ID_PREFIX));
        assert_eq!(game.messages[0].priority, MessagePriority::Urgent);
    }

    #[test]
    fn career_history_closed_on_firing() {
        let mut game = make_game(5);
        game.manager.warning_stage = STAGE_FINAL;

        check_manager_firing(&mut game);
        let entry = &game.manager.career_history[0];
        assert_eq!(entry.end_date, Some("2026-10-15".to_string()));
    }

    #[test]
    fn team_manager_id_cleared_on_firing() {
        let mut game = make_game(5);
        game.manager.warning_stage = STAGE_WARNING;

        check_manager_firing(&mut game);
        assert!(game.teams[0].manager_id.is_none());
    }

    #[test]
    fn warning_stage_does_not_carry_across_clubs() {
        // Manager previously warned/fired at an old club; after re-hire,
        // a new ≤10 satisfaction drop must not instantly fire them.
        let mut game = make_game(5);
        game.manager.warning_stage = 0; // simulate fresh hire
        let fired = check_manager_firing(&mut game);
        assert!(!fired);
        assert!(game.manager.team_id.is_some());
        assert_eq!(game.messages.len(), 1);
        assert!(game.messages[0].id.starts_with(WARNING_ID_PREFIX));
    }

    #[test]
    fn warning_message_ids_are_unique_per_club_and_date() {
        let mut game = make_game(25);
        check_manager_firing(&mut game);
        let first_id = game.messages[0].id.clone();
        assert!(first_id.contains("team1"));
        assert!(first_id.contains("2026-10-15"));
    }

    #[test]
    fn warning_deduplication() {
        let mut game = make_game(25);
        check_manager_firing(&mut game);
        assert_eq!(game.messages.len(), 1);
        // Call again — should not add a second warning
        check_manager_firing(&mut game);
        assert_eq!(game.messages.len(), 1);
    }

    #[test]
    fn no_action_when_manager_has_no_team() {
        let mut game = make_game(5);
        game.manager.team_id = None;
        let fired = check_manager_firing(&mut game);
        assert!(!fired);
        assert!(game.messages.is_empty());
    }
}
