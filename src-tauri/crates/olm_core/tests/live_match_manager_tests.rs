use chrono::{TimeZone, Utc};
use olm_core::clock::GameClock;
use olm_core::domain::league::{Fixture, FixtureStatus, League, MatchType, StandingEntry};
use olm_core::domain::manager::Manager;
use olm_core::domain::player::{Player, PlayerAttributes};
use olm_core::domain::stats::LolRole;
use olm_core::domain::team::Team;
use olm_core::game::Game;
use olm_core::live_match_manager::{self, MatchMode};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn default_attrs() -> PlayerAttributes {
    PlayerAttributes {
        mental_resilience: 65,
        champion_pool: 65,
        laning: 65,
        mechanics: 65,
        macro_play: 65,
        consistency: 65,
        discipline: 65,
        teamfighting: 65,
        shotcalling: 50,
    }
}

fn make_player(id: &str, name: &str, team_id: &str, pos: LolRole) -> Player {
    let attrs = default_attrs();
    let mut p = Player::new(
        id.to_string(),
        name.to_string(),
        format!("Full {}", name),
        "1995-01-01".to_string(),
        "GB".to_string(),
        pos,
        attrs,
    );
    p.team_id = Some(team_id.to_string());
    p.morale = 70;
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

/// Build a squad of players for a LoL team (5 starters + bench).
fn make_squad(team_id: &str) -> Vec<Player> {
    let mut players = Vec::new();
    // 2 Top
    for i in 0..2 {
        players.push(make_player(
            &format!("{}_top{}", team_id, i),
            &format!("Top{}", i),
            team_id,
            LolRole::Top,
        ));
    }
    // 2 Jungle
    for i in 0..2 {
        players.push(make_player(
            &format!("{}_jng{}", team_id, i),
            &format!("Jng{}", i),
            team_id,
            LolRole::Jungle,
        ));
    }
    // 2 Mid
    for i in 0..2 {
        players.push(make_player(
            &format!("{}_mid{}", team_id, i),
            &format!("Mid{}", i),
            team_id,
            LolRole::Mid,
        ));
    }
    // 2 ADC
    for i in 0..2 {
        players.push(make_player(
            &format!("{}_adc{}", team_id, i),
            &format!("Adc{}", i),
            team_id,
            LolRole::Adc,
        ));
    }
    // 2 Support
    for i in 0..2 {
        players.push(make_player(
            &format!("{}_sup{}", team_id, i),
            &format!("Sup{}", i),
            team_id,
            LolRole::Support,
        ));
    }
    players
}

fn make_game_with_fixture() -> Game {
    let date = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
    let clock = GameClock::new(date);
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

    let mut players = make_squad("team1");
    players.extend(make_squad("team2"));

    let fixture = Fixture {
        id: "fix1".to_string(),
        matchday: 1,
        date: "2025-06-15".to_string(),
        home_team_id: "team1".to_string(),
        away_team_id: "team2".to_string(),
        match_type: MatchType::League,
        best_of: 1,
        status: FixtureStatus::Scheduled,
        result: None,
    };

    let league = League {
        id: "league1".to_string(),
        name: "Test League".to_string(),
        season: 1,
        competition_id: None,
        fixtures: vec![fixture],
        standings: vec![
            StandingEntry::new("team1".to_string()),
            StandingEntry::new("team2".to_string()),
        ],
        ..Default::default()
    };

    let mut game = Game::new(clock, manager, vec![team1, team2], players, vec![], vec![]);
    game.leagues = vec![league];
    game
}

// ---------------------------------------------------------------------------
// create_live_match
// ---------------------------------------------------------------------------

#[test]
fn create_live_match_succeeds() {
    let game = make_game_with_fixture();
    let session = live_match_manager::create_live_match(&game, 0, MatchMode::Live, false);
    assert!(
        session.is_ok(),
        "Should create live match: {:?}",
        session.err()
    );
    let session = session.unwrap();
    assert_eq!(session.home_team_id, "team1");
    assert_eq!(session.away_team_id, "team2");
    assert_eq!(session.mode, MatchMode::Live);
    assert_eq!(session.fixture_index, 0);
    assert!(!session.is_finished());
}

#[test]
fn create_live_match_user_side_home() {
    let game = make_game_with_fixture();
    let session = live_match_manager::create_live_match(&game, 0, MatchMode::Live, false).unwrap();
    assert_eq!(session.user_side, Some(olm_core::engine::Side::Home));
}

#[test]
fn create_live_match_user_side_away() {
    let mut game = make_game_with_fixture();
    game.manager.team_id = Some("team2".to_string());
    let session = live_match_manager::create_live_match(&game, 0, MatchMode::Live, false).unwrap();
    assert_eq!(session.user_side, Some(olm_core::engine::Side::Away));
}

#[test]
fn create_live_match_user_side_none_neutral() {
    let mut game = make_game_with_fixture();
    game.manager.team_id = Some("team3".to_string());
    let session =
        live_match_manager::create_live_match(&game, 0, MatchMode::Spectator, false).unwrap();
    assert_eq!(session.user_side, None);
}

#[test]
fn create_live_match_no_league_errors() {
    let mut game = make_game_with_fixture();
    game.leagues.clear();
    let result = live_match_manager::create_live_match(&game, 0, MatchMode::Live, false);
    assert!(result.is_err());
}

#[test]
fn create_live_match_bad_fixture_index_errors() {
    let game = make_game_with_fixture();
    let result = live_match_manager::create_live_match(&game, 99, MatchMode::Live, false);
    assert!(result.is_err());
}

#[test]
fn create_live_match_errors_when_home_team_has_less_than_five_players() {
    let mut game = make_game_with_fixture();
    for player in game
        .players
        .iter_mut()
        .filter(|player| player.team_id.as_deref() == Some("team1"))
        .skip(4)
    {
        player.team_id = None;
    }

    let result = live_match_manager::create_live_match(&game, 0, MatchMode::Live, false);
    assert!(result.is_err());
    match result {
        Err(error) => assert!(error.contains("Cannot start match: incomplete lineup")),
        Ok(_) => panic!("expected create_live_match to fail with incomplete lineup"),
    }
}

// ---------------------------------------------------------------------------
// LiveMatchSession stepping
// ---------------------------------------------------------------------------

#[test]
fn step_advances_match() {
    let game = make_game_with_fixture();
    let mut session =
        live_match_manager::create_live_match(&game, 0, MatchMode::Spectator, false).unwrap();

    let result = session.step();
    assert!(
        !result.is_finished,
        "Match should not be finished after 1 step"
    );
}

#[test]
fn step_many_returns_requested_count() {
    let game = make_game_with_fixture();
    let mut session =
        live_match_manager::create_live_match(&game, 0, MatchMode::Spectator, false).unwrap();

    let results = session.step_many(10);
    assert!(results.len() >= 1 && results.len() <= 10);
}

#[test]
fn run_to_completion_finishes() {
    let game = make_game_with_fixture();
    let mut session =
        live_match_manager::create_live_match(&game, 0, MatchMode::Instant, false).unwrap();

    let results = session.run_to_completion();
    assert!(!results.is_empty());
    assert!(results.last().unwrap().is_finished);
    assert!(session.is_finished());
}

#[test]
fn snapshot_returns_valid_state() {
    let game = make_game_with_fixture();
    let session = live_match_manager::create_live_match(&game, 0, MatchMode::Live, false).unwrap();
    let snap = session.snapshot();
    // Snapshot should have non-empty team names
    assert!(!snap.home_team.name.is_empty());
    assert!(!snap.away_team.name.is_empty());
}

#[test]
fn step_many_stops_at_finish() {
    let game = make_game_with_fixture();
    let mut session =
        live_match_manager::create_live_match(&game, 0, MatchMode::Instant, false).unwrap();

    // Request way more steps than a match has
    let results = session.step_many(500);
    assert!(results.last().unwrap().is_finished);
    // Should have stopped early
    assert!(results.len() < 500);
}

// ---------------------------------------------------------------------------
// auto_select_team_roles
// ---------------------------------------------------------------------------

#[test]
fn auto_select_team_roles_picks_captain() {
    let game = make_game_with_fixture();
    let player_ids: Vec<String> = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some("team1"))
        .map(|p| p.id.clone())
        .collect();

    let (captain, shotcaller) = live_match_manager::auto_select_team_roles(&game, &player_ids);

    assert!(captain.is_some(), "Should pick a captain");
    assert!(shotcaller.is_some(), "Should pick a shotcaller");
}

#[test]
fn auto_select_team_roles_empty_ids_returns_none() {
    let game = make_game_with_fixture();
    let (captain, shotcaller) = live_match_manager::auto_select_team_roles(&game, &[]);
    assert!(captain.is_none());
    assert!(shotcaller.is_none());
}

#[test]
fn auto_select_team_roles_prefers_high_leadership_captain() {
    let mut game = make_game_with_fixture();
    // Give one player very high leadership
    let leader = game
        .players
        .iter_mut()
        .find(|p| p.id == "team1_mid0")
        .unwrap();
    leader.attributes.shotcalling = 99;
    leader.attributes.teamfighting = 99;

    let player_ids: Vec<String> = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some("team1"))
        .map(|p| p.id.clone())
        .collect();

    let (captain, _) = live_match_manager::auto_select_team_roles(&game, &player_ids);
    assert_eq!(captain, Some("team1_mid0".to_string()));
}

// ---------------------------------------------------------------------------
// Match modes
// ---------------------------------------------------------------------------

#[test]
fn all_match_modes_create_successfully() {
    let game = make_game_with_fixture();
    for mode in [MatchMode::Live, MatchMode::Spectator, MatchMode::Instant] {
        let session = live_match_manager::create_live_match(&game, 0, mode, false);
        assert!(session.is_ok(), "Mode {:?} should work", mode);
    }
}

#[test]
fn instant_mode_completes() {
    let game = make_game_with_fixture();
    let mut session =
        live_match_manager::create_live_match(&game, 0, MatchMode::Instant, false).unwrap();
    let results = session.run_to_completion();
    assert!(session.is_finished());
    assert!(
        results.len() >= 55,
        "Match should reach time limit (~60 min)"
    );
}

// ---------------------------------------------------------------------------
// Extra time
// ---------------------------------------------------------------------------

#[test]
fn extra_time_flag_passed_through() {
    let game = make_game_with_fixture();
    // Just verify it doesn't crash with extra_time=true
    let session = live_match_manager::create_live_match(&game, 0, MatchMode::Instant, true);
    assert!(session.is_ok());
}
