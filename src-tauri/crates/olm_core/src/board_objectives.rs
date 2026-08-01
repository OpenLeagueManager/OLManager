use crate::domain::league::FixtureStatus;
use crate::domain::message::*;
use crate::domain::player::Player;
use crate::domain::team::Team;
use crate::end_of_season;
use crate::game::{BoardObjective, Game, ObjectiveType};
use crate::player_rating::natural_ovr;
use std::collections::HashMap;

struct ObjectiveTargets {
    expected_pos: u32,
    win_target: u32,
    goals_target: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObjectiveProfile {
    TitleContender,
    PlayoffContender,
    MidTable,
    Survival,
}

fn team_strength_score(team: &Team, players: &[Player]) -> f64 {
    let mut roster_scores = players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(team.id.as_str()))
        .map(natural_ovr)
        .collect::<Vec<_>>();

    if roster_scores.is_empty() {
        return team.reputation as f64;
    }

    roster_scores.sort_by(|a, b| b.total_cmp(a));
    let starter_count = roster_scores.len().min(5);
    roster_scores.iter().take(starter_count).sum::<f64>() / starter_count as f64
}

fn expected_league_rank(user_team_id: &str, teams: &[Team], players: &[Player]) -> u32 {
    let Some(user_team) = teams.iter().find(|team| team.id == user_team_id) else {
        return teams.len().max(1) as u32;
    };

    let user_score = team_strength_score(user_team, players);
    1 + teams
        .iter()
        .filter(|team| team.id != user_team_id)
        .filter(|team| team_strength_score(team, players) > user_score)
        .count() as u32
}

fn objective_profile_for_rank(expected_rank: u32, num_teams: u32) -> ObjectiveProfile {
    if num_teams <= 2 || expected_rank <= ((num_teams + 4) / 5).max(1) {
        ObjectiveProfile::TitleContender
    } else if expected_rank <= num_teams.div_ceil(2) {
        ObjectiveProfile::PlayoffContender
    } else if expected_rank <= (num_teams * 3).div_ceil(4) {
        ObjectiveProfile::MidTable
    } else {
        ObjectiveProfile::Survival
    }
}

fn objective_targets(
    expected_rank: u32,
    num_teams: u32,
    total_series: u32,
    total_winnable_maps: u32,
) -> ObjectiveTargets {
    let profile = objective_profile_for_rank(expected_rank, num_teams);
    let expected_pos = match profile {
        ObjectiveProfile::TitleContender => expected_rank.max(1),
        ObjectiveProfile::PlayoffContender => expected_rank.max(num_teams.div_ceil(2).max(2)),
        ObjectiveProfile::MidTable => expected_rank.max((num_teams * 3).div_ceil(4).max(2)),
        ObjectiveProfile::Survival => num_teams.saturating_sub(1).max(1),
    }
    .min(num_teams.max(1));

    let win_target = match profile {
        ObjectiveProfile::TitleContender => total_series * 60 / 100,
        ObjectiveProfile::PlayoffContender => total_series * 45 / 100,
        ObjectiveProfile::MidTable => total_series * 35 / 100,
        ObjectiveProfile::Survival => total_series * 25 / 100,
    };

    let goals_target = match profile {
        ObjectiveProfile::TitleContender => total_winnable_maps,
        ObjectiveProfile::PlayoffContender => total_winnable_maps * 75 / 100,
        ObjectiveProfile::MidTable => total_winnable_maps * 625 / 1000,
        ObjectiveProfile::Survival => total_winnable_maps * 50 / 100,
    };

    ObjectiveTargets {
        expected_pos,
        win_target: win_target.max(1),
        goals_target: goals_target.max(1),
    }
}

fn fallback_series_count(num_teams: u32) -> u32 {
    if num_teams > 1 {
        (num_teams - 1) * 2
    } else {
        0
    }
}

fn active_league_teams(game: &Game) -> Vec<Team> {
    let Some(league) = game.active_league() else {
        return Vec::new();
    };

    league
        .standings
        .iter()
        .filter_map(|standing| {
            game.teams
                .iter()
                .find(|team| team.id == standing.team_id)
                .cloned()
        })
        .collect()
}

fn scheduled_totals(game: &Game, user_team_id: &str, num_teams: u32) -> (u32, u32) {
    let Some(league) = game.active_league() else {
        let fallback_series = fallback_series_count(num_teams);
        return (fallback_series, fallback_series * 2);
    };

    let user_fixtures = league.fixtures.iter().filter(|fixture| {
        fixture.counts_for_league_standings()
            && (fixture.home_team_id == user_team_id || fixture.away_team_id == user_team_id)
    });

    let mut total_series = 0;
    let mut total_winnable_maps = 0;
    for fixture in user_fixtures {
        total_series += 1;
        total_winnable_maps += u32::from(fixture.best_of.max(1) / 2 + 1);
    }

    if total_series > 0 {
        (total_series, total_winnable_maps.max(total_series))
    } else {
        let fallback_series = fallback_series_count(num_teams);
        (fallback_series, fallback_series * 2)
    }
}

fn board_message_id(season: u32) -> String {
    format!("board_objectives_{}", season)
}

fn build_objectives_message(
    targets: &ObjectiveTargets,
    season: u32,
    today: String,
) -> InboxMessage {
    let mut params = HashMap::new();
    params.insert("season".to_string(), season.to_string());
    params.insert("expectedPos".to_string(), targets.expected_pos.to_string());
    params.insert("winTarget".to_string(), targets.win_target.to_string());
    params.insert("goalsTarget".to_string(), targets.goals_target.to_string());

    InboxMessage::new(
        board_message_id(season),
        format!("Season {} — Board Objectives", season),
        format!(
            "The board has set the following objectives for this split:\n\n1. Finish in the top {}\n2. Win at least {} series\n3. Win at least {} maps\n\nMeeting these targets will improve the board's confidence in your roster management, draft preparation, and stage performance. Failure to meet expectations may result in reduced budgets or further consequences.",
            targets.expected_pos, targets.win_target, targets.goals_target
        ),
        "Board of Directors".to_string(),
        today,
    )
    .with_category(MessageCategory::BoardDirective)
    .with_priority(MessagePriority::High)
    .with_sender_role("Chairman")
    .with_i18n(
        "be.msg.boardObjectives.subject",
        "be.msg.boardObjectives.body",
        params,
    )
    .with_sender_i18n("be.sender.boardOfDirectors", "be.role.chairman")
}

fn satisfaction_delta(met_count: usize, total: usize) -> i8 {
    if met_count == total {
        15
    } else if met_count * 2 > total {
        5
    } else if met_count > 0 {
        -5
    } else {
        -15
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectiveEvaluation {
    pub met_count: usize,
    pub total: usize,
    pub satisfaction_delta: i8,
}

/// Generate board objectives for the current season.
/// Called at season start or when no objectives exist.
pub fn generate_objectives(game: &mut Game) {
    if !game.board_objectives.is_empty() {
        return;
    }

    let user_team_id = match &game.manager.team_id {
        Some(id) => id.clone(),
        None => return,
    };

    let active_teams = active_league_teams(game);
    let team = match active_teams.iter().find(|t| t.id == user_team_id) {
        Some(t) => t,
        None => return,
    };

    let num_teams = active_teams.len() as u32;
    let expected_rank = expected_league_rank(&team.id, &active_teams, &game.players);
    let (total_series, total_winnable_maps) = scheduled_totals(game, &team.id, num_teams);
    let targets = objective_targets(expected_rank, num_teams, total_series, total_winnable_maps);

    game.board_objectives = vec![
        BoardObjective {
            id: "obj_position".to_string(),
            description: "boardObjectives.objective.LeaguePosition".to_string(),
            target: targets.expected_pos,
            objective_type: ObjectiveType::LeaguePosition,
            met: false,
        },
        BoardObjective {
            id: "obj_wins".to_string(),
            description: "boardObjectives.objective.Wins".to_string(),
            target: targets.win_target,
            objective_type: ObjectiveType::Wins,
            met: false,
        },
        BoardObjective {
            id: "obj_goals".to_string(),
            description: "boardObjectives.objective.GoalsScored".to_string(),
            target: targets.goals_target,
            objective_type: ObjectiveType::GoalsScored,
            met: false,
        },
    ];

    // Send inbox message about objectives
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let existing_ids: std::collections::HashSet<String> =
        game.messages.iter().map(|m| m.id.clone()).collect();
    let season = game.active_league().map(|l| l.season).unwrap_or(1);
    let msg_id = board_message_id(season);
    if !existing_ids.contains(&msg_id) {
        let msg = build_objectives_message(&targets, season, today);
        game.messages.push(msg);
    }
}

/// Update objective progress based on current standings. Called daily.
pub fn update_objective_progress(game: &mut Game) {
    let user_team_id = match &game.manager.team_id {
        Some(id) => id.clone(),
        None => return,
    };

    let league = match game.active_league() {
        Some(l) => l,
        None => return,
    };

    let standings = league.sorted_standings();
    let user_pos = standings
        .iter()
        .position(|s| s.team_id == user_team_id)
        .map(|i| (i + 1) as u32)
        .unwrap_or(99);
    let user_standing = standings.iter().find(|s| s.team_id == user_team_id);

    let league_complete = end_of_season::is_league_complete(league);

    // Count user goals from completed fixtures
    let user_goals: u32 = league
        .fixtures
        .iter()
        .filter(|f| f.status == FixtureStatus::Completed && f.result.is_some())
        .map(|f| {
            let r = f.result.as_ref().unwrap();
            if f.home_team_id == user_team_id {
                r.home_wins as u32
            } else if f.away_team_id == user_team_id {
                r.away_wins as u32
            } else {
                0
            }
        })
        .sum();

    let user_wins = user_standing.map(|s| s.won).unwrap_or(0);

    for obj in game.board_objectives.iter_mut() {
        match obj.objective_type {
            ObjectiveType::LeaguePosition => {
                obj.met = league_complete && user_pos <= obj.target;
            }
            ObjectiveType::Wins => {
                obj.met = user_wins >= obj.target;
            }
            ObjectiveType::GoalsScored => {
                obj.met = user_goals >= obj.target;
            }
        }
    }
}

/// Evaluate objectives at end of season. Returns satisfaction delta.
pub fn evaluate_objectives(game: &Game) -> i8 {
    evaluate_objective_result(game).satisfaction_delta
}

pub fn evaluate_objective_result(game: &Game) -> ObjectiveEvaluation {
    if game.board_objectives.is_empty() {
        return ObjectiveEvaluation {
            met_count: 0,
            total: 0,
            satisfaction_delta: 0,
        };
    }
    let met_count = game.board_objectives.iter().filter(|o| o.met).count();
    let total = game.board_objectives.len();

    ObjectiveEvaluation {
        met_count,
        total,
        satisfaction_delta: satisfaction_delta(met_count, total),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ObjectiveProfile, evaluate_objectives, expected_league_rank, generate_objectives,
        objective_profile_for_rank, update_objective_progress,
    };
    use crate::clock::GameClock;
    use crate::domain::league::{
        Fixture, FixtureStatus, League, MatchResult, MatchType, StandingEntry,
    };
    use crate::domain::manager::Manager;
    use crate::domain::message::{InboxMessage, MessageCategory, MessagePriority};
    use crate::domain::player::{Player, PlayerAttributes};
    use crate::domain::stats::LolRole;
    use crate::domain::team::Team;
    use crate::game::{BoardObjective, Game, ObjectiveType};
    use chrono::{TimeZone, Utc};

    fn make_team(id: &str, name: &str, reputation: u32) -> Team {
        let mut team = Team::new(
            id.to_string(),
            name.to_string(),
            name.to_string(),
            "England".to_string(),
            "Testville".to_string(),
            "Test Ground".to_string(),
            20_000,
        );
        team.reputation = reputation;
        team
    }

    fn make_game(user_reputation: u32, season: u32, team_count: usize) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 8, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr1".to_string(),
            "Alex".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team1".to_string());

        let teams: Vec<Team> = (1..=team_count)
            .map(|idx| {
                make_team(
                    &format!("team{}", idx),
                    &format!("Team {}", idx),
                    if idx == 1 { user_reputation } else { 50 },
                )
            })
            .collect();
        let team_ids: Vec<String> = teams.iter().map(|team| team.id.clone()).collect();

        let mut game = Game::new(clock, manager, teams, vec![], vec![], vec![]);
        game.leagues = vec![League::new(
            "league1".to_string(),
            "Test League".to_string(),
            season,
            &team_ids,
            None,
        )];
        game
    }

    fn make_player(id: &str, team_id: &str, overall: u8) -> Player {
        let attrs = PlayerAttributes {
            mechanics: overall,
            laning: overall,
            teamfighting: overall,
            macro_play: overall,
            consistency: overall,
            shotcalling: overall,
            champion_pool: overall,
            discipline: overall,
            mental_resilience: overall,
        };
        let mut player = Player::new(
            id.to_string(),
            id.to_string(),
            format!("Full {id}"),
            "2000-01-01".to_string(),
            "ES".to_string(),
            LolRole::Mid,
            attrs,
        );
        player.team_id = Some(team_id.to_string());
        player.condition = 100;
        player
    }

    fn add_roster(game: &mut Game, team_id: &str, overall: u8) {
        for idx in 0..5 {
            game.players.push(make_player(
                &format!("{team_id}_player_{idx}"),
                team_id,
                overall,
            ));
        }
    }

    fn make_ranked_strength_game(user_team_id: &str, user_overall: u8) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 8, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr1".to_string(),
            "Alex".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire(user_team_id.to_string());

        let teams: Vec<Team> = (1..=10)
            .map(|idx| make_team(&format!("team{idx}"), &format!("Team {idx}"), 50))
            .collect();
        let team_ids: Vec<String> = teams.iter().map(|team| team.id.clone()).collect();
        let mut game = Game::new(clock, manager, teams, vec![], vec![], vec![]);
        game.leagues = vec![League::new(
            "league1".to_string(),
            "Test League".to_string(),
            1,
            &team_ids,
            None,
        )];

        let opponent_strengths = [88, 82, 76, 70, 64, 58, 52, 46, 40];
        for idx in 1..=10 {
            let team_id = format!("team{idx}");
            let overall = if team_id == user_team_id {
                user_overall
            } else {
                let opponent_idx = if idx == 1 { 0 } else { idx - 2 };
                opponent_strengths[opponent_idx]
            };
            add_roster(&mut game, &team_id, overall);
        }

        game
    }

    fn make_result(home_wins: u8, away_wins: u8) -> MatchResult {
        MatchResult {
            home_wins,
            away_wins,
            ..Default::default()
        }
    }

    fn make_objective(
        id: &str,
        objective_type: ObjectiveType,
        target: u32,
        met: bool,
    ) -> BoardObjective {
        BoardObjective {
            id: id.to_string(),
            description: format!("Objective {}", id),
            target,
            objective_type,
            met,
        }
    }

    fn objective_by_id<'a>(game: &'a Game, id: &str) -> &'a BoardObjective {
        game.board_objectives
            .iter()
            .find(|objective| objective.id == id)
            .unwrap()
    }

    #[test]
    fn generate_objectives_creates_targets_and_board_message() {
        let mut game = make_game(80, 3, 4);

        generate_objectives(&mut game);

        assert_eq!(game.board_objectives.len(), 3);
        assert_eq!(objective_by_id(&game, "obj_position").target, 1);
        assert_eq!(
            objective_by_id(&game, "obj_position").description,
            "boardObjectives.objective.LeaguePosition"
        );
        assert_eq!(objective_by_id(&game, "obj_wins").target, 3);
        assert_eq!(
            objective_by_id(&game, "obj_wins").description,
            "boardObjectives.objective.Wins"
        );
        assert_eq!(objective_by_id(&game, "obj_goals").target, 12);
        assert_eq!(
            objective_by_id(&game, "obj_goals").description,
            "boardObjectives.objective.GoalsScored"
        );

        let message = game
            .messages
            .iter()
            .find(|message| message.id == "board_objectives_3")
            .unwrap();
        assert_eq!(message.category, MessageCategory::BoardDirective);
        assert_eq!(message.priority, MessagePriority::High);
        assert_eq!(message.sender_role, "Chairman");
        assert_eq!(
            message.subject_key.as_deref(),
            Some("be.msg.boardObjectives.subject")
        );
        assert_eq!(
            message.body_key.as_deref(),
            Some("be.msg.boardObjectives.body")
        );
        assert_eq!(
            message.sender_key.as_deref(),
            Some("be.sender.boardOfDirectors")
        );
        assert_eq!(message.sender_role_key.as_deref(), Some("be.role.chairman"));
        assert_eq!(message.i18n_params.get("season"), Some(&"3".to_string()));
        assert_eq!(
            message.i18n_params.get("expectedPos"),
            Some(&"1".to_string())
        );
        assert_eq!(message.i18n_params.get("winTarget"), Some(&"3".to_string()));
        assert_eq!(
            message.i18n_params.get("goalsTarget"),
            Some(&"12".to_string())
        );
        assert!(message.body.contains("Win at least 3 series"));
        assert!(message.body.contains("Win at least 12 maps"));
        assert!(!message.body.contains("Score at least"));
        assert!(!message.body.contains("goals"));
    }

    #[test]
    fn generate_objectives_does_not_duplicate_existing_board_message() {
        let mut game = make_game(60, 2, 4);
        game.messages.push(
            InboxMessage::new(
                "board_objectives_2".to_string(),
                "Existing".to_string(),
                "Body".to_string(),
                "Board".to_string(),
                "2025-08-01".to_string(),
            )
            .with_category(MessageCategory::BoardDirective)
            .with_priority(MessagePriority::High),
        );

        generate_objectives(&mut game);

        assert_eq!(game.board_objectives.len(), 3);
        assert_eq!(
            game.messages
                .iter()
                .filter(|message| message.id == "board_objectives_2")
                .count(),
            1
        );
    }

    #[test]
    fn generate_objectives_scope_targets_to_active_league_and_user_schedule() {
        let mut game = make_game(80, 1, 30);
        for team in game.teams.iter_mut().skip(4) {
            team.reputation = 99;
        }

        let active_team_ids = vec![
            "team1".to_string(),
            "team2".to_string(),
            "team3".to_string(),
            "team4".to_string(),
        ];
        let mut active_league = League::new(
            "league1".to_string(),
            "Active League".to_string(),
            1,
            &active_team_ids,
            Some("active".to_string()),
        );
        active_league.fixtures = vec![
            Fixture {
                id: "f1".to_string(),
                matchday: 1,
                date: "2025-08-01".to_string(),
                home_team_id: "team1".to_string(),
                away_team_id: "team2".to_string(),
                match_type: MatchType::League,
                best_of: 3,
                status: FixtureStatus::Scheduled,
                result: None,
            },
            Fixture {
                id: "f2".to_string(),
                matchday: 2,
                date: "2025-08-08".to_string(),
                home_team_id: "team3".to_string(),
                away_team_id: "team1".to_string(),
                match_type: MatchType::League,
                best_of: 3,
                status: FixtureStatus::Scheduled,
                result: None,
            },
            Fixture {
                id: "f3".to_string(),
                matchday: 3,
                date: "2025-08-15".to_string(),
                home_team_id: "team1".to_string(),
                away_team_id: "team4".to_string(),
                match_type: MatchType::League,
                best_of: 3,
                status: FixtureStatus::Scheduled,
                result: None,
            },
        ];

        let other_team_ids: Vec<String> = (5..=30).map(|idx| format!("team{idx}")).collect();
        game.leagues = vec![
            active_league,
            League::new(
                "league2".to_string(),
                "Other League".to_string(),
                1,
                &other_team_ids,
                Some("other".to_string()),
            ),
        ];
        game.user_competition_id = Some("active".to_string());

        generate_objectives(&mut game);

        assert_eq!(objective_by_id(&game, "obj_position").target, 1);
        assert!(objective_by_id(&game, "obj_position").target <= active_team_ids.len() as u32);
        assert_eq!(objective_by_id(&game, "obj_wins").target, 1);
        assert!(objective_by_id(&game, "obj_wins").target <= 3);
        assert_eq!(objective_by_id(&game, "obj_goals").target, 6);
        assert!(objective_by_id(&game, "obj_goals").target <= 6);
    }

    #[test]
    fn expected_league_rank_uses_roster_strength_over_reputation() {
        let game = make_ranked_strength_game("team1", 90);

        assert_eq!(expected_league_rank("team1", &game.teams, &game.players), 1);
        assert_eq!(
            objective_profile_for_rank(1, 10),
            ObjectiveProfile::TitleContender
        );

        let game = make_ranked_strength_game("team5", 67);

        assert_eq!(expected_league_rank("team5", &game.teams, &game.players), 5);
        assert_eq!(
            objective_profile_for_rank(5, 10),
            ObjectiveProfile::PlayoffContender
        );

        let game = make_ranked_strength_game("team10", 35);

        assert_eq!(
            expected_league_rank("team10", &game.teams, &game.players),
            10
        );
        assert_eq!(
            objective_profile_for_rank(10, 10),
            ObjectiveProfile::Survival
        );
    }

    #[test]
    fn generate_objectives_scales_targets_for_strong_mid_and_weak_rosters() {
        let mut strong_game = make_ranked_strength_game("team1", 90);
        generate_objectives(&mut strong_game);

        let strong_position = objective_by_id(&strong_game, "obj_position").target;
        let strong_wins = objective_by_id(&strong_game, "obj_wins").target;
        let strong_maps = objective_by_id(&strong_game, "obj_goals").target;

        assert_eq!(strong_position, 1);

        let mut mid_game = make_ranked_strength_game("team5", 67);
        generate_objectives(&mut mid_game);

        let mid_position = objective_by_id(&mid_game, "obj_position").target;
        let mid_wins = objective_by_id(&mid_game, "obj_wins").target;
        let mid_maps = objective_by_id(&mid_game, "obj_goals").target;

        assert!(mid_position > strong_position);
        assert!(mid_wins < strong_wins);
        assert!(mid_maps < strong_maps);

        let mut weak_game = make_ranked_strength_game("team10", 35);
        generate_objectives(&mut weak_game);

        let weak_position = objective_by_id(&weak_game, "obj_position").target;
        let weak_wins = objective_by_id(&weak_game, "obj_wins").target;
        let weak_maps = objective_by_id(&weak_game, "obj_goals").target;

        assert!(weak_position > mid_position);
        assert!(weak_wins < mid_wins);
        assert!(weak_maps < mid_maps);
        assert_eq!(weak_position, 9);
    }

    #[test]
    fn update_objective_progress_updates_each_objective_from_league_state() {
        let mut game = make_game(60, 1, 3);
        game.board_objectives = vec![
            make_objective("obj_position", ObjectiveType::LeaguePosition, 1, false),
            make_objective("obj_wins", ObjectiveType::Wins, 4, false),
            make_objective("obj_goals", ObjectiveType::GoalsScored, 6, false),
        ];

        let mut league = game.active_league().cloned().unwrap();
        league.standings = vec![
            StandingEntry {
                team_id: "team1".to_string(),
                played: 4,
                won: 4,
                lost: 0,
                maps_won: 5,
                maps_lost: 1,
                points: 12,
            },
            StandingEntry {
                team_id: "team2".to_string(),
                played: 5,
                won: 5,
                lost: 0,
                maps_won: 9,
                maps_lost: 2,
                points: 15,
            },
            StandingEntry {
                team_id: "team3".to_string(),
                played: 4,
                won: 1,
                lost: 3,
                maps_won: 2,
                maps_lost: 7,
                points: 3,
            },
        ];
        league.fixtures = vec![
            Fixture {
                id: "f1".to_string(),
                matchday: 1,
                date: "2025-08-01".to_string(),
                home_team_id: "team1".to_string(),
                away_team_id: "team2".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Completed,
                result: Some(make_result(2, 1)),
            },
            Fixture {
                id: "f2".to_string(),
                matchday: 2,
                date: "2025-08-08".to_string(),
                home_team_id: "team3".to_string(),
                away_team_id: "team1".to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status: FixtureStatus::Completed,
                result: Some(make_result(0, 3)),
            },
        ];
        game.leagues = vec![league];

        update_objective_progress(&mut game);

        assert!(!objective_by_id(&game, "obj_position").met);
        assert!(objective_by_id(&game, "obj_wins").met);
        assert!(!objective_by_id(&game, "obj_goals").met);
    }

    #[test]
    fn update_objective_progress_only_marks_league_position_met_after_all_fixtures_finish() {
        let mut game = make_game(80, 1, 4);
        game.board_objectives = vec![make_objective(
            "obj_position",
            ObjectiveType::LeaguePosition,
            1,
            false,
        )];

        let mut league = game.active_league().cloned().unwrap();
        let fixture = |id: &str,
                       matchday: u32,
                       home_team_id: &str,
                       away_team_id: &str,
                       status: FixtureStatus,
                       score: Option<(u8, u8)>| {
            Fixture {
                id: id.to_string(),
                matchday,
                date: format!("2025-08-{:02}", matchday),
                home_team_id: home_team_id.to_string(),
                away_team_id: away_team_id.to_string(),
                match_type: MatchType::League,
                best_of: 1,
                status,
                result: score.map(|(home_wins, away_wins)| make_result(home_wins, away_wins)),
            }
        };
        league.standings = vec![
            StandingEntry {
                team_id: "team1".to_string(),
                played: 6,
                won: 5,
                lost: 0,
                maps_won: 12,
                maps_lost: 3,
                points: 16,
            },
            StandingEntry {
                team_id: "team2".to_string(),
                played: 6,
                won: 3,
                lost: 2,
                maps_won: 7,
                maps_lost: 6,
                points: 10,
            },
            StandingEntry {
                team_id: "team3".to_string(),
                played: 6,
                won: 1,
                lost: 3,
                maps_won: 4,
                maps_lost: 8,
                points: 5,
            },
            StandingEntry {
                team_id: "team4".to_string(),
                played: 6,
                won: 0,
                lost: 4,
                maps_won: 2,
                maps_lost: 8,
                points: 2,
            },
        ];
        league.fixtures = vec![
            fixture(
                "f1",
                1,
                "team1",
                "team2",
                FixtureStatus::Completed,
                Some((2, 0)),
            ),
            fixture(
                "f2",
                2,
                "team3",
                "team4",
                FixtureStatus::Completed,
                Some((1, 0)),
            ),
            fixture(
                "f3",
                3,
                "team1",
                "team3",
                FixtureStatus::Completed,
                Some((3, 1)),
            ),
            fixture(
                "f4",
                4,
                "team2",
                "team4",
                FixtureStatus::Completed,
                Some((2, 1)),
            ),
            fixture(
                "f5",
                5,
                "team1",
                "team4",
                FixtureStatus::Completed,
                Some((2, 0)),
            ),
            fixture(
                "f6",
                6,
                "team2",
                "team3",
                FixtureStatus::Completed,
                Some((1, 1)),
            ),
            fixture(
                "f7",
                7,
                "team2",
                "team1",
                FixtureStatus::Completed,
                Some((0, 1)),
            ),
            fixture(
                "f8",
                8,
                "team4",
                "team3",
                FixtureStatus::Completed,
                Some((0, 0)),
            ),
            fixture(
                "f9",
                9,
                "team3",
                "team1",
                FixtureStatus::Completed,
                Some((0, 2)),
            ),
            fixture(
                "f10",
                10,
                "team4",
                "team2",
                FixtureStatus::Completed,
                Some((1, 2)),
            ),
            fixture(
                "f11",
                11,
                "team4",
                "team1",
                FixtureStatus::Completed,
                Some((1, 2)),
            ),
            fixture("f12", 12, "team3", "team2", FixtureStatus::Scheduled, None),
        ];
        game.leagues = vec![league.clone()];

        update_objective_progress(&mut game);

        assert!(!objective_by_id(&game, "obj_position").met);

        league.fixtures[11].status = FixtureStatus::Completed;
        league.fixtures[11].result = Some(make_result(0, 1));
        game.leagues = vec![league];

        update_objective_progress(&mut game);

        assert!(objective_by_id(&game, "obj_position").met);
    }

    #[test]
    fn evaluate_objectives_distinguishes_some_met_from_majority_met() {
        let mut game = make_game(60, 1, 3);

        assert_eq!(evaluate_objectives(&game), 0);

        game.board_objectives = vec![
            make_objective("a", ObjectiveType::LeaguePosition, 1, true),
            make_objective("b", ObjectiveType::Wins, 1, false),
            make_objective("c", ObjectiveType::GoalsScored, 1, false),
        ];
        assert_eq!(evaluate_objectives(&game), -5);

        game.board_objectives = vec![
            make_objective("a", ObjectiveType::LeaguePosition, 1, true),
            make_objective("b", ObjectiveType::Wins, 1, true),
            make_objective("c", ObjectiveType::GoalsScored, 1, false),
        ];
        assert_eq!(evaluate_objectives(&game), 5);

        game.board_objectives = vec![
            make_objective("a", ObjectiveType::LeaguePosition, 1, true),
            make_objective("b", ObjectiveType::Wins, 1, true),
            make_objective("c", ObjectiveType::GoalsScored, 1, true),
        ];
        assert_eq!(evaluate_objectives(&game), 15);

        game.board_objectives = vec![
            make_objective("a", ObjectiveType::LeaguePosition, 1, true),
            make_objective("b", ObjectiveType::Wins, 1, true),
            make_objective("c", ObjectiveType::GoalsScored, 1, false),
            make_objective("d", ObjectiveType::GoalsScored, 1, false),
        ];
        assert_eq!(evaluate_objectives(&game), -5);

        game.board_objectives = vec![
            make_objective("a", ObjectiveType::LeaguePosition, 1, false),
            make_objective("b", ObjectiveType::Wins, 1, false),
            make_objective("c", ObjectiveType::GoalsScored, 1, false),
        ];
        assert_eq!(evaluate_objectives(&game), -15);
    }
}
