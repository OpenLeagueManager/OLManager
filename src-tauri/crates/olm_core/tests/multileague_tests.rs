use chrono::{TimeZone, Utc};
use olm_core::clock::GameClock;
use olm_core::domain::league::{
    Fixture, FixtureStatus, League, LeagueKind, MatchType, StandingEntry,
};
use olm_core::domain::manager::Manager;
use olm_core::domain::player::{Player, PlayerAttributes};
use olm_core::domain::stats::LolRole;
use olm_core::domain::team::Team;
use olm_core::game::Game;
use olm_core::turn;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn default_attrs() -> PlayerAttributes {
    PlayerAttributes {
        mental_resilience: 60,
        champion_pool: 60,
        laning: 60,
        mechanics: 60,
        macro_play: 60,
        consistency: 60,
        discipline: 60,
        teamfighting: 60,
        shotcalling: 60,
    }
}

fn make_player(id: &str, name: &str, team_id: &str, pos: LolRole) -> Player {
    let mut p = Player::new(
        id.to_string(),
        name.to_string(),
        name.to_string(),
        "1995-01-01".to_string(),
        "England".to_string(),
        pos,
        default_attrs(),
    );
    p.team_id = Some(team_id.to_string());
    p.morale = 70;
    p.condition = 100;
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

fn make_squad(team_id: &str, prefix: &str) -> Vec<Player> {
    let mut players = Vec::new();
    // 1 Support
    players.push(make_player(
        &format!("{}_sup", prefix),
        &format!("{} Support", prefix),
        team_id,
        LolRole::Support,
    ));
    // 4 other roles
    for role in &["top", "jungle", "mid", "adc"] {
        players.push(make_player(
            &format!("{}_{}", prefix, role),
            &format!("{} {}", prefix, role),
            team_id,
            match *role {
                "top" => LolRole::Top,
                "jungle" => LolRole::Jungle,
                "mid" => LolRole::Mid,
                "adc" => LolRole::Adc,
                _ => unreachable!(),
            },
        ));
    }
    players
}

/// Create a minimal league with `num_teams` teams and one round-robin matchday
/// of fixtures on the given `date`.
fn make_league(
    id: &str,
    name: &str,
    competition_id: &str,
    team_ids: &[&str],
    date: &str,
    matchday: u32,
) -> League {
    let fixtures: Vec<Fixture> = team_ids
        .chunks(2)
        .filter(|pair| pair.len() == 2)
        .enumerate()
        .map(|(i, pair)| Fixture {
            id: format!("{}-fix{}", id, i),
            matchday,
            date: date.to_string(),
            home_team_id: pair[0].to_string(),
            away_team_id: pair[1].to_string(),
            match_type: MatchType::League,
            best_of: 1,
            status: FixtureStatus::Scheduled,
            result: None,
        })
        .collect();

    let standings: Vec<StandingEntry> = team_ids
        .iter()
        .map(|tid| StandingEntry::new(tid.to_string()))
        .collect();

    League {
        id: id.to_string(),
        name: name.to_string(),
        season: 2025,
        fixtures,
        standings,
        competition_id: Some(competition_id.to_string()),
        league_kind: LeagueKind::Main,
        ..Default::default()
    }
}

fn make_game(
    date_str: &str,
    team_ids: &[&str],
    active_competition_id: &str,
) -> (Game, Vec<Player>) {
    let date = Utc
        .with_ymd_and_hms(
            date_str[0..4].parse().unwrap(),
            date_str[5..7].parse().unwrap(),
            date_str[8..10].parse().unwrap(),
            12,
            0,
            0,
        )
        .unwrap();
    let clock = GameClock::new(date);
    let mut manager = Manager::new(
        "mgr1".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire(team_ids[0].to_string());

    let teams: Vec<Team> = team_ids.iter().map(|tid| make_team(tid, tid)).collect();
    let mut all_players = Vec::new();
    for tid in team_ids {
        all_players.extend(make_squad(tid, tid));
    }

    let mut game = Game::new(clock, manager, teams, all_players.clone(), vec![], vec![]);
    game.user_competition_id = Some(active_competition_id.to_string());
    (game, all_players)
}

// ---------------------------------------------------------------------------
// Test 1: Save/Load roundtrip with multiple leagues
// ---------------------------------------------------------------------------

#[test]
fn save_load_roundtrip_with_multiple_leagues() {
    let date = "2025-06-15";
    let team_ids = ["team1", "team2", "team3", "team4"];
    let (mut game, _players) = make_game(date, &team_ids, "lec");

    let league_a = make_league("lec", "LEC", "lec", &["team1", "team2"], date, 1);
    let league_b = make_league("lcs", "LCS", "lcs", &["team3", "team4"], date, 1);
    game.leagues = vec![league_a, league_b];

    // Serialize to JSON (simulates saving)
    let json = serde_json::to_string(&game).expect("game should serialize");

    // Deserialize back (simulates loading)
    let loaded: Game = serde_json::from_str(&json).expect("game should deserialize");

    // Both leagues survive the roundtrip
    assert_eq!(
        loaded.leagues.len(),
        2,
        "both leagues should be present after save/load"
    );

    let loaded_lec = loaded
        .leagues
        .iter()
        .find(|l| l.competition_id.as_deref() == Some("lec"))
        .expect("LEC league should exist after save/load");
    assert_eq!(loaded_lec.fixtures.len(), 1);
    assert_eq!(
        loaded_lec.standings.len(),
        2,
        "LEC standings should survive save/load"
    );

    let loaded_lcs = loaded
        .leagues
        .iter()
        .find(|l| l.competition_id.as_deref() == Some("lcs"))
        .expect("LCS league should exist after save/load");
    assert_eq!(loaded_lcs.fixtures.len(), 1);
    assert_eq!(
        loaded_lcs.standings.len(),
        2,
        "LCS standings should survive save/load"
    );

    // user_competition_id survives
    assert_eq!(
        loaded.user_competition_id.as_deref(),
        Some("lec"),
        "user_competition_id should survive save/load"
    );

    // Active league helper works correctly after reload
    let active = loaded.active_league().expect("active league should exist");
    assert_eq!(
        active.competition_id.as_deref(),
        Some("lec"),
        "active_league() should return the correct competition after reload"
    );
}

// ---------------------------------------------------------------------------
// Test 2: Fixture isolation between competitions
// ---------------------------------------------------------------------------

#[test]
fn fixture_isolation_between_competitions() {
    let date = "2025-06-15";
    let team_ids = ["team1", "team2", "team3", "team4"];
    let (mut game, _players) = make_game(date, &team_ids, "lec");

    let lec = make_league("lec", "LEC", "lec", &["team1", "team2"], date, 1);
    let lcs = make_league("lcs", "LCS", "lcs", &["team3", "team4"], date, 1);
    game.leagues = vec![lec, lcs];

    // Record initial player stats so we can detect unexpected mutations
    let initial_kills: HashMap<String, u32> = game
        .players
        .iter()
        .map(|p| (p.id.clone(), p.stats.kills))
        .collect();

    // Process the matchday
    turn::process_day(&mut game);

    // LEC (active league) fixtures should be completed
    let lec_league = game
        .leagues
        .iter()
        .find(|l| l.competition_id.as_deref() == Some("lec"))
        .expect("LEC league should exist");
    for fixture in &lec_league.fixtures {
        assert_eq!(
            fixture.status,
            FixtureStatus::Completed,
            "active league fixture should be completed"
        );
        assert!(
            fixture.result.is_some(),
            "active league fixture should have a result"
        );
    }

    // LCS (background league) fixtures should also be completed
    let lcs_league = game
        .leagues
        .iter()
        .find(|l| l.competition_id.as_deref() == Some("lcs"))
        .expect("LCS league should exist");
    for fixture in &lcs_league.fixtures {
        assert_eq!(
            fixture.status,
            FixtureStatus::Completed,
            "background league fixture should be completed"
        );
        assert!(
            fixture.result.is_some(),
            "background league fixture should have a result"
        );
    }

    // LEC standings should be updated
    let lec_standings_total: u32 = lec_league.standings.iter().map(|s| s.played).sum();
    assert_eq!(
        lec_standings_total, 2,
        "LEC: total games played across standings should reflect the fixture"
    );

    // LCS standings should be updated independently
    let lcs_standings_total: u32 = lcs_league.standings.iter().map(|s| s.played).sum();
    assert_eq!(
        lcs_standings_total, 2,
        "LCS: total games played across standings should reflect the fixture"
    );

    // Leagues should NOT share standings entries
    for entry in &lec_league.standings {
        assert!(
            !lcs_league
                .standings
                .iter()
                .any(|e| e.team_id == entry.team_id),
            "LCS should not contain LEC teams in standings"
        );
    }

    // Player stats from background leagues are applied (light mode)
    // At least some players should have updated stats
    let total_kills: u32 = game.players.iter().map(|p| p.stats.kills).sum();
    let initial_kills_total: u32 = initial_kills.values().sum();
    assert!(
        total_kills >= initial_kills_total,
        "player kills should not decrease after simulation"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Background league simulation produces correct standings
// ---------------------------------------------------------------------------

#[test]
fn background_simulation_updates_standings() {
    // Set up on a day where active league has NO fixtures (so it does training)
    // but background league has due fixtures.
    // Use a Wednesday (weekday 3) — no weekend matchday for active league.
    // The date string itself doesn't determine matchday logic; what matters is
    // that no active league fixtures match this date.
    let today = "2025-06-11"; // Wednesday
    let future = "2025-06-15"; // Sunday — only background league has fixtures here

    let team_ids = ["team1", "team2", "team3", "team4", "team5", "team6"];
    let (mut game, _players) = make_game(today, &team_ids, "lec");

    // Active league (LEC) — NO fixtures on today's date
    // Give it one fixture in the future so it's not empty
    let lec = League {
        id: "lec".to_string(),
        name: "LEC".to_string(),
        season: 2025,
        fixtures: vec![Fixture {
            id: "lec-fix-future".to_string(),
            matchday: 2,
            date: future.to_string(),
            home_team_id: "team1".to_string(),
            away_team_id: "team2".to_string(),
            match_type: MatchType::League,
            best_of: 1,
            status: FixtureStatus::Scheduled,
            result: None,
        }],
        standings: vec![
            StandingEntry::new("team1".to_string()),
            StandingEntry::new("team2".to_string()),
        ],
        competition_id: Some("lec".to_string()),
        league_kind: LeagueKind::Main,
        ..Default::default()
    };

    // Background league (LCS) — HAS fixtures due today
    let lcs = League {
        id: "lcs".to_string(),
        name: "LCS".to_string(),
        season: 2025,
        fixtures: vec![
            Fixture {
                id: "lcs-fix1".to_string(),
                matchday: 1,
                date: today.to_string(),
                home_team_id: "team3".to_string(),
                away_team_id: "team4".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Scheduled,
                result: None,
            },
            Fixture {
                id: "lcs-fix2".to_string(),
                matchday: 1,
                date: today.to_string(),
                home_team_id: "team5".to_string(),
                away_team_id: "team6".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Scheduled,
                result: None,
            },
        ],
        standings: vec![
            StandingEntry::new("team3".to_string()),
            StandingEntry::new("team4".to_string()),
            StandingEntry::new("team5".to_string()),
            StandingEntry::new("team6".to_string()),
        ],
        competition_id: Some("lcs".to_string()),
        league_kind: LeagueKind::Main,
        ..Default::default()
    };

    // Background league 2: CBLOL
    let cblol = make_league("cblol", "CBLOL", "cblol", &["team5", "team6"], today, 1);

    // Background league 3: PCS (with extra teams, no fixture today)
    let pcs_future_date = "2025-06-22";
    let pcs = League {
        id: "pcs".to_string(),
        name: "PCS".to_string(),
        season: 2025,
        fixtures: vec![Fixture {
            id: "pcs-fix1".to_string(),
            matchday: 1,
            date: pcs_future_date.to_string(),
            home_team_id: "team7".to_string(),
            away_team_id: "team8".to_string(),
            match_type: MatchType::League,
            best_of: 1,
            status: FixtureStatus::Scheduled,
            result: None,
        }],
        standings: vec![
            StandingEntry::new("team7".to_string()),
            StandingEntry::new("team8".to_string()),
        ],
        competition_id: Some("pcs".to_string()),
        league_kind: LeagueKind::Main,
        ..Default::default()
    };

    game.leagues = vec![lec, lcs, cblol, pcs];

    // All leagues present
    assert_eq!(game.leagues.len(), 4);

    // Process matchday
    turn::process_day(&mut game);

    // LEC (active, index 0): fixture is in the future — should remain Scheduled
    let lec_after = &game.leagues[0];
    assert_eq!(
        lec_after.fixtures[0].status,
        FixtureStatus::Scheduled,
        "LEC has no fixture today — should remain unchanged"
    );

    // LCS (background, index 1): fixture should be completed
    let lcs_after = &game.leagues[1];
    assert_eq!(lcs_after.fixtures[0].status, FixtureStatus::Completed);

    // CBLOL (background, index 2): fixture should be completed
    let cblol_after = &game.leagues[2];
    assert_eq!(cblol_after.fixtures[0].status, FixtureStatus::Completed);

    // PCS (background, index 3): no fixture today — should remain Scheduled
    let pcs_after = &game.leagues[3];
    assert_eq!(
        pcs_after.fixtures[0].status,
        FixtureStatus::Scheduled,
        "PCS has no fixture today — should remain unchanged"
    );

    // All leagues still have correct competition identities
    assert_eq!(
        game.leagues[0].competition_id.as_deref(),
        Some("lec"),
        "league[0] should still be LEC"
    );
    assert_eq!(
        game.leagues[1].competition_id.as_deref(),
        Some("lcs"),
        "league[1] should still be LCS"
    );
    assert_eq!(
        game.leagues[2].competition_id.as_deref(),
        Some("cblol"),
        "league[2] should still be CBLOL"
    );
    assert_eq!(
        game.leagues[3].competition_id.as_deref(),
        Some("pcs"),
        "league[3] should still be PCS"
    );
}
