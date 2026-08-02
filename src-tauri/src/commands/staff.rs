use chrono::Datelike;
use log::info;
use tauri::State;

use olm_core::domain::team::FinancialTransactionKind;
use olm_core::finances::{record_transaction, BudgetImpact, FinanceTransactionInput};
use olm_core::game::Game;
use olm_core::state::StateManager;

fn is_normal_staff_hiring_window_open(game: &Game) -> bool {
    let context = &game.season_context;
    if context.season_start.is_some() {
        return context.phase != olm_core::domain::season::SeasonPhase::InSeason;
    }

    let month = game.clock.current_date.month();
    // Fallback when season context is missing: offseason/winter->spring window.
    month <= 5 || month == 12
}

#[tauri::command]
pub fn hire_staff(state: State<'_, StateManager>, staff_id: String) -> Result<Game, String> {
    hire_staff_internal(&state, &staff_id)
}

fn hire_staff_internal(state: &StateManager, staff_id: &str) -> Result<Game, String> {
    info!("[cmd] hire_staff: staff_id={}", staff_id);
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    if !is_normal_staff_hiring_window_open(&game) {
        return Err(
            "Staff hiring is locked right now. Normal hires are only available during offseason (winter to spring)."
                .to_string(),
        );
    }

    let staff = game
        .staff
        .iter_mut()
        .find(|s| s.id == staff_id)
        .ok_or("Staff member not found".to_string())?;

    if staff.team_id.is_some() {
        return Err("Staff member already employed by a team".to_string());
    }

    staff.team_id = Some(team_id.clone());

    // Immediate hiring impact: reserve budget from real cash pool now.
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) {
        if team.finance < staff.wage as i64 {
            return Err("Insufficient club funds to hire this staff member".to_string());
        }
        record_transaction(
            team,
            FinanceTransactionInput {
                date: game.clock.current_date.format("%Y-%m-%d").to_string(),
                description: format!(
                    "Staff hiring cost: {} {}",
                    staff.first_name, staff.last_name
                ),
                amount: -(staff.wage as i64),
                kind: FinancialTransactionKind::StaffWage,
                budget_impact: BudgetImpact::None,
                affects_season_totals: true,
                source: "staff".to_string(),
                source_id: Some(staff.id.clone()),
                correlation_id: Some(format!("staff-hire:{}:{}", team_id, staff.id)),
            },
        )
        .map_err(|err| format!("Failed to record staff hiring transaction: {err:?}"))?;
    }

    state.set_game(game.clone());
    Ok(crate::client_game::game_for_client(&game))
}

#[cfg(test)]
mod tests {
    use super::{hire_staff_internal, release_staff_internal};
    use chrono::{TimeZone, Utc};
    use olm_core::clock::GameClock;
    use olm_core::domain::manager::Manager;
    use olm_core::domain::staff::{Staff, StaffAttributes, StaffRole};
    use olm_core::domain::team::{FinancialTransactionKind, Team};
    use olm_core::game::Game;
    use olm_core::state::StateManager;

    fn make_team() -> Team {
        let mut team = Team::new(
            "team-1".to_string(),
            "User FC".to_string(),
            "USR".to_string(),
            "England".to_string(),
            "London".to_string(),
            "User Ground".to_string(),
            25_000,
        );
        team.manager_id = Some("manager-1".to_string());
        team.finance = 200_000;
        team
    }

    fn make_staff() -> Staff {
        let mut staff = Staff::new(
            "staff-1".to_string(),
            "Alex".to_string(),
            "Coach".to_string(),
            "1985-01-01".to_string(),
            StaffRole::Coach,
            StaffAttributes {
                coaching: 70,
                judging_ability: 50,
                judging_potential: 50,
                physiotherapy: 30,
            },
        );
        staff.wage = 12_000;
        staff
    }

    fn make_employed_staff() -> Staff {
        let mut staff = make_staff();
        staff.team_id = Some("team-1".to_string());
        staff
    }

    fn make_game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team-1".to_string());

        Game::new(
            clock,
            manager,
            vec![make_team()],
            vec![],
            vec![make_staff()],
            vec![],
        )
    }

    fn make_game_with_employed_staff() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team-1".to_string());

        Game::new(
            clock,
            manager,
            vec![make_team()],
            vec![],
            vec![make_employed_staff()],
            vec![],
        )
    }

    #[test]
    fn hire_staff_internal_updates_state() {
        let state = StateManager::new();
        state.set_game(make_game());

        let response = hire_staff_internal(&state, "staff-1").expect("response");
        let staff = response
            .staff
            .iter()
            .find(|staff| staff.id == "staff-1")
            .unwrap();
        let team = response
            .teams
            .iter()
            .find(|team| team.id == "team-1")
            .unwrap();

        assert_eq!(staff.team_id.as_deref(), Some("team-1"));
        assert_eq!(team.finance, 188_000);
        assert_eq!(team.season_expenses, 12_000);
        assert_eq!(team.financial_ledger.len(), 1);
        assert_eq!(
            team.financial_ledger[0].kind,
            FinancialTransactionKind::StaffWage
        );
        assert_eq!(team.financial_ledger[0].amount, -12_000);

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_staff = stored_game
            .staff
            .iter()
            .find(|staff| staff.id == "staff-1")
            .expect("stored staff should exist");
        let stored_team = stored_game
            .teams
            .iter()
            .find(|team| team.id == "team-1")
            .expect("stored team should exist");
        assert_eq!(stored_staff.team_id.as_deref(), Some("team-1"));
        assert_eq!(stored_team.finance, 188_000);
        assert_eq!(stored_team.season_expenses, 12_000);
        assert_eq!(stored_team.financial_ledger.len(), 1);
        assert_eq!(
            stored_team.financial_ledger[0].kind,
            FinancialTransactionKind::StaffWage
        );
        assert_eq!(stored_team.financial_ledger[0].amount, -12_000);
    }

    #[test]
    fn hire_staff_is_blocked_outside_hiring_window() {
        let state = StateManager::new();
        let mut game = make_game();
        game.clock.current_date = Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap();
        state.set_game(game);

        let err = hire_staff_internal(&state, "staff-1").expect_err("should be blocked");
        assert!(err.contains("winter to spring"));
    }

    #[test]
    fn hire_staff_fails_when_finance_is_too_low() {
        let state = StateManager::new();
        let mut game = make_game();
        if let Some(team) = game.teams.iter_mut().find(|team| team.id == "team-1") {
            team.finance = 8_000;
        }
        state.set_game(game);

        let err = hire_staff_internal(&state, "staff-1").expect_err("should fail");
        assert_eq!(err, "Insufficient club funds to hire this staff member");
    }

    #[test]
    fn release_staff_internal_updates_state() {
        let state = StateManager::new();
        state.set_game(make_game_with_employed_staff());

        let response = release_staff_internal(&state, "staff-1").expect("response");
        let staff = response
            .staff
            .iter()
            .find(|staff| staff.id == "staff-1")
            .unwrap();

        assert!(staff.team_id.is_none());

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_staff = stored_game
            .staff
            .iter()
            .find(|staff| staff.id == "staff-1")
            .expect("stored staff should exist");
        assert!(stored_staff.team_id.is_none());
    }
}

#[tauri::command]
pub fn release_staff(state: State<'_, StateManager>, staff_id: String) -> Result<Game, String> {
    release_staff_internal(&state, &staff_id)
}

fn release_staff_internal(state: &StateManager, staff_id: &str) -> Result<Game, String> {
    info!("[cmd] release_staff: staff_id={}", staff_id);
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned".to_string())?;

    let staff = game
        .staff
        .iter_mut()
        .find(|s| s.id == staff_id)
        .ok_or("Staff member not found".to_string())?;

    if staff.team_id.as_deref() != Some(&team_id) {
        return Err("Staff member does not belong to your team".to_string());
    }

    staff.team_id = None;

    state.set_game(game.clone());
    Ok(crate::client_game::game_for_client(&game))
}
