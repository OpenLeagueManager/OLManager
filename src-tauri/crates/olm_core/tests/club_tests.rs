use chrono::{TimeZone, Utc};
use olm_core::clock::GameClock;
use olm_core::club;
use olm_core::domain::manager::Manager;
use olm_core::domain::team::{
    Facilities, FacilityType, FinancialTransactionKind, MainFacilityModuleKind, Team,
};
use olm_core::game::Game;

fn make_team(id: &str, name: &str) -> Team {
    let mut team = Team::new(
        id.to_string(),
        name.to_string(),
        name[..3].to_string(),
        "England".to_string(),
        "London".to_string(),
        "Stadium".to_string(),
        40_000,
    );
    team.finance = 2_000_000;
    team
}

fn make_game() -> Game {
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

    let team1 = make_team("team1", "Test FC");

    Game::new(clock, manager, vec![team1], vec![], vec![], vec![])
}

#[test]
fn upgrade_facility_deducts_funds_and_increments_level() {
    let mut game = make_game();
    let initial_finance = game.teams[0].finance;

    let cost =
        club::upgrade_facility(&mut game.teams[0], FacilityType::Medical, "2025-06-16").unwrap();

    assert_eq!(cost, 500_000);
    assert_eq!(game.teams[0].finance, initial_finance - cost);
    assert_eq!(game.teams[0].season_expenses, cost);
    assert_eq!(game.teams[0].financial_ledger.len(), 1);
    assert_eq!(
        game.teams[0].financial_ledger[0].kind,
        FinancialTransactionKind::FacilityUpgrade
    );
    assert_eq!(game.teams[0].financial_ledger[0].amount, -cost);
    assert_eq!(game.teams[0].financial_ledger[0].date, "2025-06-16");
    assert_eq!(game.teams[0].financial_ledger[0].source, "facility");
    assert_eq!(game.teams[0].facilities.medical, 2);
}

#[test]
fn upgrade_facility_rejects_when_funds_are_insufficient() {
    let mut game = make_game();
    game.teams[0].finance = 100_000;
    game.teams[0].facilities = Facilities {
        main_hub_level: 1,
        training: 1,
        medical: 1,
        scouting: 1,
        ..Default::default()
    };

    let result = club::upgrade_facility(&mut game.teams[0], FacilityType::Training, "2025-06-16");

    assert!(result.is_err());
    assert_eq!(game.teams[0].finance, 100_000);
    assert_eq!(game.teams[0].facilities.training, 1);
}

#[test]
fn expand_main_facility_hub_deducts_funds_and_unlocks_next_module_level() {
    let mut game = make_game();
    let initial_finance = game.teams[0].finance;

    let cost = club::expand_main_facility_hub(&mut game.teams[0], "2025-06-16").unwrap();

    assert_eq!(cost, 1_000_000);
    assert_eq!(game.teams[0].finance, initial_finance - cost);
    assert_eq!(game.teams[0].season_expenses, cost);
    assert_eq!(game.teams[0].financial_ledger.len(), 1);
    assert_eq!(
        game.teams[0].financial_ledger[0].kind,
        FinancialTransactionKind::FacilityUpgrade
    );
    assert_eq!(game.teams[0].financial_ledger[0].amount, -cost);
    assert_eq!(game.teams[0].facilities.as_main_facility_hub().level, 2);
    assert_eq!(game.teams[0].facilities.training, 1);
    assert!(
        game.teams[0]
            .facilities
            .can_upgrade_main_facility_module(MainFacilityModuleKind::RecoverySuite)
    );
}

#[test]
fn upgrade_main_facility_module_requires_the_next_hub_level_to_be_unlocked() {
    let mut game = make_game();

    let result = club::upgrade_main_facility_module(
        &mut game.teams[0],
        MainFacilityModuleKind::ScoutingLab,
        "2025-06-16",
    );

    assert!(result.is_err());
    assert_eq!(game.teams[0].finance, 2_000_000);
    assert_eq!(game.teams[0].facilities.scouting, 1);
}

#[test]
fn legacy_facility_upgrade_entry_point_expands_the_hub_cap_safely() {
    let mut game = make_game();

    let cost =
        club::upgrade_facility(&mut game.teams[0], FacilityType::Scouting, "2025-06-16").unwrap();

    assert_eq!(cost, 500_000);
    assert_eq!(game.teams[0].facilities.as_main_facility_hub().level, 2);
    assert_eq!(game.teams[0].facilities.scouting, 2);
}
