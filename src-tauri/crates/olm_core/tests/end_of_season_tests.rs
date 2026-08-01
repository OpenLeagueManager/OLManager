use chrono::{TimeZone, Utc};
use olm_core::clock::GameClock;
use olm_core::domain::league::{
    Fixture, FixtureStatus, League, LeagueKind, MatchResult, MatchType, StandingEntry,
};
use olm_core::domain::manager::Manager;
use olm_core::domain::message::MessageCategory;
use olm_core::domain::player::{Player, PlayerAttributes, PlayerSeasonStats};
use olm_core::domain::stats::LolRole;
use olm_core::domain::team::{FinancialTransactionKind, Team, TeamKind};
use olm_core::end_of_season::{
    is_season_complete, process_background_seasons, process_end_of_season, process_end_of_split,
};
use olm_core::game::{BoardObjective, Game, ObjectiveType};
use olm_core::generator::definitions::{
    CompetitionManifest, ScheduleConfig, SeasonStart, SplitConfig,
};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

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

fn make_player(id: &str, name: &str, team_id: &str, pos: LolRole) -> Player {
    let attrs = PlayerAttributes {
        mental_resilience: 65,
        champion_pool: 65,
        laning: 65,
        mechanics: 65,
        macro_play: 65,
        consistency: 65,
        discipline: 65,
        teamfighting: 65,
        shotcalling: 50,
    };
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

fn make_completed_fixture(id: &str, home: &str, away: &str, hg: u8, ag: u8) -> Fixture {
    Fixture {
        id: id.to_string(),
        matchday: 1,
        date: "2025-06-01".to_string(),
        home_team_id: home.to_string(),
        away_team_id: away.to_string(),
        match_type: MatchType::League,
        best_of: 1,
        status: FixtureStatus::Completed,
        result: Some(MatchResult {
            home_wins: hg,
            away_wins: ag,
            ended_by: Default::default(),
            game_duration_seconds: 90 * 60,
            report: None,
        }),
    }
}

fn make_standing(
    team_id: &str,
    won: u32,
    lost: u32,
    maps_won: u32,
    maps_lost: u32,
) -> StandingEntry {
    StandingEntry {
        team_id: team_id.to_string(),
        played: won + lost,
        won,
        lost,
        maps_won,
        maps_lost,
        points: won * 3,
    }
}

/// Build a game with a completed season (2 teams, all fixtures done).
fn make_completed_season_game() -> Game {
    let date = Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap();
    let clock = GameClock::new(date);
    let mut manager = Manager::new(
        "mgr1".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("team1".to_string());
    manager.satisfaction = 60;

    let team1 = make_team("team1", "Test FC");
    let team2 = make_team("team2", "Rival FC");

    let mut p1 = make_player("p1", "Star", "team1", LolRole::Adc);
    p1.stats = PlayerSeasonStats {
        appearances: 30,
        kills: 20,
        assists: 10,
        avg_rating: 7.5,
        minutes_played: 2700,
        ..PlayerSeasonStats::default()
    };

    let mut p2 = make_player("p2", "Rival", "team2", LolRole::Adc);
    p2.stats = PlayerSeasonStats {
        appearances: 28,
        kills: 15,
        assists: 8,
        avg_rating: 7.0,
        minutes_played: 2500,
        ..PlayerSeasonStats::default()
    };

    let fixtures = vec![
        make_completed_fixture("f1", "team1", "team2", 2, 1),
        make_completed_fixture("f2", "team2", "team1", 0, 1),
    ];

    // team1 won both: 6 pts, team2 lost both: 0 pts
    let standings = vec![
        make_standing("team1", 2, 0, 3, 1),
        make_standing("team2", 0, 2, 1, 3),
    ];

    let league = League {
        id: "league1".to_string(),
        name: "Test League".to_string(),
        logo: None,
        season: 1,
        competition_id: None,
        league_kind: LeagueKind::Main,
        split_index: 0,
        fixtures,
        standings,
        ..Default::default()
    };

    let mut game = Game::new(
        clock,
        manager,
        vec![team1, team2],
        vec![p1, p2],
        vec![],
        vec![],
    );
    game.leagues = vec![league];
    game
}

fn make_completed_background_league(id: &str, competition_id: Option<&str>, season: u32) -> League {
    League {
        id: id.to_string(),
        name: "Background League".to_string(),
        logo: None,
        season,
        competition_id: competition_id.map(str::to_string),
        league_kind: LeagueKind::Main,
        split_index: 0,
        fixtures: vec![
            make_completed_fixture(&format!("{}-f1", id), "team1", "team2", 2, 1),
            make_completed_fixture(&format!("{}-f2", id), "team2", "team1", 0, 1),
        ],
        standings: vec![
            make_standing("team1", 2, 0, 3, 1),
            make_standing("team2", 0, 2, 1, 3),
        ],
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// is_season_complete
// ---------------------------------------------------------------------------

fn make_test_manifest() -> CompetitionManifest {
    CompetitionManifest {
        id: "test-league".to_string(),
        name: "Test League".to_string(),
        full_name: None,
        region: "EU".to_string(),
        country: None,
        tier: None,
        logo: None,
        schedule: ScheduleConfig {
            format: "single_round_robin".to_string(),
            team_count: 2,
            splits: vec![SplitConfig {
                name: "Spring".to_string(),
                season_start: SeasonStart { month: 8, day: 1 },
                superweek_offsets: vec![],
                best_of: 1,
                playoffs: None,
            }],
            preseason_friendlies: 0,
        },
        teams_file: "teams.json".to_string(),
        players_file: "players.json".to_string(),
        staff_file: None,
        championships_file: None,
        erls: vec![],
        reputation: None,
        nearby_country_codes: vec![],
        legacy: false,
        active: true,
    }
}

#[test]
fn season_complete_when_all_fixtures_completed() {
    let game = make_completed_season_game();
    assert!(is_season_complete(&game));
}

#[test]
fn season_not_complete_with_scheduled_fixtures() {
    // One of the two league fixtures is still Scheduled.
    // has_full_schedule returns true (2 == 2), but .all(Completed) returns false.
    let mut game = make_completed_season_game();
    if let Some(league) = game.leagues.first_mut() {
        league.fixtures[1].status = FixtureStatus::Scheduled;
        league.fixtures[1].result = None;
    }
    assert!(!is_season_complete(&game));
}

#[test]
fn season_not_complete_with_no_league() {
    let mut game = make_completed_season_game();
    game.leagues.clear();
    assert!(!is_season_complete(&game));
}

#[test]
fn season_not_complete_with_empty_fixtures() {
    let mut game = make_completed_season_game();
    if let Some(league) = game.leagues.first_mut() {
        league.fixtures.clear();
    }
    assert!(!is_season_complete(&game));
}

#[test]
fn season_not_complete_with_truncated_completed_fixture_list() {
    let mut game = make_completed_season_game();
    game.teams.push(make_team("team3", "Third FC"));
    game.teams.push(make_team("team4", "Fourth FC"));

    if let Some(league) = game.leagues.first_mut() {
        league.standings = vec![
            make_standing("team1", 1, 0, 2, 0),
            make_standing("team4", 1, 0, 1, 0),
            make_standing("team3", 0, 1, 0, 1),
            make_standing("team2", 0, 1, 0, 2),
        ];
        league.fixtures = vec![
            Fixture {
                id: "f1".to_string(),
                matchday: 1,
                date: "2026-08-01".to_string(),
                home_team_id: "team1".to_string(),
                away_team_id: "team2".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Completed,
                result: Some(MatchResult {
                    home_wins: 2,
                    away_wins: 0,
                    ended_by: Default::default(),
                    game_duration_seconds: 90 * 60,
                    report: None,
                }),
            },
            Fixture {
                id: "f2".to_string(),
                matchday: 1,
                date: "2026-08-01".to_string(),
                home_team_id: "team3".to_string(),
                away_team_id: "team4".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Completed,
                result: Some(MatchResult {
                    home_wins: 0,
                    away_wins: 1,
                    ended_by: Default::default(),
                    game_duration_seconds: 90 * 60,
                    report: None,
                }),
            },
        ];
    }

    assert!(
        !is_season_complete(&game),
        "A truncated fixture list must not count as a completed season"
    );
}

// ---------------------------------------------------------------------------
// process_background_seasons — stale manifest compatibility
// ---------------------------------------------------------------------------

#[test]
fn background_league_with_missing_manifest_records_history_without_regeneration() {
    let mut game = make_completed_season_game();
    game.leagues.push(make_completed_background_league(
        "stale-bg",
        Some("removed-competition"),
        2026,
    ));

    let manifests = HashMap::new();
    process_background_seasons(&mut game, &manifests);
    process_background_seasons(&mut game, &manifests);

    let team1 = game.teams.iter().find(|team| team.id == "team1").unwrap();
    assert_eq!(team1.history.len(), 1);
    assert_eq!(team1.history[0].season, 2026);
    assert_eq!(team1.history[0].league_position, 1);

    let background_league = game
        .leagues
        .iter()
        .find(|league| league.id == "stale-bg")
        .expect("background league should remain present");
    assert_eq!(background_league.season, 2026);
    assert_eq!(
        background_league.competition_id.as_deref(),
        Some("removed-competition")
    );
    assert!(
        background_league
            .fixtures
            .iter()
            .all(|fixture| { fixture.status == FixtureStatus::Completed })
    );
}

// ---------------------------------------------------------------------------
// process_end_of_season — summary
// ---------------------------------------------------------------------------

#[test]
fn summary_has_correct_champion() {
    let mut game = make_completed_season_game();
    let summary = process_end_of_season(&mut game);
    assert_eq!(summary.champion_id, "team1");
    assert_eq!(summary.champion_name, "Test FC");
    assert_eq!(summary.season, 1);
}

#[test]
fn summary_has_correct_user_position() {
    let mut game = make_completed_season_game();
    let summary = process_end_of_season(&mut game);
    // team1 (user) is champion
    assert_eq!(summary.user_position, 1);
    assert_eq!(summary.user_points, 6);
    assert_eq!(summary.user_won, 2);
    assert_eq!(summary.user_lost, 0);
}

#[test]
fn summary_has_correct_goals() {
    let mut game = make_completed_season_game();
    let summary = process_end_of_season(&mut game);
    assert_eq!(summary.user_maps_won, 3);
    assert_eq!(summary.user_maps_lost, 1);
}

#[test]
fn summary_total_teams() {
    let mut game = make_completed_season_game();
    let summary = process_end_of_season(&mut game);
    assert_eq!(summary.total_teams, 2);
}

// ---------------------------------------------------------------------------
// process_end_of_season — history recording
// ---------------------------------------------------------------------------

#[test]
fn team_history_recorded() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    let team1 = game.teams.iter().find(|t| t.id == "team1").unwrap();
    assert_eq!(team1.history.len(), 1);
    let record = &team1.history[0];
    assert_eq!(record.season, 1);
    assert_eq!(record.league_position, 1);
    assert_eq!(record.won, 2);
    assert_eq!(record.lost, 0);

    let team2 = game.teams.iter().find(|t| t.id == "team2").unwrap();
    assert_eq!(team2.history.len(), 1);
    assert_eq!(team2.history[0].league_position, 2);
}

#[test]
fn team_form_cleared() {
    let mut game = make_completed_season_game();
    // Give team1 some form
    game.teams
        .iter_mut()
        .find(|t| t.id == "team1")
        .unwrap()
        .form = vec!["W".to_string(), "W".to_string()];

    process_end_of_season(&mut game);

    let team1 = game.teams.iter().find(|t| t.id == "team1").unwrap();
    assert!(
        team1.form.is_empty(),
        "Form should be cleared after season end"
    );
}

// ---------------------------------------------------------------------------
// process_end_of_season — player career/stats reset
// ---------------------------------------------------------------------------

#[test]
fn player_career_entry_added() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    let p1 = game.players.iter().find(|p| p.id == "p1").unwrap();
    assert_eq!(p1.career.len(), 1);
    let entry = &p1.career[0];
    assert_eq!(entry.season, 1);
    assert_eq!(entry.appearances, 30);
    assert_eq!(entry.kills, 20);
    assert_eq!(entry.assists, 10);
    assert!((entry.avg_rating - 7.5).abs() < f32::EPSILON);
}

#[test]
fn player_stats_reset() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    let p1 = game.players.iter().find(|p| p.id == "p1").unwrap();
    assert_eq!(p1.stats.appearances, 0);
    assert_eq!(p1.stats.kills, 0);
    assert_eq!(p1.stats.assists, 0);
}

#[test]
fn player_with_zero_appearances_no_career_entry() {
    let mut game = make_completed_season_game();
    // Add a player with 0 appearances
    let p3 = make_player("p3", "Bench", "team1", LolRole::Top);
    game.players.push(p3);

    process_end_of_season(&mut game);

    let p3 = game.players.iter().find(|p| p.id == "p3").unwrap();
    assert!(
        p3.career.is_empty(),
        "No career entry for 0-appearance player"
    );
}

// ---------------------------------------------------------------------------
// process_end_of_season — manager career
// ---------------------------------------------------------------------------

#[test]
fn manager_career_stats_updated() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    assert_eq!(game.manager.career_stats.matches_managed, 2);
    assert_eq!(game.manager.career_stats.wins, 2);
    assert_eq!(game.manager.career_stats.losses, 0);
}

#[test]
fn manager_trophy_awarded_for_first_place() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);
    assert_eq!(game.manager.career_stats.trophies, 1);
}

#[test]
fn manager_no_trophy_for_non_first() {
    let mut game = make_completed_season_game();
    // Swap standings so team2 is first
    if let Some(league) = game.leagues.first_mut() {
        league.standings = vec![
            make_standing("team2", 2, 0, 3, 1),
            make_standing("team1", 0, 2, 1, 3),
        ];
    }
    process_end_of_season(&mut game);
    assert_eq!(game.manager.career_stats.trophies, 0);
}

#[test]
fn manager_best_finish_set() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);
    assert_eq!(game.manager.career_stats.best_finish, Some(1));
}

#[test]
fn manager_career_history_entry_created() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    assert_eq!(game.manager.career_history.len(), 1);
    let entry = &game.manager.career_history[0];
    assert_eq!(entry.team_id, "team1");
    assert_eq!(entry.matches, 2);
    assert_eq!(entry.wins, 2);
    assert_eq!(entry.best_league_position, Some(1));
}

#[test]
fn manager_career_history_entry_updated_on_second_season() {
    let mut game = make_completed_season_game();
    // Add pre-existing career history entry
    game.manager
        .career_history
        .push(olm_core::domain::manager::ManagerCareerEntry {
            team_id: "team1".to_string(),
            team_name: "Test FC".to_string(),
            start_date: "2025-08-01".to_string(),
            end_date: None,
            matches: 10,
            wins: 5,
            losses: 2,
            best_league_position: Some(3),
        });

    process_end_of_season(&mut game);

    // Should update existing entry, not create new
    assert_eq!(game.manager.career_history.len(), 1);
    let entry = &game.manager.career_history[0];
    assert_eq!(entry.matches, 12); // 10 + 2
    assert_eq!(entry.wins, 7); // 5 + 2
    assert_eq!(entry.best_league_position, Some(1)); // improved from 3 to 1
}

// ---------------------------------------------------------------------------
// process_end_of_season — next season generation
// ---------------------------------------------------------------------------

#[test]
fn new_league_generated() {
    let mut game = make_completed_season_game();
    let manifest = make_test_manifest();
    game.leagues[0].competition_id = Some(manifest.id.clone());

    process_end_of_season(&mut game);
    process_end_of_split(&mut game, &manifest);

    let league = game.leagues.first().unwrap();
    assert_eq!(league.season, 2, "Should be season 2");
    assert!(
        !league.fixtures.is_empty(),
        "Should have fixtures for new season"
    );
    // All fixtures should be Scheduled
    assert!(
        league
            .fixtures
            .iter()
            .all(|f| f.status == FixtureStatus::Scheduled),
        "All new fixtures should be Scheduled"
    );
}

#[test]
fn board_objectives_cleared() {
    let mut game = make_completed_season_game();
    game.board_objectives.push(BoardObjective {
        id: "obj1".to_string(),
        objective_type: ObjectiveType::LeaguePosition,
        description: "Finish top 2".to_string(),
        target: 2,
        met: true,
    });

    process_end_of_season(&mut game);
    assert!(
        game.board_objectives.is_empty(),
        "Objectives should be cleared"
    );
}

#[test]
fn board_objectives_recalculated_before_satisfaction_adjustment() {
    let mut game = make_completed_season_game();
    game.board_objectives = vec![
        BoardObjective {
            id: "obj_position".to_string(),
            objective_type: ObjectiveType::LeaguePosition,
            description: "Finish top 1".to_string(),
            target: 1,
            met: false,
        },
        BoardObjective {
            id: "obj_wins".to_string(),
            objective_type: ObjectiveType::Wins,
            description: "Win 2 series".to_string(),
            target: 2,
            met: false,
        },
        BoardObjective {
            id: "obj_maps".to_string(),
            objective_type: ObjectiveType::GoalsScored,
            description: "Win 3 maps".to_string(),
            target: 3,
            met: false,
        },
    ];

    process_end_of_season(&mut game);

    assert_eq!(
        game.manager.satisfaction, 75,
        "All three stale objectives should be recalculated as met before applying +15"
    );
    assert!(game.board_objectives.is_empty());
}

#[test]
fn board_objective_review_message_reports_result_and_satisfaction_delta() {
    let mut game = make_completed_season_game();
    game.board_objectives = vec![
        BoardObjective {
            id: "obj_position".to_string(),
            objective_type: ObjectiveType::LeaguePosition,
            description: "Finish top 1".to_string(),
            target: 1,
            met: false,
        },
        BoardObjective {
            id: "obj_wins".to_string(),
            objective_type: ObjectiveType::Wins,
            description: "Win 3 series".to_string(),
            target: 3,
            met: false,
        },
        BoardObjective {
            id: "obj_maps".to_string(),
            objective_type: ObjectiveType::GoalsScored,
            description: "Win 10 maps".to_string(),
            target: 10,
            met: false,
        },
    ];

    process_end_of_season(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.id == "board_objective_review_1")
        .expect("Board objective review message should be visible in the inbox");
    assert_eq!(
        msg.subject_key.as_deref(),
        Some("be.msg.boardObjectiveReview.subject")
    );
    assert_eq!(
        msg.body_key.as_deref(),
        Some("be.msg.boardObjectiveReview.body")
    );
    assert_eq!(
        msg.sender_key.as_deref(),
        Some("be.sender.boardOfDirectors")
    );
    assert_eq!(msg.sender_role_key.as_deref(), Some("be.role.chairman"));
    assert_eq!(msg.i18n_params.get("season"), Some(&"1".to_string()));
    assert_eq!(msg.i18n_params.get("metCount"), Some(&"1".to_string()));
    assert_eq!(msg.i18n_params.get("total"), Some(&"3".to_string()));
    assert_eq!(
        msg.i18n_params.get("satisfactionDelta"),
        Some(&"-5".to_string())
    );
    assert!(msg.subject.contains("Board Objective Review"));
    assert!(msg.body.contains("1/3 objectives"), "got: {}", msg.body);
    assert!(
        msg.body.contains("Manager satisfaction impact: -5"),
        "got: {}",
        msg.body
    );
    assert!(msg.body.contains("series wins"), "got: {}", msg.body);
    assert!(msg.body.contains("map wins"), "got: {}", msg.body);
    assert!(!msg.body.contains("football"), "got: {}", msg.body);
}

#[test]
fn news_cleared() {
    let mut game = make_completed_season_game();
    game.news.push(olm_core::domain::news::NewsArticle::new(
        "n1".to_string(),
        "Old news".to_string(),
        "...".to_string(),
        "Source".to_string(),
        "2025-01-01".to_string(),
        olm_core::domain::news::NewsCategory::MatchReport,
    ));

    process_end_of_season(&mut game);
    assert_eq!(
        game.news.len(),
        1,
        "Old news should be replaced by the new season preview"
    );
    assert_ne!(game.news[0].id, "n1");
    assert_eq!(
        game.news[0].category,
        olm_core::domain::news::NewsCategory::SeasonPreview
    );
}

// ---------------------------------------------------------------------------
// process_end_of_season — messages
// ---------------------------------------------------------------------------

#[test]
fn champion_receives_prize_money_and_ledger_entry() {
    let mut game = make_completed_season_game();
    let initial_finance = game
        .teams
        .iter()
        .find(|team| team.id == "team1")
        .unwrap()
        .finance;

    process_end_of_season(&mut game);

    let team1 = game.teams.iter().find(|team| team.id == "team1").unwrap();
    assert_eq!(team1.finance, initial_finance + 800_000);
    assert_eq!(team1.season_income, 800_000);
    assert_eq!(
        team1.wage_budget,
        ((team1.finance as f64) * 0.06).round() as i64
    );
    assert_eq!(
        team1.transfer_budget,
        ((team1.finance as f64) * 0.22).round() as i64
    );
    assert_eq!(team1.financial_ledger.len(), 1);
    assert_eq!(
        team1.financial_ledger[0].kind,
        FinancialTransactionKind::PrizeMoney
    );
    assert_eq!(team1.financial_ledger[0].amount, 800_000);
    assert_eq!(team1.financial_ledger[0].date, "2025-06-01");
    assert_eq!(team1.financial_ledger[0].source, "prize");
    assert_eq!(
        team1.financial_ledger[0].source_id.as_deref(),
        Some("season-1-position-1")
    );
}

#[test]
fn top_half_finish_receives_expected_prize_money() {
    let mut game = make_completed_season_game();
    game.teams.push(make_team("team3", "Third FC"));
    game.teams.push(make_team("team4", "Fourth FC"));

    if let Some(league) = game.leagues.first_mut() {
        league.standings = vec![
            make_standing("team2", 6, 0, 12, 2),
            make_standing("team1", 4, 2, 8, 5),
            make_standing("team3", 2, 4, 4, 8),
            make_standing("team4", 0, 6, 2, 12),
        ];
    }

    let initial_finance = game
        .teams
        .iter()
        .find(|team| team.id == "team1")
        .unwrap()
        .finance;

    process_end_of_season(&mut game);

    let team1 = game.teams.iter().find(|team| team.id == "team1").unwrap();
    assert_eq!(team1.finance, initial_finance + 500_000);
}

#[test]
fn lower_table_finish_receives_expected_prize_money() {
    let mut game = make_completed_season_game();

    for i in 3..=10 {
        let team_id = format!("team{}", i);
        game.teams
            .push(make_team(&team_id, &format!("Team{} FC", i)));
    }

    if let Some(league) = game.leagues.first_mut() {
        let mut standings = Vec::new();

        for i in 2..=10 {
            standings.push(make_standing(&format!("team{}", i), 10, 6, 20, 15));
        }

        standings.push(make_standing("team1", 0, 18, 2, 40));
        league.standings = standings;
    }

    let initial_finance = game
        .teams
        .iter()
        .find(|team| team.id == "team1")
        .unwrap()
        .finance;

    process_end_of_season(&mut game);

    let team1 = game.teams.iter().find(|team| team.id == "team1").unwrap();
    assert_eq!(team1.finance, initial_finance + 50_000);
}

#[test]
fn prize_money_message_sent_once_per_season() {
    let mut game = make_completed_season_game();
    game.messages
        .push(olm_core::domain::message::InboxMessage::new(
            "season_payout_1".to_string(),
            "Already exists".to_string(),
            "...".to_string(),
            "Board".to_string(),
            "2026-05-20".to_string(),
        ));

    process_end_of_season(&mut game);

    let payout_messages = game
        .messages
        .iter()
        .filter(|message| message.id == "season_payout_1")
        .count();

    assert_eq!(payout_messages, 1);
}

#[test]
fn season_end_sends_scoped_prize_and_board_health_finance_mail_once() {
    let mut game = make_completed_season_game();
    game.teams[0].finance = 3_500_000;

    process_end_of_season(&mut game);
    process_end_of_season(&mut game);

    assert_eq!(
        game.messages
            .iter()
            .filter(|message| message.id == "finance:prize:team1:1:1:prizePayout")
            .count(),
        1
    );
    assert_eq!(
        game.messages
            .iter()
            .filter(|message| message.id
                == "finance:board-health:team1:2025-06-01:boardFinancialHealth")
            .count(),
        1
    );

    let prize = game
        .messages
        .iter()
        .find(|message| message.id == "finance:prize:team1:1:1:prizePayout")
        .expect("prize finance mail should exist");
    assert_eq!(prize.category, MessageCategory::Finance);
    assert_eq!(
        prize.subject_key.as_deref(),
        Some("be.msg.finance.prizePayout.subject")
    );

    let board_health = game
        .messages
        .iter()
        .find(|message| message.id == "finance:board-health:team1:2025-06-01:boardFinancialHealth")
        .expect("board health finance mail should exist");
    assert_eq!(board_health.category, MessageCategory::Finance);
    assert_eq!(
        board_health.subject_key.as_deref(),
        Some("be.msg.finance.boardFinancialHealth.subject")
    );
}

#[test]
fn season_end_message_sent() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    let msg = game.messages.iter().find(|m| m.id == "season_end_1");
    assert!(msg.is_some(), "Should send season end message");
    let msg = msg.unwrap();
    assert!(msg.subject.contains("Season 1"));
}

#[test]
fn new_season_schedule_message_sent() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    let msg = game.messages.iter().find(|m| m.id == "new_season_2");
    assert!(msg.is_some(), "Should send new season message");
    let msg = msg.unwrap();
    assert!(msg.subject.contains("Season 2"));
}

#[test]
fn messages_not_duplicated() {
    let mut game = make_completed_season_game();
    // Pre-add the messages
    game.messages
        .push(olm_core::domain::message::InboxMessage::new(
            "season_end_1".to_string(),
            "Already exists".to_string(),
            "...".to_string(),
            "Board".to_string(),
            "2026-05-20".to_string(),
        ));
    game.messages
        .push(olm_core::domain::message::InboxMessage::new(
            "new_season_2".to_string(),
            "Already exists".to_string(),
            "...".to_string(),
            "League".to_string(),
            "2026-05-20".to_string(),
        ));

    process_end_of_season(&mut game);

    let season_end_count = game
        .messages
        .iter()
        .filter(|m| m.id == "season_end_1")
        .count();
    let new_season_count = game
        .messages
        .iter()
        .filter(|m| m.id == "new_season_2")
        .count();
    assert_eq!(
        season_end_count, 1,
        "Should not duplicate season_end message"
    );
    assert_eq!(
        new_season_count, 1,
        "Should not duplicate new_season message"
    );
}

// ---------------------------------------------------------------------------
// process_end_of_season — board message variations
// ---------------------------------------------------------------------------

#[test]
fn champion_gets_congratulatory_message() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.id == "season_end_1")
        .unwrap();
    assert!(
        msg.body.contains("champions") || msg.body.contains("Congratulations"),
        "Champion should get congratulatory message, got: {}",
        msg.body
    );
}

#[test]
fn mid_table_gets_appropriate_message() {
    let mut game = make_completed_season_game();
    // Make a 4-team league where team1 finishes 3rd (mid-table for total_teams=4)
    let team3 = make_team("team3", "Third FC");
    let team4 = make_team("team4", "Fourth FC");
    game.teams.push(team3);
    game.teams.push(team4);

    if let Some(league) = game.leagues.first_mut() {
        league.standings = vec![
            make_standing("team2", 6, 0, 12, 2),
            make_standing("team3", 4, 2, 8, 5),
            make_standing("team1", 2, 4, 4, 8), // user team 3rd of 4
            make_standing("team4", 0, 6, 2, 12),
        ];
    }

    let summary = process_end_of_season(&mut game);
    // 3rd out of 4 → user_position=3, total_teams=4, 3 <= 4/2=2 is false, so it's "below mid"
    // Actually 3 <= 4/2=2 → false → goes to else branch (disappointing)
    assert_eq!(summary.user_position, 3);
}

#[test]
fn bottom_half_gets_concerned_message() {
    let mut game = make_completed_season_game();
    // Add enough teams so that finishing last (10th of 10) triggers the disappointed branch
    // (user_position > 4 AND user_position > total_teams / 2)
    for i in 3..=10 {
        let tid = format!("team{}", i);
        game.teams.push(make_team(&tid, &format!("Team{} FC", i)));
    }
    if let Some(league) = game.leagues.first_mut() {
        let mut standings = Vec::new();
        for i in 2..=10 {
            standings.push(make_standing(&format!("team{}", i), 10, 6, 20, 15));
        }
        // team1 (user) finishes dead last
        standings.push(make_standing("team1", 0, 18, 2, 40));
        league.standings = standings;
    }

    process_end_of_season(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.id == "season_end_1")
        .unwrap();
    assert!(
        msg.body.contains("disappointing") || msg.body.contains("concerned"),
        "Bottom team should get concerned message, got: {}",
        msg.body
    );
}

#[test]
fn next_season_generation_ignores_academy_team_ids() {
    let mut game = make_completed_season_game();

    for i in 3..=10 {
        let tid = format!("team{}", i);
        game.teams.push(make_team(&tid, &format!("Team{} FC", i)));
    }

    let mut academy = make_team("academy-1", "Academy One");
    academy.team_kind = TeamKind::Academy;
    academy.parent_team_id = Some("team1".to_string());
    game.teams.push(academy);

    if let Some(league) = game.leagues.first_mut() {
        league.standings = vec![
            make_standing("team1", 14, 2, 36, 18),
            make_standing("team2", 13, 3, 34, 19),
            make_standing("team3", 11, 4, 30, 22),
            make_standing("team4", 10, 4, 27, 21),
            make_standing("team5", 9, 5, 24, 23),
            make_standing("team6", 8, 7, 22, 25),
            make_standing("team7", 6, 7, 20, 26),
            make_standing("team8", 5, 8, 19, 28),
            make_standing("team9", 4, 10, 16, 31),
            make_standing("team10", 2, 12, 12, 36),
        ];
    }

    process_end_of_season(&mut game);

    let next_league = game.leagues.first().expect("next league should exist");
    assert_eq!(next_league.standings.len(), 10);
    assert!(
        !next_league
            .standings
            .iter()
            .any(|entry| entry.team_id == "academy-1")
    );
}

// ---------------------------------------------------------------------------
// process_end_of_season — no league edge case
// ---------------------------------------------------------------------------

#[test]
fn no_league_returns_default_summary() {
    let date = Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap();
    let clock = GameClock::new(date);
    let mut manager = Manager::new(
        "mgr1".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("team1".to_string());

    let mut game = Game::new(clock, manager, vec![], vec![], vec![], vec![]);
    // No league set
    let summary = process_end_of_season(&mut game);
    assert_eq!(summary.season, 0);
    assert!(summary.league_name.is_empty());
}

// ---------------------------------------------------------------------------
// process_end_of_season — satisfaction adjustment
// ---------------------------------------------------------------------------

#[test]
fn satisfaction_adjusted_after_season() {
    let mut game = make_completed_season_game();
    let initial_sat = game.manager.satisfaction;
    process_end_of_season(&mut game);
    // With no objectives, evaluate_objectives returns 0, so satisfaction unchanged
    assert_eq!(game.manager.satisfaction, initial_sat);
}

// ---------------------------------------------------------------------------
// is_season_complete — season not started guard
// ---------------------------------------------------------------------------

#[test]
fn season_not_complete_when_no_matches_played() {
    // Full schedule exists but all fixtures are still Scheduled (preseason state).
    // is_season_complete must return false — we must not trigger end-of-season
    // processing before the campaign has even begun.
    let mut game = make_completed_season_game();
    if let Some(league) = game.leagues.first_mut() {
        for fixture in &mut league.fixtures {
            fixture.status = FixtureStatus::Scheduled;
            fixture.result = None;
        }
        for standing in &mut league.standings {
            *standing = StandingEntry::new(standing.team_id.clone());
        }
    }
    assert!(
        !is_season_complete(&game),
        "Season with no matches played must not be considered complete"
    );
}

// ---------------------------------------------------------------------------
// process_end_of_season — message dates
// ---------------------------------------------------------------------------

#[test]
fn season_end_messages_dated_on_last_fixture_date() {
    // make_completed_season_game() sets the clock to 2026-05-20 but both league
    // fixtures are dated 2025-06-01 (see make_completed_fixture).
    // End-of-season messages must be dated on the last completed fixture date
    // (2025-06-01), not on the clock date (2026-05-20).
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    let board_msg = game
        .messages
        .iter()
        .find(|m| m.id == "season_end_1")
        .expect("season_end_1 message must be present");
    assert_eq!(
        board_msg.date, "2025-06-01",
        "Board review must be dated on the last fixture date, not the clock date"
    );

    let payout_msg = game
        .messages
        .iter()
        .find(|m| m.id == "season_payout_1")
        .expect("season_payout_1 message must be present");
    assert_eq!(
        payout_msg.date, "2025-06-01",
        "Prize money message must be dated on the last fixture date"
    );

    let schedule_msg = game
        .messages
        .iter()
        .find(|m| m.id == "new_season_2")
        .expect("new_season_2 message must be present");
    assert_eq!(
        schedule_msg.date, "2025-06-01",
        "New season schedule message must be dated on the last fixture date"
    );
}

// ---------------------------------------------------------------------------
// process_end_of_season — i18n on end-of-season messages
// ---------------------------------------------------------------------------

#[test]
fn season_end_board_message_has_i18n_keys() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.id == "season_end_1")
        .expect("season_end_1 message must be present");

    assert_eq!(
        msg.subject_key.as_deref(),
        Some("be.msg.seasonReview.subject"),
        "Board review subject must have i18n key"
    );
    assert!(
        msg.body_key.is_some(),
        "Board review body must have an i18n key"
    );
    assert!(
        msg.body_key
            .as_deref()
            .unwrap_or("")
            .starts_with("be.msg.seasonReview.body."),
        "Board review body key must be under be.msg.seasonReview.body, got: {:?}",
        msg.body_key
    );
    assert!(
        msg.i18n_params.contains_key("season"),
        "Board review i18n params must contain 'season'"
    );
    assert!(
        msg.i18n_params.contains_key("team"),
        "Board review i18n params must contain 'team'"
    );
    assert!(
        msg.i18n_params.contains_key("points"),
        "Board review i18n params must contain 'points'"
    );
}

#[test]
fn season_end_board_message_has_sender_i18n() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.id == "season_end_1")
        .expect("season_end_1 message must be present");

    assert_eq!(
        msg.sender_key.as_deref(),
        Some("be.sender.boardOfDirectors"),
        "Board review sender must have i18n key"
    );
    assert_eq!(
        msg.sender_role_key.as_deref(),
        Some("be.role.chairman"),
        "Board review sender role must have i18n key"
    );
}

#[test]
fn season_end_new_schedule_message_has_i18n_keys() {
    let mut game = make_completed_season_game();
    process_end_of_season(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.id == "new_season_2")
        .expect("new_season_2 message must be present");

    assert_eq!(
        msg.subject_key.as_deref(),
        Some("be.msg.newSeasonSchedule.subject"),
        "New season schedule subject must have i18n key"
    );
    assert_eq!(
        msg.body_key.as_deref(),
        Some("be.msg.newSeasonSchedule.body"),
        "New season schedule body must have i18n key"
    );
    assert_eq!(
        msg.sender_key.as_deref(),
        Some("be.sender.leagueOffice"),
        "New season schedule sender must have i18n key"
    );
    assert_eq!(
        msg.sender_role_key.as_deref(),
        Some("be.role.competitionSecretary"),
        "New season schedule sender role must have i18n key"
    );
    assert!(
        msg.i18n_params.contains_key("season"),
        "New season schedule i18n params must contain 'season'"
    );
}

#[test]
fn season_end_board_message_top_four_uses_correct_body_key() {
    let mut game = make_completed_season_game();
    // Make team1 finish 2nd (top-4 branch)
    if let Some(league) = game.leagues.first_mut() {
        league.standings = vec![
            make_standing("team2", 2, 0, 3, 1),
            make_standing("team1", 0, 2, 1, 3),
        ];
    }
    process_end_of_season(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.id == "season_end_1")
        .unwrap();
    assert_eq!(
        msg.body_key.as_deref(),
        Some("be.msg.seasonReview.body.topFour"),
        "2nd-place finish should use topFour body key"
    );
    assert!(
        msg.i18n_params.contains_key("position"),
        "topFour key must include position param"
    );
    assert!(
        msg.i18n_params.contains_key("suffix"),
        "topFour key must include suffix param"
    );
}

// ---------------------------------------------------------------------------
// was_released filter tests
// ---------------------------------------------------------------------------

#[test]
fn replenish_depleted_rosters_excludes_was_released_players() {
    // Build a game where team1 has only 2 players (needs 3 free agents)
    // and adds: 1 with was_released=true, 2 without. Only non-released should
    // be assigned.
    let date = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
    let clock = GameClock::new(date);

    let manager = Manager::new(
        "mgr-wr".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );

    fn make_free_agent(id: &str, was_released: bool) -> Player {
        let attrs = PlayerAttributes {
            mental_resilience: 60,
            champion_pool: 60,
            laning: 60,
            mechanics: 60,
            macro_play: 60,
            consistency: 60,
            discipline: 60,
            teamfighting: 60,
            shotcalling: 60,
        };
        let mut p = Player::new(
            id.to_string(),
            id.to_string(),
            format!("Full {}", id),
            "2000-01-01".to_string(),
            "GB".to_string(),
            LolRole::Adc,
            attrs,
        );
        p.team_id = None;
        p.contract_end = None;
        p.wage = 0;
        p.morale = 70;
        p.condition = 90;
        p.was_released = was_released;
        p
    }

    // team1 only has 2 players — needs 3 more
    let mut p1 = make_player("p-existing-1", "Existing1", "team1", LolRole::Top);
    p1.contract_end = Some("2027-06-30".to_string());
    p1.wage = 30_000;
    let mut p2 = make_player("p-existing-2", "Existing2", "team1", LolRole::Jungle);
    p2.contract_end = Some("2027-06-30".to_string());
    p2.wage = 30_000;

    // team2 has 5 players — won't need free agents
    let mut team2_players = Vec::new();
    for i in 0..5 {
        let mut p = make_player(
            &format!("p-team2-{}", i),
            &format!("Team2Player{}", i),
            "team2",
            LolRole::Adc,
        );
        p.contract_end = Some("2027-06-30".to_string());
        p.wage = 100_000;
        team2_players.push(p);
    }

    // Free agents: 3 total — 1 released, 2 available
    let mut players = vec![p1, p2];
    players.extend(team2_players);
    players.push(make_free_agent("fa-released", true));
    players.push(make_free_agent("fa-available-1", false));
    let mut fa2 = make_free_agent("fa-available-2", false);
    fa2.natural_position = LolRole::Support;
    players.push(fa2);

    let mut team1 = make_team("team1", "Needy FC");
    team1.finance = 10_000_000;
    let team2 = make_team("team2", "Full FC");

    let mut game = Game::new(clock, manager, vec![team1, team2], players, vec![], vec![]);

    // Add a completed league so process_end_of_season runs fully
    let fixtures = vec![
        make_completed_fixture("f1", "team1", "team2", 2, 1),
        make_completed_fixture("f2", "team2", "team1", 0, 1),
    ];
    let standings = vec![
        make_standing("team1", 2, 0, 3, 1),
        make_standing("team2", 0, 2, 1, 3),
    ];
    game.leagues = vec![League {
        id: "league-wr".to_string(),
        name: "Test League".to_string(),
        logo: None,
        season: 2026,
        competition_id: None,
        league_kind: LeagueKind::Main,
        split_index: 0,
        fixtures,
        standings,
        ..Default::default()
    }];

    process_end_of_season(&mut game);

    // was_released player should NOT be assigned
    let fa_released = game
        .players
        .iter()
        .find(|p| p.id == "fa-released")
        .expect("released free agent should exist");
    assert!(
        fa_released.team_id.is_none(),
        "was_released=true free agent should NOT be assigned"
    );

    // Non-released free agents SHOULD be assigned (needy team needs 3)
    let fa_avail_1 = game
        .players
        .iter()
        .find(|p| p.id == "fa-available-1")
        .expect("available free agent 1 should exist");
    let fa_avail_2 = game
        .players
        .iter()
        .find(|p| p.id == "fa-available-2")
        .expect("available free agent 2 should exist");

    assert!(
        fa_avail_1.team_id.is_some(),
        "was_released=false free agent 1 SHOULD be assigned"
    );
    assert!(
        fa_avail_2.team_id.is_some(),
        "was_released=false free agent 2 SHOULD be assigned"
    );
}
