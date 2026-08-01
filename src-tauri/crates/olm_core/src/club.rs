use crate::domain::team::{
    Facilities, FacilityType, FinancialTransactionKind, MainFacilityModuleKind, Team,
};
use crate::finances::{BudgetImpact, FinanceTransactionInput, record_transaction};

pub const BASE_FACILITY_UPGRADE_COST: i64 = 250_000;
pub const BASE_MAIN_HUB_EXPANSION_COST: i64 = 500_000;

fn facility_level(facilities: &Facilities, facility_type: &FacilityType) -> u8 {
    match facility_type {
        FacilityType::Training => facilities.training,
        FacilityType::Medical => facilities.medical,
        FacilityType::Scouting => facilities.scouting,
    }
}

pub fn next_upgrade_cost(team: &Team, facility_type: &FacilityType) -> i64 {
    i64::from(facility_level(&team.facilities, facility_type).saturating_add(1))
        * BASE_FACILITY_UPGRADE_COST
}

fn module_from_facility_type(facility_type: &FacilityType) -> MainFacilityModuleKind {
    match facility_type {
        FacilityType::Training => MainFacilityModuleKind::ScrimsRoom,
        FacilityType::Medical => MainFacilityModuleKind::RecoverySuite,
        FacilityType::Scouting => MainFacilityModuleKind::ScoutingLab,
    }
}

fn set_module_level(facilities: &mut Facilities, module: MainFacilityModuleKind, level: u8) {
    match module {
        MainFacilityModuleKind::ScrimsRoom => facilities.scrims_room_level = Some(level),
        MainFacilityModuleKind::AnalysisRoom => facilities.analysis_room_level = Some(level),
        MainFacilityModuleKind::BootcampArea => facilities.bootcamp_area_level = Some(level),
        MainFacilityModuleKind::RecoverySuite => facilities.recovery_suite_level = Some(level),
        MainFacilityModuleKind::ContentStudio => facilities.content_studio_level = Some(level),
        MainFacilityModuleKind::ScoutingLab => facilities.scouting_lab_level = Some(level),
    }

    let scrims = facilities.scrims_room_level.unwrap_or(facilities.training);
    let analysis = facilities
        .analysis_room_level
        .unwrap_or(facilities.training);
    let bootcamp = facilities.bootcamp_area_level.unwrap_or(facilities.medical);
    let recovery = facilities
        .recovery_suite_level
        .unwrap_or(facilities.medical);
    let scouting_lab = facilities.scouting_lab_level.unwrap_or(facilities.scouting);

    facilities.training = scrims.max(analysis);
    facilities.medical = bootcamp.max(recovery);
    facilities.scouting = scouting_lab;
}

pub fn next_main_hub_expansion_cost(team: &Team) -> i64 {
    i64::from(
        team.facilities
            .as_main_facility_hub()
            .level
            .saturating_add(1),
    ) * BASE_MAIN_HUB_EXPANSION_COST
}

pub fn expand_main_facility_hub(team: &mut Team, date: &str) -> Result<i64, String> {
    let cost = next_main_hub_expansion_cost(team);
    if team.finance < cost {
        return Err(format!(
            "Insufficient funds for main facility expansion: need €{}",
            cost
        ));
    }

    record_transaction(
        team,
        FinanceTransactionInput {
            date: date.to_string(),
            description: "Main facility hub expansion".to_string(),
            amount: -cost,
            kind: FinancialTransactionKind::FacilityUpgrade,
            budget_impact: BudgetImpact::None,
            affects_season_totals: true,
            source: "facility".to_string(),
            source_id: Some("main-hub".to_string()),
            correlation_id: Some(format!("facility:{}:{date}:main-hub", team.id)),
        },
    )
    .map_err(|err| format!("Failed to record facility expansion: {err:?}"))?;
    team.facilities.main_hub_level = team
        .facilities
        .as_main_facility_hub()
        .level
        .saturating_add(1);

    Ok(cost)
}

pub fn upgrade_main_facility_module(
    team: &mut Team,
    module: MainFacilityModuleKind,
    date: &str,
) -> Result<i64, String> {
    if !team.facilities.can_upgrade_main_facility_module(module) {
        return Err("Main facility hub must be expanded before upgrading this module".to_string());
    }

    let current_level = team.facilities.module_level(module);
    let cost = i64::from(current_level + 1) * BASE_FACILITY_UPGRADE_COST;
    if team.finance < cost {
        return Err(format!(
            "Insufficient funds for facility module upgrade: need €{}",
            cost
        ));
    }

    record_transaction(
        team,
        FinanceTransactionInput {
            date: date.to_string(),
            description: format!("Facility module upgrade: {module:?}"),
            amount: -cost,
            kind: FinancialTransactionKind::FacilityUpgrade,
            budget_impact: BudgetImpact::None,
            affects_season_totals: true,
            source: "facility".to_string(),
            source_id: Some(format!("{module:?}")),
            correlation_id: Some(format!("facility:{}:{date}:{module:?}", team.id)),
        },
    )
    .map_err(|err| format!("Failed to record facility module upgrade: {err:?}"))?;
    set_module_level(
        &mut team.facilities,
        module,
        current_level.saturating_add(1),
    );

    Ok(cost)
}

pub fn upgrade_facility(
    team: &mut Team,
    facility_type: FacilityType,
    date: &str,
) -> Result<i64, String> {
    let cost = next_upgrade_cost(team, &facility_type);
    if team.finance < cost {
        return Err(format!(
            "Insufficient funds for facility upgrade: need €{}",
            cost
        ));
    }

    let module = module_from_facility_type(&facility_type);
    let target_level = team.facilities.module_level(module).saturating_add(1);
    if target_level > team.facilities.as_main_facility_hub().level {
        team.facilities.main_hub_level = target_level;
    }

    record_transaction(
        team,
        FinanceTransactionInput {
            date: date.to_string(),
            description: format!("Facility upgrade: {facility_type:?}"),
            amount: -cost,
            kind: FinancialTransactionKind::FacilityUpgrade,
            budget_impact: BudgetImpact::None,
            affects_season_totals: true,
            source: "facility".to_string(),
            source_id: Some(format!("{facility_type:?}")),
            correlation_id: Some(format!("facility:{}:{date}:{facility_type:?}", team.id)),
        },
    )
    .map_err(|err| format!("Failed to record facility upgrade: {err:?}"))?;

    match facility_type {
        FacilityType::Training => {
            let next_level = team.facilities.training.saturating_add(1);
            team.facilities.training = next_level;
            team.facilities.scrims_room_level = Some(next_level);
            team.facilities.analysis_room_level = Some(next_level);
        }
        FacilityType::Medical => {
            let next_level = team.facilities.medical.saturating_add(1);
            team.facilities.medical = next_level;
            team.facilities.bootcamp_area_level = Some(next_level);
            team.facilities.recovery_suite_level = Some(next_level);
        }
        FacilityType::Scouting => {
            let next_level = team.facilities.scouting.saturating_add(1);
            team.facilities.scouting = next_level;
            team.facilities.scouting_lab_level = Some(next_level);
        }
    }

    Ok(cost)
}
