use chrono::{TimeZone, Utc};
use olm_core::clock::GameClock;
use olm_core::domain::league::MatchType;
use olm_core::domain::manager::Manager;
use olm_core::domain::player::{LolRole, Player, PlayerAttributes};
use olm_core::domain::stats::{
    MatchOutcome, PlayerMatchStatsRecord, StatsState, TeamMatchStatsRecord, TeamSide,
};
use olm_core::domain::team::Team;
use olm_core::game::Game;
use olm_core::state::StateManager;

use super::player::{get_player_match_history_internal, get_player_stats_overview_internal};
use super::team::{get_team_match_history_internal, get_team_stats_overview_internal};

fn default_attrs() -> PlayerAttributes {
    PlayerAttributes {
        mechanics: 60,
        laning: 60,
        teamfighting: 60,
        macro_play: 60,
        consistency: 60,
        shotcalling: 60,
        champion_pool: 60,
        discipline: 60,
        mental_resilience: 60,
    }
}

fn make_player(id: &str, team_id: &str, natural_position: LolRole) -> Player {
    let mut player = Player::new(
        id.to_string(),
        id.to_string(),
        id.to_string(),
        "2000-01-01".to_string(),
        "England".to_string(),
        natural_position.clone(),
        default_attrs(),
    );
    player.team_id = Some(team_id.to_string());
    player.natural_position = natural_position.into();
    player
}

fn make_game(players: Vec<Player>) -> Game {
    let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap());
    let mut manager = Manager::new(
        "mgr-1".to_string(),
        "Alex".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("team-1".to_string());

    let mut team = Team::new(
        "team-1".to_string(),
        "Alpha FC".to_string(),
        "ALP".to_string(),
        "England".to_string(),
        "Alpha City".to_string(),
        "Alpha Ground".to_string(),
        20_000,
    );
    team.active_lineup_ids = players.iter().map(|player| player.id.clone()).collect();

    let opponent = Team::new(
        "team-2".to_string(),
        "Bravo FC".to_string(),
        "BRV".to_string(),
        "England".to_string(),
        "Bravo City".to_string(),
        "Bravo Ground".to_string(),
        18_000,
    );

    Game::new(
        clock,
        manager,
        vec![team, opponent],
        players,
        vec![],
        vec![],
    )
}

fn player_record(
    fixture_id: &str,
    player_id: &str,
    side: TeamSide,
    result: MatchOutcome,
    kills: u16,
    deaths: u16,
    assists: u16,
    creep_score: u16,
) -> PlayerMatchStatsRecord {
    PlayerMatchStatsRecord {
        fixture_id: fixture_id.to_string(),
        season: 2025,
        matchday: 1,
        date: fixture_id.to_string(),
        match_type: MatchType::League,
        player_id: player_id.to_string(),
        team_id: "team-1".to_string(),
        opponent_team_id: "team-2".to_string(),
        side,
        result,
        role: LolRole::Mid,
        champion: None,
        duration_seconds: 1800,
        kills,
        deaths,
        assists,
        creep_score,
        gold_earned: 60_000,
        damage_dealt: 95_000,
        vision_score: 30,
        wards_placed: 12,
        bans_json: String::new(),
    }
}

fn team_record(
    fixture_id: &str,
    side: TeamSide,
    result: MatchOutcome,
    kills: u16,
    deaths: u16,
) -> TeamMatchStatsRecord {
    TeamMatchStatsRecord {
        fixture_id: fixture_id.to_string(),
        season: 2025,
        matchday: 1,
        date: fixture_id.to_string(),
        match_type: MatchType::League,
        team_id: "team-1".to_string(),
        opponent_team_id: "team-2".to_string(),
        side,
        result,
        duration_seconds: 2100,
        kills,
        deaths,
        gold_earned: 60_000,
        damage_dealt: 95_000,
        objectives: 7,
    }
}

#[test]
fn get_player_match_history_returns_lol_first_fields() {
    let state = StateManager::new();
    state.set_game(make_game(vec![make_player(
        "player-1",
        "team-1",
        LolRole::Adc,
    )]));
    state.set_stats_state(StatsState {
        player_matches: vec![
            player_record(
                "2025-06-10",
                "player-1",
                TeamSide::Blue,
                MatchOutcome::Win,
                3,
                1,
                8,
                210,
            ),
            player_record(
                "2025-06-17",
                "player-1",
                TeamSide::Red,
                MatchOutcome::Loss,
                1,
                4,
                2,
                190,
            ),
        ],
        team_matches: vec![],
    });

    let history = get_player_match_history_internal(&state, "player-1", Some(1)).unwrap();

    assert_eq!(history.len(), 1);
    assert_eq!(history[0].fixture_id, "2025-06-17");
    assert_eq!(history[0].side, "Red");
    assert_eq!(history[0].result, "Loss");
    assert_eq!(history[0].role, "Mid");
    assert_eq!(history[0].kills, 1);
    assert_eq!(history[0].cs, 190);
    assert_eq!(history[0].game_duration_seconds, 1800);
}

#[test]
fn get_player_stats_overview_aggregates_lol_metrics_by_position() {
    let state = StateManager::new();
    state.set_game(make_game(vec![
        make_player("player-1", "team-1", LolRole::Adc),
        make_player("player-2", "team-1", LolRole::Adc),
        make_player("player-3", "team-1", LolRole::Adc),
    ]));
    state.set_stats_state(StatsState {
        player_matches: vec![
            player_record(
                "2025-08-01-a",
                "player-1",
                TeamSide::Blue,
                MatchOutcome::Win,
                6,
                2,
                10,
                250,
            ),
            player_record(
                "2025-08-08-a",
                "player-1",
                TeamSide::Red,
                MatchOutcome::Win,
                5,
                1,
                9,
                240,
            ),
            player_record(
                "2025-08-15-a",
                "player-1",
                TeamSide::Blue,
                MatchOutcome::Loss,
                4,
                3,
                7,
                230,
            ),
            player_record(
                "2025-08-01-b",
                "player-2",
                TeamSide::Blue,
                MatchOutcome::Win,
                2,
                4,
                5,
                180,
            ),
            player_record(
                "2025-08-08-b",
                "player-2",
                TeamSide::Red,
                MatchOutcome::Loss,
                1,
                5,
                4,
                170,
            ),
            player_record(
                "2025-08-15-b",
                "player-2",
                TeamSide::Blue,
                MatchOutcome::Win,
                3,
                4,
                6,
                175,
            ),
            player_record(
                "2025-08-01-c",
                "player-3",
                TeamSide::Blue,
                MatchOutcome::Win,
                1,
                3,
                7,
                160,
            ),
            player_record(
                "2025-08-08-c",
                "player-3",
                TeamSide::Red,
                MatchOutcome::Loss,
                2,
                4,
                5,
                165,
            ),
            player_record(
                "2025-08-15-c",
                "player-3",
                TeamSide::Blue,
                MatchOutcome::Win,
                2,
                2,
                6,
                170,
            ),
        ],
        team_matches: vec![],
    });

    let overview = get_player_stats_overview_internal(&state, "player-1").unwrap();

    assert!(overview.percentile_eligible);
    assert_eq!(overview.matches_played, 3);
    assert_eq!(overview.metrics.kills.total, 15);
    assert_eq!(overview.metrics.kills.per_match, Some(5.0));
    assert_eq!(overview.metrics.kills.percentile, Some(100));
    assert_eq!(overview.metrics.vision_score.total, 90);
    assert_eq!(overview.metrics.wards_placed.total, 36);
}

#[test]
fn get_team_stats_overview_aggregates_lol_team_metrics() {
    let state = StateManager::new();
    state.set_game(make_game(vec![make_player(
        "player-1",
        "team-1",
        LolRole::Adc,
    )]));
    state.set_stats_state(StatsState {
        player_matches: vec![],
        team_matches: vec![
            team_record("2025-08-01", TeamSide::Blue, MatchOutcome::Win, 18, 9),
            team_record("2025-08-08", TeamSide::Red, MatchOutcome::Loss, 11, 17),
        ],
    });

    let overview = get_team_stats_overview_internal(&state, "team-1")
        .unwrap()
        .expect("expected team overview");

    assert_eq!(overview.matches_played, 2);
    assert_eq!(overview.wins, 1);
    assert_eq!(overview.losses, 1);
    assert_eq!(overview.metrics.kills.total, 29);
    assert_eq!(overview.metrics.kills.per_match, Some(14.5));
    assert_eq!(overview.metrics.average_game_duration_seconds.total, 4200);
}

#[test]
fn get_team_match_history_returns_lol_first_fields() {
    let state = StateManager::new();
    state.set_game(make_game(vec![make_player(
        "player-1",
        "team-1",
        LolRole::Adc,
    )]));
    state.set_stats_state(StatsState {
        player_matches: vec![],
        team_matches: vec![
            team_record("2025-08-01", TeamSide::Blue, MatchOutcome::Win, 18, 9),
            team_record("2025-08-08", TeamSide::Red, MatchOutcome::Loss, 11, 17),
        ],
    });

    let history = get_team_match_history_internal(&state, "team-1", Some(1)).unwrap();

    assert_eq!(history.len(), 1);
    assert_eq!(history[0].fixture_id, "2025-08-08");
    assert_eq!(history[0].side, "Red");
    assert_eq!(history[0].result, "Loss");
    assert_eq!(history[0].kills, 11);
    assert_eq!(history[0].objectives, 7);
}
