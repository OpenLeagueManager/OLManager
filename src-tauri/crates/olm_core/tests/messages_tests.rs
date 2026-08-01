use olm_core::messages;

// ---------------------------------------------------------------------------
// welcome_message
// ---------------------------------------------------------------------------

#[test]
fn welcome_message_has_correct_fields() {
    let msg = messages::welcome_message("Test FC", "team1", "2025-06-01", "en");
    assert_eq!(msg.id, "welcome_team1");
    assert!(
        msg.subject.contains("Test FC"),
        "Subject should contain team name: {}",
        msg.subject
    );
    assert!(
        msg.body.contains("Test FC"),
        "Body should contain team name"
    );
    assert_eq!(msg.sender_role, "Chairman");
    assert!(!msg.actions.is_empty(), "Should have actions");
}

#[test]
fn welcome_message_has_i18n_keys() {
    let msg = messages::welcome_message("Test FC", "team1", "2025-06-01", "en");
    assert!(msg.subject_key.is_some(), "Should have subject i18n key");
    assert!(msg.body_key.is_some(), "Should have body i18n key");
    assert!(msg.sender_key.is_some(), "Should have sender i18n key");
}

#[test]
fn welcome_message_has_context() {
    let msg = messages::welcome_message("Test FC", "team1", "2025-06-01", "en");
    assert_eq!(msg.context.team_id.as_deref(), Some("team1"));
}

// ---------------------------------------------------------------------------
// season_schedule_message
// ---------------------------------------------------------------------------

#[test]
fn season_schedule_message_fields() {
    let msg = messages::season_schedule_message("Premier League", "2025-08-10", "2025-06-01");
    assert_eq!(msg.id, "season_1");
    assert!(msg.body.contains("Premier League"));
    assert!(msg.body.contains("2025-08-10"));
    assert_eq!(msg.sender_role, "Competition Secretary");
}

#[test]
fn season_schedule_has_view_action() {
    let msg = messages::season_schedule_message("Premier League", "2025-08-10", "2025-06-01");
    let has_view = msg.actions.iter().any(|a| a.id == "view_schedule");
    assert!(has_view, "Should have view schedule action");
}

// ---------------------------------------------------------------------------
// pre_match_message
// ---------------------------------------------------------------------------

#[test]
fn pre_match_message_home() {
    let msg = messages::pre_match_message(
        "f1",
        "Rival FC",
        "team2",
        true,
        5,
        "2025-09-15",
        "2025-09-12",
    );
    assert_eq!(msg.id, "prematch_f1");
    assert!(msg.subject.contains("Rival FC"));
    assert!(msg.subject.contains("H"), "Home match should show (H)");
    assert!(msg.body.contains("home") || msg.body.contains("Home"));
}

#[test]
fn pre_match_message_away() {
    let msg = messages::pre_match_message(
        "f2",
        "Rival FC",
        "team2",
        false,
        6,
        "2025-09-22",
        "2025-09-19",
    );
    assert!(msg.subject.contains("A"), "Away match should show (A)");
    assert!(msg.body.contains("away") || msg.body.contains("Away"));
}

#[test]
fn pre_match_has_tactics_action() {
    let msg = messages::pre_match_message(
        "f1",
        "Rival FC",
        "team2",
        true,
        5,
        "2025-09-15",
        "2025-09-12",
    );
    let has_tactics = msg.actions.iter().any(|a| a.id == "set_tactics");
    let has_scout = msg.actions.iter().any(|a| a.id == "view_opponent");
    assert!(has_tactics, "Should have set tactics action");
    assert!(has_scout, "Should have scout opponent action");
}

#[test]
fn pre_match_context_has_fixture_id() {
    let msg = messages::pre_match_message(
        "f1",
        "Rival FC",
        "team2",
        true,
        5,
        "2025-09-15",
        "2025-09-12",
    );
    assert_eq!(msg.context.fixture_id.as_deref(), Some("f1"));
}

// ---------------------------------------------------------------------------
// match_result_message
// ---------------------------------------------------------------------------

#[test]
fn match_result_victory() {
    let msg = messages::match_result_message(
        "f1",
        "Test FC",
        "Rival FC",
        3,
        1,
        "team1",
        "team2",
        "team1",
        10,
        "2025-10-01",
    );
    assert_eq!(msg.id, "result_f1");
    assert!(
        msg.subject.contains("Victory"),
        "Should show Victory: {}",
        msg.subject
    );
    assert!(msg.body.contains("3") && msg.body.contains("1"));
}

#[test]
fn match_result_defeat() {
    let msg = messages::match_result_message(
        "f2",
        "Test FC",
        "Rival FC",
        0,
        2,
        "team1",
        "team2",
        "team1",
        11,
        "2025-10-08",
    );
    assert!(
        msg.subject.contains("Defeat"),
        "Should show Defeat: {}",
        msg.subject
    );
}

#[test]
fn match_result_draw() {
    let msg = messages::match_result_message(
        "f3",
        "Test FC",
        "Rival FC",
        1,
        1,
        "team1",
        "team2",
        "team1",
        12,
        "2025-10-15",
    );
    assert!(
        msg.subject.contains("Draw"),
        "Should show Draw: {}",
        msg.subject
    );
}

#[test]
fn match_result_away_perspective() {
    // User is team2 (away), and they won 0-2
    let msg = messages::match_result_message(
        "f4",
        "Home FC",
        "Test FC",
        0,
        2,
        "team1",
        "team2",
        "team2",
        13,
        "2025-10-22",
    );
    assert!(
        msg.subject.contains("Victory"),
        "Away win should show Victory: {}",
        msg.subject
    );
}

#[test]
fn match_result_has_context_with_score() {
    let msg = messages::match_result_message(
        "f1",
        "Test FC",
        "Rival FC",
        3,
        1,
        "team1",
        "team2",
        "team1",
        10,
        "2025-10-01",
    );
    assert_eq!(msg.context.fixture_id.as_deref(), Some("f1"));
    let result = msg.context.match_result.as_ref().unwrap();
    assert_eq!(result.home_goals, 3);
    assert_eq!(result.away_goals, 1);
}

#[test]
fn victory_has_normal_priority() {
    let msg =
        messages::match_result_message("f1", "A", "B", 2, 0, "t1", "t2", "t1", 1, "2025-01-01");
    assert_eq!(
        msg.priority,
        olm_core::domain::message::MessagePriority::Normal
    );
}

#[test]
fn defeat_has_high_priority() {
    let msg =
        messages::match_result_message("f1", "A", "B", 0, 2, "t1", "t2", "t1", 1, "2025-01-01");
    assert_eq!(
        msg.priority,
        olm_core::domain::message::MessagePriority::High
    );
}

// ---------------------------------------------------------------------------
// staff_advice_message
// ---------------------------------------------------------------------------

#[test]
fn staff_advice_message_fields() {
    let msg = messages::staff_advice_message("Test FC", "team1", "2025-06-01");
    assert_eq!(msg.id, "staff_advice_1");
    assert!(msg.body.contains("Coach"));
    assert!(msg.body.contains("Physio"));
    assert!(msg.body.contains("Scouts"));
    assert_eq!(msg.sender_role, "Assistant Manager");
}

#[test]
fn staff_advice_has_view_action() {
    let msg = messages::staff_advice_message("Test FC", "team1", "2025-06-01");
    let has_view = msg.actions.iter().any(|a| a.id == "view_staff");
    assert!(has_view, "Should have view staff action");
}

// ---------------------------------------------------------------------------
// board_expectations_message
// ---------------------------------------------------------------------------

#[test]
fn board_expectations_message_fields() {
    let msg = messages::board_expectations_message("Test FC", "team1", "2025-06-01");
    assert_eq!(msg.id, "board_expect_1");
    assert!(msg.subject.contains("Test FC"));
    assert!(msg.body.contains("LEC table"));
    assert!(msg.body.contains("roster pipeline"));
    assert_eq!(msg.sender_role, "Chairman");
}

#[test]
fn board_expectations_has_ack_action() {
    let msg = messages::board_expectations_message("Test FC", "team1", "2025-06-01");
    let has_ack = msg.actions.iter().any(|a| a.id == "ack_objectives");
    assert!(has_ack, "Should have acknowledge action");
}

// ---------------------------------------------------------------------------
// transfer_complete_message
// ---------------------------------------------------------------------------

#[test]
fn transfer_message_millions() {
    let msg = messages::transfer_complete_message("John Star", 5_500_000, "2025-08-01");
    assert!(msg.subject.contains("John Star"));
    assert!(
        msg.body.contains("5.5M"),
        "Should format millions: {}",
        msg.body
    );
    assert_eq!(msg.sender_role, "Director of Football");
}

#[test]
fn transfer_message_thousands() {
    let msg = messages::transfer_complete_message("Young Player", 250_000, "2025-08-01");
    assert!(
        msg.body.contains("250K"),
        "Should format thousands: {}",
        msg.body
    );
}

#[test]
fn transfer_message_small_fee() {
    let msg = messages::transfer_complete_message("Free Agent", 500, "2025-08-01");
    assert!(
        msg.body.contains("€500"),
        "Should format small fee: {}",
        msg.body
    );
}

#[test]
fn transfer_message_has_unique_id() {
    let msg1 = messages::transfer_complete_message("A", 1000, "2025-08-01");
    let msg2 = messages::transfer_complete_message("B", 2000, "2025-08-01");
    assert_ne!(msg1.id, msg2.id, "Transfer messages should have unique IDs");
}
