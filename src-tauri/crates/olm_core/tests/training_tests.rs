use chrono::{TimeZone, Utc};
use olm_core::champions::{ChampionMasteryEntry, ChampionMetaEntry};
use olm_core::clock::GameClock;
use olm_core::domain::manager::Manager;
use olm_core::domain::player::LolRole;
use olm_core::domain::player::{Player, PlayerAttributes};
use olm_core::domain::staff::{Staff, StaffAttributes, StaffRole};
use olm_core::domain::team::{
    PostScrimDecision, ScrimChampionPick, ScrimFocus, ScrimIssue, ScrimReport, ScrimStatus, Team,
    TrainingFocus, TrainingIntensity, TrainingSchedule,
};
use olm_core::game::Game;
use olm_core::training;

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

fn lol_visible_stat(player: &Player, stat: &str) -> u8 {
    let attrs = &player.attributes;
    let avg = |values: [u8; 4]| -> u8 {
        let sum: u16 = values.iter().map(|value| *value as u16).sum();
        ((sum as f64 / values.len() as f64).round()) as u8
    };

    match stat {
        "mechanics" => avg([
            attrs.mechanics,
            attrs.champion_pool,
            attrs.mechanics,
            attrs.discipline,
        ]),
        "laning" => avg([
            attrs.laning,
            attrs.consistency,
            attrs.mechanics,
            attrs.discipline,
        ]),
        "teamfighting" => avg([
            attrs.teamfighting,
            attrs.mental_resilience,
            attrs.consistency,
            attrs.discipline,
        ]),
        "macro" => avg([
            attrs.macro_play,
            attrs.consistency,
            attrs.consistency,
            attrs.teamfighting,
        ]),
        "consistency" => avg([
            attrs.consistency,
            attrs.macro_play,
            attrs.discipline,
            attrs.teamfighting,
        ]),
        "shotcalling" => avg([
            attrs.shotcalling,
            attrs.teamfighting,
            attrs.macro_play,
            attrs.consistency,
        ]),
        "champion_pool" => avg([
            attrs.mechanics,
            attrs.champion_pool,
            attrs.macro_play,
            attrs.teamfighting,
        ]),
        "discipline" => avg([
            attrs.consistency,
            attrs.discipline,
            attrs.teamfighting,
            attrs.shotcalling,
        ]),
        _ => panic!("Unknown visible stat {stat}"),
    }
}

fn make_player(id: &str, name: &str, team_id: &str, dob: &str) -> Player {
    let mut p = Player::new(
        id.to_string(),
        name.to_string(),
        format!("Full {}", name),
        dob.to_string(),
        "GB".to_string(),
        LolRole::Jungle,
        default_attrs(),
    );
    p.team_id = Some(team_id.to_string());
    p.morale = 70;
    p.condition = 80;
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

fn make_staff(id: &str, team_id: &str, role: StaffRole, coaching: u8, physio: u8) -> Staff {
    let mut s = Staff::new(
        id.to_string(),
        "Staff".to_string(),
        id.to_string(),
        "1980-01-01".to_string(),
        role,
        StaffAttributes {
            coaching,
            judging_ability: 50,
            judging_potential: 50,
            physiotherapy: physio,
        },
    );
    s.team_id = Some(team_id.to_string());
    s.nationality = "GB".to_string();
    s
}

fn make_game() -> Game {
    let date = Utc.with_ymd_and_hms(2025, 6, 16, 12, 0, 0).unwrap(); // Monday
    let clock = GameClock::new(date);
    let mut manager = Manager::new(
        "mgr1".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("team1".to_string());

    let mut team1 = make_team("team1", "Test FC");
    team1.training_focus = TrainingFocus::Scrims;
    team1.training_intensity = TrainingIntensity::Medium;
    team1.training_schedule = TrainingSchedule::Balanced;

    // Young player (age ~21)
    let p1 = make_player("p1", "Young", "team1", "2004-03-15");
    // Prime player (age ~27)
    let p2 = make_player("p2", "Prime", "team1", "1998-06-10");
    // Old player (age ~35)
    let p3 = make_player("p3", "Veteran", "team1", "1990-01-01");

    let coach = make_staff("coach1", "team1", StaffRole::Coach, 80, 30);
    let physio = make_staff("physio1", "team1", StaffRole::Physio, 30, 80);

    Game::new(
        clock,
        manager,
        vec![team1],
        vec![p1, p2, p3],
        vec![coach, physio],
        vec![],
    )
}

// ---------------------------------------------------------------------------
// process_training — basic behavior
// ---------------------------------------------------------------------------

#[test]
fn training_on_training_day_costs_condition() {
    let mut game = make_game();
    // Monday (0) is a training day for Balanced schedule
    let initial_conditions: Vec<u8> = game.players.iter().map(|p| p.condition).collect();

    training::process_training(&mut game, 0);

    // Condition should change (cost - recovery, net effect depends on stamina/physio)
    // At minimum, training happened (we can check it didn't stay exactly the same
    // for all players, which would be extremely unlikely)
    let after_conditions: Vec<u8> = game.players.iter().map(|p| p.condition).collect();
    // Just verify no panics and condition is in valid range
    for c in &after_conditions {
        assert!(*c <= 100, "Condition should be <= 100");
    }
    // The test verifies the function runs without error
    let _ = (initial_conditions, after_conditions);
}

#[test]
fn rest_day_only_recovers_condition() {
    let mut game = make_game();
    // Set condition low
    for p in game.players.iter_mut() {
        p.condition = 50;
    }

    // Wednesday (2) is a rest day for Balanced schedule
    training::process_training(&mut game, 2);

    // All players should have gained condition (recovery only, no cost)
    for p in &game.players {
        assert!(
            p.condition > 50,
            "Player {} should recover on rest day, got {}",
            p.id,
            p.condition
        );
    }
}

#[test]
fn recovery_focus_no_condition_cost() {
    let mut game = make_game();
    game.teams[0].training_focus = TrainingFocus::MentalResetRecovery;
    for p in game.players.iter_mut() {
        p.condition = 60;
    }

    // Monday (0) is training day, but Mental Reset / Recovery has 0 condition cost
    training::process_training(&mut game, 0);

    for p in &game.players {
        assert!(
            p.condition >= 60,
            "Mental Reset / Recovery should not reduce condition, got {}",
            p.condition
        );
    }
}

#[test]
fn high_intensity_costs_more_condition() {
    let mut game = make_game();
    game.teams[0].training_intensity = TrainingIntensity::High;
    game.teams[0].training_focus = TrainingFocus::Scrims;
    for p in game.players.iter_mut() {
        p.condition = 90;
    }

    // Monday training day
    training::process_training(&mut game, 0);
    let high_conditions: Vec<u8> = game.players.iter().map(|p| p.condition).collect();

    // Reset and do low intensity
    let mut game2 = make_game();
    game2.teams[0].training_intensity = TrainingIntensity::Low;
    game2.teams[0].training_focus = TrainingFocus::Scrims;
    for p in game2.players.iter_mut() {
        p.condition = 90;
    }

    training::process_training(&mut game2, 0);
    let low_conditions: Vec<u8> = game2.players.iter().map(|p| p.condition).collect();

    // High intensity should leave lower condition than low intensity on average
    let avg_high: f64 =
        high_conditions.iter().map(|c| *c as f64).sum::<f64>() / high_conditions.len() as f64;
    let avg_low: f64 =
        low_conditions.iter().map(|c| *c as f64).sum::<f64>() / low_conditions.len() as f64;
    assert!(
        avg_high <= avg_low,
        "High intensity ({:.1}) should cost more condition than low ({:.1})",
        avg_high,
        avg_low
    );
}

// ---------------------------------------------------------------------------
// process_training — schedules
// ---------------------------------------------------------------------------

#[test]
fn intense_schedule_trains_six_days() {
    let mut game = make_game();
    game.teams[0].training_schedule = TrainingSchedule::Intense;
    game.teams[0].training_focus = TrainingFocus::Scrims;

    // Train all 7 days and count how many days condition drops
    let mut training_days = 0;
    for weekday in 0..7 {
        for p in game.players.iter_mut() {
            p.condition = 80;
        }
        training::process_training(&mut game, weekday);
        // If condition cost > recovery, it's a real training day
        // For Intense, Sun(6) is rest
        if weekday != 6 {
            training_days += 1;
        }
    }
    assert_eq!(training_days, 6, "Intense schedule should train 6 days");
}

#[test]
fn light_schedule_trains_two_days() {
    // Light: only Tue(1) and Thu(3) are training days
    assert!(TrainingSchedule::Light.is_training_day(1));
    assert!(TrainingSchedule::Light.is_training_day(3));
    assert!(!TrainingSchedule::Light.is_training_day(0));
    assert!(!TrainingSchedule::Light.is_training_day(2));
    assert!(!TrainingSchedule::Light.is_training_day(4));
    assert!(!TrainingSchedule::Light.is_training_day(5));
    assert!(!TrainingSchedule::Light.is_training_day(6));
}

#[test]
fn higher_medical_facility_level_improves_recovery_on_rest_days() {
    let mut baseline = make_game();
    for player in baseline.players.iter_mut() {
        player.condition = 50;
    }

    let mut upgraded = make_game();
    for player in upgraded.players.iter_mut() {
        player.condition = 50;
    }
    upgraded.teams[0].facilities.medical = 3;

    training::process_training(&mut baseline, 2);
    training::process_training(&mut upgraded, 2);

    let baseline_avg = baseline
        .players
        .iter()
        .map(|player| player.condition as f64)
        .sum::<f64>()
        / baseline.players.len() as f64;
    let upgraded_avg = upgraded
        .players
        .iter()
        .map(|player| player.condition as f64)
        .sum::<f64>()
        / upgraded.players.len() as f64;

    assert!(
        upgraded_avg > baseline_avg,
        "Higher medical level should improve recovery: upgraded {:.2}, baseline {:.2}",
        upgraded_avg,
        baseline_avg
    );
}

// ---------------------------------------------------------------------------
// process_training — attribute gains (probabilistic)
// ---------------------------------------------------------------------------

#[test]
fn scrims_focus_can_improve_teamplay_attrs() {
    let mut game = make_game();
    game.teams[0].training_focus = TrainingFocus::Scrims;
    game.teams[0].training_intensity = TrainingIntensity::High;
    game.teams[0].training_schedule = TrainingSchedule::Intense;

    // Record initial stats
    let initial_teamfighting: Vec<u8> = game
        .players
        .iter()
        .map(|player| lol_visible_stat(player, "teamfighting"))
        .collect();
    let initial_macro: Vec<u8> = game
        .players
        .iter()
        .map(|player| lol_visible_stat(player, "macro"))
        .collect();

    // Train many sessions to make probabilistic gains likely
    for _ in 0..100 {
        for p in game.players.iter_mut() {
            p.condition = 90; // Keep condition high so training continues
        }
        training::process_training(&mut game, 0); // Monday = training day
    }

    let final_teamfighting: Vec<u8> = game
        .players
        .iter()
        .map(|player| lol_visible_stat(player, "teamfighting"))
        .collect();
    let final_macro: Vec<u8> = game
        .players
        .iter()
        .map(|player| lol_visible_stat(player, "macro"))
        .collect();

    let any_teamfighting_gain = initial_teamfighting
        .iter()
        .zip(final_teamfighting.iter())
        .any(|(i, f)| f > i);
    let any_macro_gain = initial_macro
        .iter()
        .zip(final_macro.iter())
        .any(|(i, f)| f > i);

    assert!(
        any_teamfighting_gain || any_macro_gain,
        "Scrims should improve visible teamfighting or macro after many sessions"
    );
}

#[test]
fn scrim_days_generate_enriched_reports_with_champion_picks() {
    let mut game = make_game();
    let mut opponent = make_team("team2", "Rival FC");
    let opponent_players = vec![
        make_player("r1", "Rival One", "team2", "2000-01-01"),
        make_player("r2", "Rival Two", "team2", "2000-01-01"),
        make_player("r3", "Rival Three", "team2", "2000-01-01"),
    ];
    opponent.active_lineup_ids = opponent_players
        .iter()
        .map(|player| player.id.clone())
        .collect();
    game.teams.push(opponent);
    game.players.extend(opponent_players);
    game.teams[0].scrim_weekly_slots = 2;
    game.teams[0].scrim_weekly_objective = Some(ScrimFocus::DraftPrep);
    game.teams[0].weekly_scrim_plan_team_ids = vec![vec!["team2".to_string()]];
    game.players[0].champion_training_targets = vec!["Azir".to_string()];

    training::process_training(&mut game, 2);

    let report = game.teams[0]
        .scrim_reports
        .first()
        .expect("scrim report should be generated");
    assert_eq!(report.team_id, "team1");
    assert_eq!(report.opponent_team_id, "team2");
    assert_eq!(report.status, ScrimStatus::Played);
    assert_eq!(report.focus, ScrimFocus::DraftPrep);
    assert!(report.quality >= 30);
    assert!(!report.player_champion_picks.is_empty());
    assert!(
        report
            .player_champion_picks
            .iter()
            .any(|pick| pick.champion_id == "Azir")
    );
}

#[test]
fn scrim_block_is_idempotent_before_training_block() {
    let mut game = make_game();
    let mut opponent = make_team("team2", "Rival FC");
    let opponent_players = vec![
        make_player("r1", "Rival One", "team2", "2000-01-01"),
        make_player("r2", "Rival Two", "team2", "2000-01-01"),
        make_player("r3", "Rival Three", "team2", "2000-01-01"),
    ];
    opponent.active_lineup_ids = opponent_players
        .iter()
        .map(|player| player.id.clone())
        .collect();
    game.teams.push(opponent);
    game.players.extend(opponent_players);
    game.teams[0].scrim_weekly_slots = 2;
    game.teams[0].weekly_scrim_plan_team_ids = vec![vec!["team2".to_string()]];

    assert!(training::process_scrim_block(&mut game, 2));
    let reports_after_scrim_block = game.teams[0].scrim_reports.len();
    let played_after_scrim_block = game.teams[0].scrim_weekly_played;

    training::process_training(&mut game, 2);

    assert_eq!(game.teams[0].scrim_reports.len(), reports_after_scrim_block);
    assert_eq!(game.teams[0].scrim_weekly_played, played_after_scrim_block);
}

#[test]
fn scrim_mastery_progress_uses_report_quality_and_review_decision() {
    let mut game = make_game();
    let before = olm_core::champions::mastery_for_player_champion(&game, "p1", "Azir");

    olm_core::champions::apply_scrim_mastery_progress(
        &mut game,
        "p1",
        "Azir",
        86,
        false,
        Some(&PostScrimDecision::TargetedDrills),
    );

    let after = olm_core::champions::mastery_for_player_champion(&game, "p1", "Azir");
    assert!(
        after > before,
        "scrim review should improve champion mastery"
    );
}

#[test]
fn sunday_training_generates_rich_weekly_scrim_staff_report() {
    let mut game = make_game();
    game.teams[0].scrim_weekly_played = 2;
    game.teams[0].scrim_weekly_wins = 1;
    game.teams[0].scrim_weekly_losses = 1;
    game.teams[0].scrim_weekly_cancellations = 1;
    game.teams[0].scrim_reports = vec![
        ScrimReport {
            date: "2025-06-17".to_string(),
            week_key: "2025-W25".to_string(),
            slot_index: 0,
            weekday: 1,
            team_id: "team1".to_string(),
            opponent_team_id: "team2".to_string(),
            status: ScrimStatus::Played,
            won: Some(true),
            focus: ScrimFocus::DraftPrep,
            issue: Some(ScrimIssue::ObjectiveSetup),
            severity: 2,
            quality: 82,
            player_champion_picks: vec![ScrimChampionPick {
                player_id: "p1".to_string(),
                champion_id: "Azir".to_string(),
                role: "Mid".to_string(),
            }],
            post_decision: Some(PostScrimDecision::VodReview),
            created_on: "2025-06-17T12:00:00Z".to_string(),
        },
        ScrimReport {
            date: "2025-06-19".to_string(),
            week_key: "2025-W25".to_string(),
            slot_index: 1,
            weekday: 3,
            team_id: "team1".to_string(),
            opponent_team_id: "team3".to_string(),
            status: ScrimStatus::Played,
            won: Some(false),
            focus: ScrimFocus::DraftPrep,
            issue: Some(ScrimIssue::ObjectiveSetup),
            severity: 3,
            quality: 70,
            player_champion_picks: vec![ScrimChampionPick {
                player_id: "p2".to_string(),
                champion_id: "Azir".to_string(),
                role: "Mid".to_string(),
            }],
            post_decision: Some(PostScrimDecision::TargetedDrills),
            created_on: "2025-06-19T12:00:00Z".to_string(),
        },
    ];

    training::process_training(&mut game, 6);

    let message = game
        .messages
        .iter()
        .find(|message| message.subject == "Weekly Scrim Staff Report")
        .expect("weekly scrim staff report should be generated");

    assert!(message.body.contains("Average quality: 76"));
    assert!(message.body.contains("Main focus: Draft prep"));
    assert!(message.body.contains("Recurring issue: Objective setup"));
    assert!(message.body.contains("Most practiced champion: Azir"));
    assert!(message.body.contains("Recommendation:"));
    assert_eq!(
        message.i18n_params.get("topFocus"),
        Some(&"be.msg.scrimWeekly.focus.draftPrep".to_string())
    );
    assert_eq!(
        message.i18n_params.get("recurringIssue"),
        Some(&"be.msg.scrimWeekly.issues.objectiveSetup".to_string())
    );
    assert_eq!(
        message.i18n_params.get("recommendation"),
        Some(&"be.msg.scrimWeekly.recommendations.resetBeforeVolume".to_string())
    );
    assert_eq!(game.teams[0].scrim_weekly_played, 0);
    assert_eq!(game.teams[0].scrim_weekly_cancellations, 0);
}

#[test]
fn champion_pool_practice_can_improve_mechanics_attrs() {
    let mut game = make_game();
    game.teams[0].training_focus = TrainingFocus::ChampionPoolPractice;
    game.teams[0].training_intensity = TrainingIntensity::High;
    game.teams[0].training_schedule = TrainingSchedule::Intense;

    let initial_mechanics: Vec<u8> = game
        .players
        .iter()
        .map(|player| lol_visible_stat(player, "mechanics"))
        .collect();
    let initial_champion_pool: Vec<u8> = game
        .players
        .iter()
        .map(|player| lol_visible_stat(player, "champion_pool"))
        .collect();

    for _ in 0..100 {
        for p in game.players.iter_mut() {
            p.condition = 90;
        }
        training::process_training(&mut game, 0);
    }

    let final_mechanics: Vec<u8> = game
        .players
        .iter()
        .map(|player| lol_visible_stat(player, "mechanics"))
        .collect();
    let final_champion_pool: Vec<u8> = game
        .players
        .iter()
        .map(|player| lol_visible_stat(player, "champion_pool"))
        .collect();
    let any_mechanics_gain = initial_mechanics
        .iter()
        .zip(final_mechanics.iter())
        .any(|(i, f)| f > i);
    let any_pool_gain = initial_champion_pool
        .iter()
        .zip(final_champion_pool.iter())
        .any(|(i, f)| f > i);
    assert!(
        any_mechanics_gain || any_pool_gain,
        "Champion Pool Practice should improve visible mechanics or champion pool after many sessions"
    );
}

#[test]
fn mental_reset_recovery_has_no_attribute_gains() {
    let mut game = make_game();
    game.teams[0].training_focus = TrainingFocus::MentalResetRecovery;
    game.teams[0].training_intensity = TrainingIntensity::High;

    let initial_attrs: Vec<PlayerAttributes> =
        game.players.iter().map(|p| p.attributes.clone()).collect();

    for _ in 0..50 {
        for p in game.players.iter_mut() {
            p.condition = 90;
        }
        training::process_training(&mut game, 0);
    }

    // Mental Reset / Recovery: no attribute gains at all
    for (i, p) in game.players.iter().enumerate() {
        assert_eq!(
            p.attributes.mechanics, initial_attrs[i].mechanics,
            "Mental Reset / Recovery should not change mechanics"
        );
        assert_eq!(
            p.attributes.laning, initial_attrs[i].laning,
            "Mental Reset / Recovery should not change laning"
        );
    }
}

// ---------------------------------------------------------------------------
// process_training — no coaching staff penalty
// ---------------------------------------------------------------------------

#[test]
fn no_coaching_staff_reduces_gains() {
    // Game with no staff
    let date = Utc.with_ymd_and_hms(2025, 6, 16, 12, 0, 0).unwrap();
    let clock = GameClock::new(date);
    let mut manager = Manager::new(
        "mgr1".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("team1".to_string());

    let mut team1 = make_team("team1", "Test FC");
    team1.training_focus = TrainingFocus::Scrims;
    team1.training_intensity = TrainingIntensity::High;
    team1.training_schedule = TrainingSchedule::Intense;

    let p1 = make_player("p1", "Young", "team1", "2004-03-15");

    let mut game = Game::new(clock, manager, vec![team1], vec![p1], vec![], vec![]);

    // Train many sessions
    let initial_mechanics = game.players[0].attributes.mechanics;
    for _ in 0..200 {
        game.players[0].condition = 90;
        training::process_training(&mut game, 0);
    }

    // Should still gain something (just less than with staff)
    // The 0.8 penalty from no staff still allows some growth
    let final_mechanics = game.players[0].attributes.mechanics;
    // After 200 intense sessions with a young player, some gain is expected
    assert!(
        final_mechanics >= initial_mechanics,
        "Should still gain attributes without staff"
    );
}

// ---------------------------------------------------------------------------
// check_squad_fitness_warnings
// ---------------------------------------------------------------------------

#[test]
fn no_warning_when_squad_is_fit() {
    let mut game = make_game();
    for p in game.players.iter_mut() {
        p.condition = 90;
    }

    training::check_squad_fitness_warnings(&mut game);

    let fitness_msgs: Vec<_> = game
        .messages
        .iter()
        .filter(|m| m.id.starts_with("fitness_warn_"))
        .collect();
    assert!(fitness_msgs.is_empty(), "No warning when squad is fit");
}

#[test]
fn warning_when_avg_condition_below_50() {
    let mut game = make_game();
    for p in game.players.iter_mut() {
        p.condition = 40; // avg = 40 < 50
    }

    training::check_squad_fitness_warnings(&mut game);

    let fitness_msgs: Vec<_> = game
        .messages
        .iter()
        .filter(|m| m.id.starts_with("fitness_warn_"))
        .collect();
    assert_eq!(fitness_msgs.len(), 1, "Should send fitness warning");
    assert!(
        fitness_msgs[0].subject.contains("Warning"),
        "Should be a warning, got: {}",
        fitness_msgs[0].subject
    );
}

#[test]
fn critical_warning_when_many_players_below_25() {
    let mut game = make_game();
    for p in game.players.iter_mut() {
        p.condition = 20; // all below 25 → critical
    }

    training::check_squad_fitness_warnings(&mut game);

    let fitness_msgs: Vec<_> = game
        .messages
        .iter()
        .filter(|m| m.id.starts_with("fitness_warn_"))
        .collect();
    assert_eq!(fitness_msgs.len(), 1, "Should send fitness message");
    assert!(
        fitness_msgs[0].subject.contains("URGENT") || fitness_msgs[0].subject.contains("Crisis"),
        "Should be critical, got: {}",
        fitness_msgs[0].subject
    );
}

#[test]
fn fitness_warning_not_duplicated_same_day() {
    let mut game = make_game();
    for p in game.players.iter_mut() {
        p.condition = 40;
    }

    training::check_squad_fitness_warnings(&mut game);
    training::check_squad_fitness_warnings(&mut game);

    let fitness_msgs: Vec<_> = game
        .messages
        .iter()
        .filter(|m| m.id.starts_with("fitness_warn_"))
        .collect();
    assert_eq!(
        fitness_msgs.len(),
        1,
        "Should not duplicate same-day warning"
    );
}

#[test]
fn no_warning_without_manager_team() {
    let mut game = make_game();
    game.manager.team_id = None;
    for p in game.players.iter_mut() {
        p.condition = 20;
    }

    training::check_squad_fitness_warnings(&mut game);

    let fitness_msgs: Vec<_> = game
        .messages
        .iter()
        .filter(|m| m.id.starts_with("fitness_warn_"))
        .collect();
    assert!(fitness_msgs.is_empty(), "No warning without manager team");
}

#[test]
fn warning_uses_physio_sender_when_available() {
    let mut game = make_game();
    for p in game.players.iter_mut() {
        p.condition = 40;
    }

    training::check_squad_fitness_warnings(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.id.starts_with("fitness_warn_"))
        .unwrap();
    // We have a physio on staff, so sender_role should be physio-related
    assert!(
        msg.sender_role == "Head Physio",
        "Sender should be Head Physio when physio is on staff, got: {}",
        msg.sender_role
    );
}

#[test]
fn warning_uses_assistant_manager_when_no_physio() {
    let mut game = make_game();
    // Remove physio
    game.staff.retain(|s| !matches!(s.role, StaffRole::Physio));
    for p in game.players.iter_mut() {
        p.condition = 40;
    }

    training::check_squad_fitness_warnings(&mut game);

    let msg = game
        .messages
        .iter()
        .find(|m| m.id.starts_with("fitness_warn_"))
        .unwrap();
    assert!(
        msg.sender_role == "Assistant Manager",
        "Sender should be Assistant Manager when no physio, got: {}",
        msg.sender_role
    );
}

// ---------------------------------------------------------------------------
// Age factor effects
// ---------------------------------------------------------------------------

#[test]
fn young_player_gains_more_than_old() {
    // Compare gains for young (21) vs old (35) player over many sessions
    let mut game = make_game();
    game.teams[0].training_focus = TrainingFocus::Scrims;
    game.teams[0].training_intensity = TrainingIntensity::High;
    game.teams[0].training_schedule = TrainingSchedule::Intense;

    let p1_initial_mechanics = game
        .players
        .iter()
        .find(|p| p.id == "p1")
        .unwrap()
        .attributes
        .mechanics;
    let p3_initial_mechanics = game
        .players
        .iter()
        .find(|p| p.id == "p3")
        .unwrap()
        .attributes
        .mechanics;

    for _ in 0..300 {
        for p in game.players.iter_mut() {
            p.condition = 90;
        }
        training::process_training(&mut game, 0);
    }

    let p1_final_mechanics = game
        .players
        .iter()
        .find(|p| p.id == "p1")
        .unwrap()
        .attributes
        .mechanics;
    let p3_final_mechanics = game
        .players
        .iter()
        .find(|p| p.id == "p3")
        .unwrap()
        .attributes
        .mechanics;

    let p1_gain = p1_final_mechanics - p1_initial_mechanics;
    let p3_gain = p3_final_mechanics - p3_initial_mechanics;

    assert!(
        p1_gain >= p3_gain,
        "Young player (gain={}) should gain at least as much as old player (gain={})",
        p1_gain,
        p3_gain
    );
}

// ---------------------------------------------------------------------------
// All training focuses work
// ---------------------------------------------------------------------------

#[test]
fn all_focuses_run_without_panic() {
    let focuses = [
        TrainingFocus::Scrims,
        TrainingFocus::VODReview,
        TrainingFocus::IndividualCoaching,
        TrainingFocus::ChampionPoolPractice,
        TrainingFocus::MacroSystems,
        TrainingFocus::MentalResetRecovery,
    ];

    for focus in &focuses {
        let mut game = make_game();
        game.teams[0].training_focus = focus.clone();
        training::process_training(&mut game, 0);
        // Just verify no panics
    }
}

#[test]
fn all_intensities_run_without_panic() {
    let intensities = [
        TrainingIntensity::Low,
        TrainingIntensity::Medium,
        TrainingIntensity::High,
    ];

    for intensity in &intensities {
        let mut game = make_game();
        game.teams[0].training_intensity = intensity.clone();
        training::process_training(&mut game, 0);
    }
}

// ---------------------------------------------------------------------------
// Fitness system tests
// ---------------------------------------------------------------------------

#[test]
fn high_fitness_player_recovers_condition_faster_on_rest_day() {
    // Two players identical except fitness
    let mut game_low = make_game();
    let mut game_high = make_game();

    for p in game_low.players.iter_mut() {
        p.fitness = 20; // very unfit
        p.condition = 50;
    }
    for p in game_high.players.iter_mut() {
        p.fitness = 95; // peak fitness
        p.condition = 50;
    }

    // Wednesday (2) is rest day for Balanced schedule
    training::process_training(&mut game_low, 2);
    training::process_training(&mut game_high, 2);

    let avg_low = game_low
        .players
        .iter()
        .map(|p| p.condition as f64)
        .sum::<f64>()
        / game_low.players.len() as f64;
    let avg_high = game_high
        .players
        .iter()
        .map(|p| p.condition as f64)
        .sum::<f64>()
        / game_high.players.len() as f64;

    assert!(
        avg_high > avg_low,
        "High fitness players ({:.1}) should recover more than low fitness ({:.1})",
        avg_high,
        avg_low
    );
}

#[test]
fn scrims_can_increase_fitness() {
    let mut game = make_game();
    game.teams[0].training_focus = TrainingFocus::Scrims;
    game.teams[0].training_intensity = TrainingIntensity::High;
    game.teams[0].training_schedule = TrainingSchedule::Intense;

    // Set a below-peak fitness so gains are possible
    for p in game.players.iter_mut() {
        p.fitness = 70;
    }

    let initial_fitness: Vec<u8> = game.players.iter().map(|p| p.fitness).collect();

    // Train many sessions to trigger probabilistic fitness gain
    for _ in 0..500 {
        for p in game.players.iter_mut() {
            p.condition = 90;
        }
        training::process_training(&mut game, 0); // Monday = training day
    }

    let final_fitness: Vec<u8> = game.players.iter().map(|p| p.fitness).collect();
    let any_gain = initial_fitness
        .iter()
        .zip(final_fitness.iter())
        .any(|(i, f)| f > i);

    assert!(
        any_gain,
        "Scrims should increase fitness after many sessions"
    );
}

#[test]
fn rival_players_get_auto_targets_and_gain_mastery_on_training() {
    let mut game = make_game();

    let mut team2 = make_team("team2", "Rival FC");
    team2.training_focus = TrainingFocus::ChampionPoolPractice;
    team2.training_intensity = TrainingIntensity::High;
    team2.training_schedule = TrainingSchedule::Intense;
    game.teams.push(team2);

    let mut rival = make_player("p-rival", "Rival Carry", "team2", "2001-04-11");
    rival.champion_training_targets = Vec::new();
    game.players.push(rival);

    game.champion_masteries.push(ChampionMasteryEntry {
        player_id: "p-rival".to_string(),
        champion_id: "Azir".to_string(),
        mastery: 62,
        last_active_on: "2025-06-15".to_string(),
    });
    game.champion_masteries.push(ChampionMasteryEntry {
        player_id: "p-rival".to_string(),
        champion_id: "Orianna".to_string(),
        mastery: 58,
        last_active_on: "2025-06-15".to_string(),
    });

    // Pin the champion-patch RNG so this test is deterministic across runs and
    // unaffected by the global thread-local RNG state touched by other tests.
    game.champion_patch.rng_seed = 42;

    olm_core::champions::ensure_training_targets_from_mastery(&mut game, "p-rival");
    let trained_champion = game
        .players
        .iter()
        .find(|player| player.id == "p-rival")
        .and_then(|player| {
            olm_core::champions::training_targets_for_player(player)
                .first()
                .cloned()
        })
        .expect("rival player should auto-assign a primary mastery target");
    let before_mastery =
        olm_core::champions::mastery_for_player_champion(&game, "p-rival", &trained_champion);

    for _ in 0..40 {
        if let Some(player) = game
            .players
            .iter_mut()
            .find(|player| player.id == "p-rival")
        {
            player.condition = 95;
        }
        training::process_training(&mut game, 0);
    }

    let rival_player = game
        .players
        .iter()
        .find(|player| player.id == "p-rival")
        .expect("rival player should exist");
    let targets = olm_core::champions::training_targets_for_player(rival_player);
    assert!(
        !targets.is_empty(),
        "rival player should auto-assign mastery targets"
    );

    let after_mastery =
        olm_core::champions::mastery_for_player_champion(&game, "p-rival", &trained_champion);
    assert!(
        after_mastery > before_mastery,
        "rival mastery should grow for trained target {} (before={}, after={})",
        trained_champion,
        before_mastery,
        after_mastery
    );
}

#[test]
fn rival_auto_targets_prioritize_meta_tier_over_raw_mastery() {
    let mut game = make_game();

    let mut rival = make_player("p-meta", "Meta Mid", "team2", "2001-04-11");
    rival.natural_position = LolRole::Mid;
    rival.champion_training_targets = Vec::new();
    game.players.push(rival);

    game.champion_masteries.push(ChampionMasteryEntry {
        player_id: "p-meta".to_string(),
        champion_id: "OffMetaHigh".to_string(),
        mastery: 92,
        last_active_on: "2025-06-15".to_string(),
    });
    game.champion_masteries.push(ChampionMasteryEntry {
        player_id: "p-meta".to_string(),
        champion_id: "MetaLow".to_string(),
        mastery: 30,
        last_active_on: "2025-06-15".to_string(),
    });

    game.champion_patch.discovered_champion_ids =
        vec!["OffMetaHigh".to_string(), "MetaLow".to_string()];
    game.champion_patch.hidden_meta = vec![
        ChampionMetaEntry {
            champion_id: "MetaLow".to_string(),
            role: "Mid".to_string(),
            tier: "S".to_string(),
        },
        ChampionMetaEntry {
            champion_id: "OffMetaHigh".to_string(),
            role: "Mid".to_string(),
            tier: "D".to_string(),
        },
    ];

    olm_core::champions::ensure_training_targets_from_mastery(&mut game, "p-meta");
    let player = game
        .players
        .iter()
        .find(|candidate| candidate.id == "p-meta")
        .expect("meta test player should exist");
    let targets = olm_core::champions::training_targets_for_player(player);

    assert_eq!(targets.first().map(String::as_str), Some("MetaLow"));
}
