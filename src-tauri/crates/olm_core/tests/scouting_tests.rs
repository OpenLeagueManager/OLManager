use chrono::{TimeZone, Utc};
use olm_core::clock::GameClock;
use olm_core::domain::manager::Manager;
use olm_core::domain::message::*;
use olm_core::domain::player::{Player, PlayerAttributes};
use olm_core::domain::staff::{Staff, StaffAttributes, StaffRole};
use olm_core::domain::stats::LolRole;
use olm_core::domain::team::Team;
use olm_core::game::Game;
use olm_core::scouting::{process_scouting, scout_max_assignments, send_scout};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn default_attrs() -> PlayerAttributes {
    PlayerAttributes {
        mental_resilience: 65,
        champion_pool: 68,
        laning: 66,
        mechanics: 74,
        macro_play: 70,
        consistency: 64,
        discipline: 60,
        teamfighting: 66,
        shotcalling: 55,
    }
}

fn make_player(id: &str, name: &str, team_id: &str) -> Player {
    let mut p = Player::new(
        id.to_string(),
        name.to_string(),
        name.to_string(),
        "1998-03-15".to_string(),
        "BR".to_string(),
        LolRole::Jungle,
        default_attrs(),
    );
    p.team_id = Some(team_id.to_string());
    p.morale = 75;
    p.condition = 90;
    p
}

fn make_team(id: &str, name: &str) -> Team {
    Team::new(
        id.to_string(),
        name.to_string(),
        name[..3].to_string(),
        "England".to_string(),
        "London".to_string(),
        "Stadium".to_string(),
        40_000,
    )
}

fn make_scout(id: &str, team_id: &str, judging_ability: u8, judging_potential: u8) -> Staff {
    let mut s = Staff::new(
        id.to_string(),
        "Scout".to_string(),
        format!("Nr{}", id),
        "1985-01-01".to_string(),
        StaffRole::Scout,
        StaffAttributes {
            coaching: 30,
            judging_ability,
            judging_potential,
            physiotherapy: 20,
        },
    );
    s.team_id = Some(team_id.to_string());
    s
}

fn make_game() -> Game {
    let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap());
    let mut manager = Manager::new(
        "mgr1".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("team1".to_string());

    let team1 = make_team("team1", "Test FC");
    let team2 = make_team("team2", "Rival FC");
    let players = vec![
        make_player("p1", "Own Player", "team1"),
        make_player("p2", "Target Player", "team2"),
        make_player("p3", "Another Target", "team2"),
    ];
    let scout = make_scout("scout1", "team1", 80, 75);

    Game::new(
        clock,
        manager,
        vec![team1, team2],
        players,
        vec![scout],
        vec![],
    )
}

// ---------------------------------------------------------------------------
// scout_max_assignments
// ---------------------------------------------------------------------------

#[test]
fn max_assignments_scales_with_ability() {
    assert_eq!(scout_max_assignments(90), 5);
    assert_eq!(scout_max_assignments(80), 5);
    assert_eq!(scout_max_assignments(79), 4);
    assert_eq!(scout_max_assignments(60), 4);
    assert_eq!(scout_max_assignments(59), 3);
    assert_eq!(scout_max_assignments(40), 3);
    assert_eq!(scout_max_assignments(39), 2);
    assert_eq!(scout_max_assignments(20), 2);
    assert_eq!(scout_max_assignments(19), 1);
    assert_eq!(scout_max_assignments(0), 1);
}

// ---------------------------------------------------------------------------
// send_scout
// ---------------------------------------------------------------------------

#[test]
fn send_scout_creates_assignment() {
    let mut game = make_game();
    let result = send_scout(&mut game, "scout1", "p2");
    assert!(result.is_ok());
    assert_eq!(game.scouting_assignments.len(), 1);
    assert_eq!(game.scouting_assignments[0].player_id, "p2");
    assert_eq!(game.scouting_assignments[0].scout_id, "scout1");
}

#[test]
fn send_scout_accepts_own_player() {
    let mut game = make_game();
    let result = send_scout(&mut game, "scout1", "p1");
    assert!(result.is_ok());
    assert_eq!(game.scouting_assignments.len(), 1);
    assert_eq!(game.scouting_assignments[0].player_id, "p1");
}

#[test]
fn send_scout_rejects_unknown_player() {
    let mut game = make_game();
    let result = send_scout(&mut game, "scout1", "nonexistent");
    assert!(result.is_err());
}

#[test]
fn send_scout_rejects_duplicate_assignment() {
    let mut game = make_game();
    send_scout(&mut game, "scout1", "p2").unwrap();
    let result = send_scout(&mut game, "scout1", "p2");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("already being scouted"));
}

#[test]
fn send_scout_rejects_non_scout_staff() {
    let mut game = make_game();
    // Replace scout with a coach
    game.staff[0].role = StaffRole::Coach;
    let result = send_scout(&mut game, "scout1", "p2");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not a scout"));
}

// ---------------------------------------------------------------------------
// process_scouting — report generation
// ---------------------------------------------------------------------------

fn complete_scouting(game: &mut Game) {
    // Advance enough days for the assignment to complete
    for _ in 0..10 {
        process_scouting(game);
        game.clock.advance_days(1);
    }
}

#[test]
fn process_scouting_generates_report_message() {
    let mut game = make_game();
    send_scout(&mut game, "scout1", "p2").unwrap();
    complete_scouting(&mut game);

    let scout_msgs: Vec<&InboxMessage> = game
        .messages
        .iter()
        .filter(|m| m.category == MessageCategory::ScoutReport)
        .collect();
    assert_eq!(
        scout_msgs.len(),
        1,
        "Should produce exactly one scout report"
    );
}

#[test]
fn report_has_scout_report_data() {
    let mut game = make_game();
    send_scout(&mut game, "scout1", "p2").unwrap();
    complete_scouting(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.category == MessageCategory::ScoutReport)
        .expect("Should have a scout report");

    let report = msg
        .context
        .scout_report
        .as_ref()
        .expect("Should have scout_report data in context");

    assert_eq!(report.player_id, "p2");
    assert_eq!(report.player_name, "Target Player");
    assert_eq!(report.nationality, "BR");
    assert!(report.team_name.is_some(), "Should have team name");
    assert_eq!(report.team_name.as_deref(), Some("Rival FC"));
}

#[test]
fn own_player_report_has_exact_attributes_no_noise() {
    let mut game = make_game();
    let attrs = default_attrs();

    send_scout(&mut game, "scout1", "p1").unwrap();
    complete_scouting(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.category == MessageCategory::ScoutReport)
        .expect("Should have a scout report for own player");

    let report = msg
        .context
        .scout_report
        .as_ref()
        .expect("Should have scout_report data");

    assert_eq!(report.player_id, "p1");
    // With noise_range = 0, fuzzed values should match original exactly
    assert_eq!(
        report.mechanics,
        Some(attrs.mechanics),
        "Mechanics should be exact for own player"
    );
    assert_eq!(
        report.laning,
        Some(attrs.laning),
        "Laning should be exact for own player"
    );
    assert_eq!(
        report.teamfighting,
        Some(attrs.teamfighting),
        "Teamfighting should be exact for own player"
    );
    assert_eq!(
        report.macro_,
        Some(attrs.macro_play),
        "Macro should be exact for own player"
    );
    assert_eq!(
        report.champion_pool,
        Some(attrs.champion_pool),
        "Champion pool should be exact for own player"
    );
    assert_eq!(
        report.discipline,
        Some(attrs.discipline),
        "Discipline should be exact for own player"
    );
    assert_eq!(
        report.condition,
        Some(90),
        "Condition should be exact for own player"
    );
    assert_eq!(
        report.morale,
        Some(75),
        "Morale should be exact for own player"
    );
}

#[test]
fn report_has_i18n_keys() {
    let mut game = make_game();
    send_scout(&mut game, "scout1", "p2").unwrap();
    complete_scouting(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.category == MessageCategory::ScoutReport)
        .unwrap();

    assert!(msg.subject_key.is_some());
    assert!(msg.body_key.is_some());
    assert!(msg.sender_key.is_some());
    assert!(msg.sender_role_key.is_some());

    let report = msg.context.scout_report.as_ref().unwrap();
    assert!(
        report.rating_key.starts_with("common.scoutRatings."),
        "rating_key should be an i18n key: {}",
        report.rating_key
    );
    assert!(
        report.potential_key.starts_with("common.scoutPotential."),
        "potential_key should be an i18n key: {}",
        report.potential_key
    );
    assert!(
        report.confidence_key.starts_with("common.scoutConfidence."),
        "confidence_key should be an i18n key: {}",
        report.confidence_key
    );
}

// ---------------------------------------------------------------------------
// Discovery mechanic — attribute reveal count
// ---------------------------------------------------------------------------

fn count_revealed(report: &ScoutReportData) -> usize {
    [
        report.mechanics,
        report.laning,
        report.teamfighting,
        report.macro_,
        report.champion_pool,
        report.discipline,
    ]
    .iter()
    .filter(|a| a.is_some())
    .count()
}

#[test]
fn high_ability_scout_reveals_all_attrs() {
    let mut game = make_game();
    // Scout already has judging_ability = 80
    send_scout(&mut game, "scout1", "p2").unwrap();
    complete_scouting(&mut game);

    let report = game
        .messages
        .iter()
        .find(|m| m.category == MessageCategory::ScoutReport)
        .unwrap()
        .context
        .scout_report
        .as_ref()
        .unwrap();

    assert_eq!(
        count_revealed(report),
        6,
        "High ability scout should reveal all 6 attrs"
    );
    assert!(
        report.condition.is_some(),
        "High ability should reveal condition"
    );
    assert!(report.morale.is_some(), "High ability should reveal morale");
    assert_eq!(report.confidence_key, "common.scoutConfidence.high");
}

#[test]
fn medium_ability_scout_reveals_5_attrs() {
    let mut game = make_game();
    game.staff[0].attributes.judging_ability = 65;
    send_scout(&mut game, "scout1", "p2").unwrap();
    complete_scouting(&mut game);

    let report = game
        .messages
        .iter()
        .find(|m| m.category == MessageCategory::ScoutReport)
        .unwrap()
        .context
        .scout_report
        .as_ref()
        .unwrap();

    assert_eq!(
        count_revealed(report),
        5,
        "Medium ability scout should reveal 5 attrs"
    );
    assert!(
        report.condition.is_some(),
        "Medium ability should reveal condition"
    );
    assert!(
        report.morale.is_none(),
        "Medium ability should NOT reveal morale"
    );
    assert_eq!(report.confidence_key, "common.scoutConfidence.moderate");
}

#[test]
fn low_ability_scout_reveals_3_attrs() {
    let mut game = make_game();
    game.staff[0].attributes.judging_ability = 45;
    send_scout(&mut game, "scout1", "p2").unwrap();
    complete_scouting(&mut game);

    let report = game
        .messages
        .iter()
        .find(|m| m.category == MessageCategory::ScoutReport)
        .unwrap()
        .context
        .scout_report
        .as_ref()
        .unwrap();

    assert_eq!(
        count_revealed(report),
        3,
        "Low ability scout should reveal 3 attrs"
    );
    assert!(
        report.condition.is_none(),
        "Low ability should NOT reveal condition"
    );
    assert!(
        report.morale.is_none(),
        "Low ability should NOT reveal morale"
    );
    assert_eq!(report.confidence_key, "common.scoutConfidence.low");
}

#[test]
fn very_low_ability_scout_reveals_2_attrs() {
    let mut game = make_game();
    game.staff[0].attributes.judging_ability = 25;
    send_scout(&mut game, "scout1", "p2").unwrap();
    complete_scouting(&mut game);

    let report = game
        .messages
        .iter()
        .find(|m| m.category == MessageCategory::ScoutReport)
        .unwrap()
        .context
        .scout_report
        .as_ref()
        .unwrap();

    assert_eq!(
        count_revealed(report),
        2,
        "Very low ability scout should reveal 2 attrs"
    );
    assert_eq!(report.confidence_key, "common.scoutConfidence.low");
}

// ---------------------------------------------------------------------------
// Fuzzed attribute accuracy
// ---------------------------------------------------------------------------

#[test]
fn high_ability_scout_attrs_are_close_to_real() {
    // With judging_ability 80+, noise range is ±2
    // Run multiple times to check range statistically
    for _ in 0..10 {
        let mut game = make_game();
        send_scout(&mut game, "scout1", "p2").unwrap();
        complete_scouting(&mut game);

        let report = game
            .messages
            .iter()
            .find(|m| m.category == MessageCategory::ScoutReport)
            .unwrap()
            .context
            .scout_report
            .as_ref()
            .unwrap();

        // Real mechanics is 74, with noise ±2 → should be in [72, 76]
        if let Some(mechanics) = report.mechanics {
            assert!(
                (72..=76).contains(&mechanics),
                "High-ability fuzzed mechanics {} should be close to real value 74",
                mechanics
            );
        }
    }
}

#[test]
fn low_ability_scout_attrs_have_more_noise() {
    // With judging_ability 25, noise range is ±12
    let mut game = make_game();
    game.staff[0].attributes.judging_ability = 25;
    send_scout(&mut game, "scout1", "p2").unwrap();
    complete_scouting(&mut game);

    let report = game
        .messages
        .iter()
        .find(|m| m.category == MessageCategory::ScoutReport)
        .unwrap()
        .context
        .scout_report
        .as_ref()
        .unwrap();

    // Just verify values are in valid range (1-99)
    for val in [
        report.pace,
        report.laning,
        report.passing,
        report.mechanics,
        report.defending,
        report.physical,
    ] {
        if let Some(v) = val {
            assert!(v >= 1 && v <= 99, "Fuzzed value {} should be in [1, 99]", v);
        }
    }
}

// ---------------------------------------------------------------------------
// Potential assessment depends on judging_potential
// ---------------------------------------------------------------------------

#[test]
fn high_judging_potential_gives_specific_assessment() {
    let mut game = make_game();
    game.staff[0].attributes.judging_potential = 75;
    send_scout(&mut game, "scout1", "p2").unwrap();
    complete_scouting(&mut game);

    let report = game
        .messages
        .iter()
        .find(|m| m.category == MessageCategory::ScoutReport)
        .unwrap()
        .context
        .scout_report
        .as_ref()
        .unwrap();

    // With high judging_potential (>=70), should get a specific potential key
    assert!(
        report.potential_key != "common.scoutPotential.unclear",
        "High judging_potential should give a specific assessment, got: {}",
        report.potential_key
    );
}

#[test]
fn low_judging_potential_gives_unclear_assessment() {
    let mut game = make_game();
    game.staff[0].attributes.judging_potential = 40;
    send_scout(&mut game, "scout1", "p2").unwrap();
    complete_scouting(&mut game);

    let report = game
        .messages
        .iter()
        .find(|m| m.category == MessageCategory::ScoutReport)
        .unwrap()
        .context
        .scout_report
        .as_ref()
        .unwrap();

    assert_eq!(
        report.potential_key, "common.scoutPotential.unclear",
        "Low judging_potential should give unclear assessment"
    );
}

// ---------------------------------------------------------------------------
// Assignment removal after completion
// ---------------------------------------------------------------------------

#[test]
fn completed_assignment_is_removed() {
    let mut game = make_game();
    send_scout(&mut game, "scout1", "p2").unwrap();
    assert_eq!(game.scouting_assignments.len(), 1);
    complete_scouting(&mut game);
    assert_eq!(
        game.scouting_assignments.len(),
        0,
        "Completed assignments should be removed"
    );
}
