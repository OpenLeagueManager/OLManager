use crate::domain::message::*;
use crate::domain::team::{
    Facilities, FinancialTransaction, FinancialTransactionKind, MainFacilityModuleKind,
    Sponsorship, SponsorshipBonusCriterion, Team, main_facility_module_catalog,
};
use crate::game::Game;
use chrono::{Datelike, NaiveDate};
use rand::RngExt;
use std::collections::{HashMap, HashSet};

const MAIN_HUB_UPKEEP_PER_EXTRA_LEVEL: i64 = 20_000;
const ESPORTS_SPONSOR_THEME_MULTIPLIER: f64 = 1.15;

pub struct FacilityUpkeepBreakdown {
    pub monthly_total: i64,
    pub hub_extra_level_total: i64,
    pub module_extra_level_total: i64,
}

struct MonthlyTeamExpenses {
    team_id: String,
    player_wages: i64,
    staff_wages: i64,
    facility_upkeep: i64,
}

enum MonthlyFinanceMailEvent {
    SponsorPayout {
        team_id: String,
        sponsor_name: String,
        amount: i64,
    },
    SponsorBonus {
        team_id: String,
        sponsor_name: String,
        bonus_amount: i64,
        total_amount: i64,
    },
    SponsorExpired {
        team_id: String,
        sponsor_name: String,
    },
    FacilityUpkeepSummary {
        team_id: String,
        amount: i64,
    },
    FacilityUpkeepSpike {
        team_id: String,
        amount: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetImpact {
    None,
    Transfer(i64),
    Wage(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceTransactionInput {
    pub date: String,
    pub description: String,
    pub amount: i64,
    pub kind: FinancialTransactionKind,
    pub budget_impact: BudgetImpact,
    pub affects_season_totals: bool,
    pub source: String,
    pub source_id: Option<String>,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinanceTransactionError {
    InvalidDate,
    BlankDescription,
    BlankSource,
    ZeroAmount,
    SignKindMismatch,
}

pub fn record_transaction(
    team: &mut Team,
    input: FinanceTransactionInput,
) -> Result<FinancialTransaction, FinanceTransactionError> {
    validate_transaction_input(&input)?;

    let balance_before = team.finance;
    let balance_after = balance_before + input.amount;
    let ledger_len = team.financial_ledger.len();
    let source_id_part = input.source_id.as_deref().unwrap_or("none");
    let id = format!(
        "tx:{}:{}:{}:{}:{}:{}",
        team.id, input.date, input.source, source_id_part, input.amount, ledger_len
    );

    if input.amount == 0 {
        return Err(FinanceTransactionError::ZeroAmount);
    }

    team.finance = balance_after;

    if input.affects_season_totals {
        if input.amount > 0 {
            team.season_income += input.amount;
        } else {
            team.season_expenses += input.amount.abs();
        }
    }

    match input.budget_impact {
        BudgetImpact::None => {}
        BudgetImpact::Transfer(delta) => team.transfer_budget += delta,
        BudgetImpact::Wage(delta) => team.wage_budget += delta,
    }

    let transaction = FinancialTransaction {
        id,
        date: input.date,
        description: input.description,
        amount: input.amount,
        kind: input.kind,
        balance_before,
        balance_after,
        source: input.source,
        source_id: input.source_id,
        correlation_id: input.correlation_id,
    };
    team.financial_ledger.push(transaction.clone());
    Ok(transaction)
}

fn validate_transaction_input(
    input: &FinanceTransactionInput,
) -> Result<(), FinanceTransactionError> {
    if NaiveDate::parse_from_str(&input.date, "%Y-%m-%d").is_err() {
        return Err(FinanceTransactionError::InvalidDate);
    }
    if input.description.trim().is_empty() {
        return Err(FinanceTransactionError::BlankDescription);
    }
    if input.source.trim().is_empty() {
        return Err(FinanceTransactionError::BlankSource);
    }
    if input.amount == 0 {
        return Err(FinanceTransactionError::ZeroAmount);
    }
    if !kind_matches_amount_sign(input.kind.clone(), input.amount) {
        return Err(FinanceTransactionError::SignKindMismatch);
    }
    Ok(())
}

fn kind_matches_amount_sign(kind: FinancialTransactionKind, amount: i64) -> bool {
    match kind {
        FinancialTransactionKind::Salary
        | FinancialTransactionKind::StaffWage
        | FinancialTransactionKind::FacilityUpkeep
        | FinancialTransactionKind::FacilityUpgrade
        | FinancialTransactionKind::TransferPurchase
        | FinancialTransactionKind::ReleasePenalty
        | FinancialTransactionKind::AcademyAcquisition => amount < 0,
        FinancialTransactionKind::TransferSale
        | FinancialTransactionKind::Sponsorship
        | FinancialTransactionKind::MatchdayRevenue
        | FinancialTransactionKind::PrizeMoney
        | FinancialTransactionKind::BudgetRefresh => amount > 0,
        FinancialTransactionKind::Other => true,
    }
}

pub fn push_finance_mail_once(
    game: &mut Game,
    team_id: &str,
    correlation_id: &str,
    event: &str,
    subject_key: &str,
    body_key: &str,
    params: HashMap<String, String>,
    date: &str,
) -> bool {
    let id = format!("finance:{correlation_id}:{event}");
    if game.messages.iter().any(|message| message.id == id) {
        return false;
    }

    let mut message = InboxMessage::new(
        id,
        "Finance update".to_string(),
        "A finance update is available.".to_string(),
        "Financial Director".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Finance)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Financial Director")
    .with_i18n(subject_key, body_key, params)
    .with_sender_i18n("be.sender.financialDirector", "be.role.financialDirector")
    .with_action(action(
        "view_finances",
        "View Finances",
        "be.msg.event.ack",
        ActionType::NavigateTo {
            route: "/dashboard?tab=Finances".to_string(),
        },
    ));
    message.context.team_id = Some(team_id.to_string());
    game.messages.push(message);
    true
}

pub fn push_sponsor_accepted_mail(
    game: &mut Game,
    team_id: &str,
    sponsor_name: &str,
    annual_amount: i64,
    date: &str,
) -> bool {
    push_finance_mail_once(
        game,
        team_id,
        &format!("sponsor-accepted:{team_id}:{sponsor_name}:{date}"),
        "sponsorAccepted",
        "be.msg.finance.sponsorAccepted.subject",
        "be.msg.finance.sponsorAccepted.body",
        params(&[
            ("sponsorName", sponsor_name.to_string()),
            ("amount", format_money(annual_amount.unsigned_abs())),
        ]),
        date,
    )
}

pub fn push_sponsor_payout_mail(
    game: &mut Game,
    team_id: &str,
    sponsor_name: &str,
    amount: i64,
    date: &str,
) -> bool {
    push_finance_mail_once(
        game,
        team_id,
        &format!("sponsor-payout:{team_id}:{date}"),
        "sponsorPayout",
        "be.msg.finance.sponsorPayout.subject",
        "be.msg.finance.sponsorPayout.body",
        params(&[
            ("sponsorName", sponsor_name.to_string()),
            ("amount", format_money(amount.unsigned_abs())),
        ]),
        date,
    )
}

pub fn push_sponsor_bonus_mail(
    game: &mut Game,
    team_id: &str,
    sponsor_name: &str,
    bonus_amount: i64,
    total_amount: i64,
    date: &str,
) -> bool {
    push_finance_mail_once(
        game,
        team_id,
        &format!("sponsor-bonus:{team_id}:{sponsor_name}:{date}"),
        "sponsorBonus",
        "be.msg.finance.sponsorBonus.subject",
        "be.msg.finance.sponsorBonus.body",
        params(&[
            ("sponsorName", sponsor_name.to_string()),
            ("bonusAmount", format_money(bonus_amount.unsigned_abs())),
            ("totalAmount", format_money(total_amount.unsigned_abs())),
        ]),
        date,
    )
}

pub fn push_sponsor_expired_mail(
    game: &mut Game,
    team_id: &str,
    sponsor_name: &str,
    date: &str,
) -> bool {
    push_finance_mail_once(
        game,
        team_id,
        &format!("sponsor-expired:{team_id}:{sponsor_name}:{date}"),
        "sponsorExpired",
        "be.msg.finance.sponsorExpired.subject",
        "be.msg.finance.sponsorExpired.body",
        params(&[("sponsorName", sponsor_name.to_string())]),
        date,
    )
}

pub fn push_facility_upkeep_summary_mail(
    game: &mut Game,
    team_id: &str,
    amount: i64,
    date: &str,
) -> bool {
    push_finance_mail_once(
        game,
        team_id,
        &format!("facility-upkeep:{team_id}:{date}"),
        "facilityUpkeepSummary",
        "be.msg.finance.facilityUpkeepSummary.subject",
        "be.msg.finance.facilityUpkeepSummary.body",
        params(&[("amount", format_money(amount.unsigned_abs()))]),
        date,
    )
}

pub fn push_facility_upkeep_spike_mail(
    game: &mut Game,
    team_id: &str,
    amount: i64,
    date: &str,
) -> bool {
    push_finance_mail_once(
        game,
        team_id,
        &format!("facility-upkeep-spike:{team_id}:{date}"),
        "facilityUpkeepSpike",
        "be.msg.finance.facilityUpkeepSpike.subject",
        "be.msg.finance.facilityUpkeepSpike.body",
        params(&[("amount", format_money(amount.unsigned_abs()))]),
        date,
    )
}

pub fn push_prize_payout_mail(
    game: &mut Game,
    team_id: &str,
    season: u32,
    position: u32,
    amount: i64,
    date: &str,
) -> bool {
    push_finance_mail_once(
        game,
        team_id,
        &format!("prize:{team_id}:{season}:{position}"),
        "prizePayout",
        "be.msg.finance.prizePayout.subject",
        "be.msg.finance.prizePayout.body",
        params(&[
            ("season", season.to_string()),
            ("position", position.to_string()),
            ("amount", format_money(amount.unsigned_abs())),
        ]),
        date,
    )
}

pub fn push_board_financial_health_mail(
    game: &mut Game,
    team_id: &str,
    balance: i64,
    monthly_net: i64,
    months_left: Option<i64>,
    date: &str,
) -> bool {
    push_finance_mail_once(
        game,
        team_id,
        &format!("board-health:{team_id}:{date}"),
        "boardFinancialHealth",
        "be.msg.finance.boardFinancialHealth.subject",
        "be.msg.finance.boardFinancialHealth.body",
        params(&[
            ("balance", format_money(balance.unsigned_abs())),
            ("monthlyNet", signed_money(monthly_net)),
            (
                "monthsLeft",
                months_left
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "stable".to_string()),
            ),
        ]),
        date,
    )
}

fn params(pairs: &[(&str, String)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

fn signed_money(amount: i64) -> String {
    let formatted = format_money(amount.unsigned_abs());
    if amount < 0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}

fn action(id: &str, label: &str, label_key: &str, action_type: ActionType) -> MessageAction {
    MessageAction {
        id: id.to_string(),
        label: label.to_string(),
        action_type,
        resolved: false,
        label_key: Some(label_key.to_string()),
    }
}

pub fn calc_annual_wages(game: &Game, team_id: &str) -> i64 {
    let player_wages: i64 = game
        .players
        .iter()
        .filter(|player| player.team_id.as_deref() == Some(team_id))
        .map(|player| player.wage as i64)
        .sum();

    let staff_wages: i64 = game
        .staff
        .iter()
        .filter(|staff_member| staff_member.team_id.as_deref() == Some(team_id))
        .map(|staff_member| staff_member.wage as i64)
        .sum();

    player_wages + staff_wages
}

pub fn calc_cash_runway_weeks(balance: i64, projected_weekly_net: i64) -> Option<i64> {
    if projected_weekly_net >= 0 {
        return None;
    }

    Some(std::cmp::max(0, balance / projected_weekly_net.abs()))
}

pub fn calc_matchday(
    stadium_capacity: u32,
    home_match_count: i64,
    attendance_pct: f64,
    avg_ticket: f64,
) -> i64 {
    let revenue_per_match = (stadium_capacity as f64 * attendance_pct * avg_ticket) as i64;

    revenue_per_match * home_match_count
}

pub fn calc_upkeep(team: &Team) -> i64 {
    facility_upkeep_breakdown(&team.facilities).monthly_total
}

pub fn facility_upkeep_breakdown(facilities: &Facilities) -> FacilityUpkeepBreakdown {
    let hub_level = facilities.as_main_facility_hub().level;
    let hub_extra_level_total =
        i64::from(hub_level.saturating_sub(1)) * MAIN_HUB_UPKEEP_PER_EXTRA_LEVEL;
    let module_extra_level_total = main_facility_module_catalog()
        .iter()
        .map(|definition| canonical_module_upkeep(facilities, definition.kind))
        .sum();

    FacilityUpkeepBreakdown {
        monthly_total: hub_extra_level_total + module_extra_level_total,
        hub_extra_level_total,
        module_extra_level_total,
    }
}

fn canonical_module_upkeep(facilities: &Facilities, module: MainFacilityModuleKind) -> i64 {
    let extra_levels = i64::from(facilities.module_level(module).saturating_sub(1));
    let per_level = match module {
        MainFacilityModuleKind::ScrimsRoom => 20_000,
        MainFacilityModuleKind::AnalysisRoom => 15_000,
        MainFacilityModuleKind::BootcampArea => 15_000,
        MainFacilityModuleKind::RecoverySuite => 10_000,
        MainFacilityModuleKind::ScoutingLab => 10_000,
        MainFacilityModuleKind::ContentStudio => 0,
    };

    extra_levels * per_level
}

pub fn facility_module_sponsorship_multiplier(facilities: &Facilities) -> f64 {
    let extra_content_levels = facilities
        .module_level(MainFacilityModuleKind::ContentStudio)
        .saturating_sub(1);

    1.0 + f64::from(extra_content_levels) * 0.02
}

fn sponsorship_theme_multiplier(sponsor_name: &str) -> f64 {
    let normalized = sponsor_name.to_lowercase();
    if normalized.contains("esport")
        || normalized.contains("gaming")
        || normalized.contains("pc")
        || normalized.contains("hardware")
        || normalized.contains("tech")
    {
        ESPORTS_SPONSOR_THEME_MULTIPLIER
    } else {
        1.0
    }
}

pub fn calc_sponsorship_income(
    current_position: Option<u32>,
    recent_form: &[String],
    sponsorship: &Sponsorship,
) -> i64 {
    let theme_multiplier = sponsorship_theme_multiplier(&sponsorship.sponsor_name);
    let base_income = (sponsorship.base_value as f64 * theme_multiplier).round() as i64;
    base_income + evaluate_sponsorship_bonus(current_position, recent_form, sponsorship)
}

pub fn evaluate_sponsorship_bonus(
    current_position: Option<u32>,
    recent_form: &[String],
    sponsorship: &Sponsorship,
) -> i64 {
    sponsorship
        .bonus_criteria
        .iter()
        .map(|criterion| match criterion {
            SponsorshipBonusCriterion::LeaguePosition {
                max_position,
                bonus_amount,
            } => {
                if current_position.is_some_and(|position| position <= *max_position) {
                    *bonus_amount
                } else {
                    0
                }
            }
            SponsorshipBonusCriterion::UnbeatenRun {
                required_matches,
                bonus_amount,
            } => {
                if recent_form.len() >= *required_matches
                    && recent_form
                        .iter()
                        .rev()
                        .take(*required_matches)
                        .all(|result| result != "L")
                {
                    *bonus_amount
                } else {
                    0
                }
            }
        })
        .sum()
}

fn current_league_position(game: &Game, team_id: &str) -> Option<u32> {
    let league = game.active_league()?;

    league
        .sorted_standings()
        .iter()
        .position(|standing| standing.team_id == team_id)
        .map(|index| index as u32 + 1)
}

fn count_recent_home_matches(game: &Game, team_id: &str) -> i64 {
    let Some(league) = game.active_league() else {
        return 0;
    };

    let current = game.clock.current_date.date_naive();
    let month_ago = current - chrono::Duration::days(28);

    league
        .fixtures
        .iter()
        .filter(|fixture| {
            fixture.status == crate::domain::league::FixtureStatus::Completed
                && fixture.home_team_id == team_id
                && fixture.result.is_some()
        })
        .filter(|fixture| {
            if let Ok(date) = chrono::NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d") {
                date > month_ago && date <= current
            } else {
                false
            }
        })
        .count() as i64
}

fn should_apply_upkeep(game: &Game) -> bool {
    game.clock.current_date.date_naive().day() == 1
}

/// Process monthly financial operations (called on the 1st of each month).
/// - Deduct player wages (monthly = annual / 12)
/// - Deduct staff wages
/// - Add matchday revenue for home matches played that month
/// - Check financial health and generate warnings
pub fn process_monthly_finances(game: &mut Game) {
    if game.clock.current_date.date_naive().day() != 1 {
        return; // Only process on the 1st of each month
    }

    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let user_team_id = game.manager.team_id.clone();
    let mut mail_events = Vec::new();
    let team_expenses: Vec<MonthlyTeamExpenses> = game
        .teams
        .iter()
        .map(|team| {
            let player_wages: i64 = game
                .players
                .iter()
                .filter(|player| player.team_id.as_deref() == Some(team.id.as_str()))
                .map(|player| player.wage as i64)
                .sum::<i64>()
                / 12;
            let staff_wages: i64 = game
                .staff
                .iter()
                .filter(|staff_member| staff_member.team_id.as_deref() == Some(team.id.as_str()))
                .map(|staff_member| staff_member.wage as i64)
                .sum::<i64>()
                / 12;
            let upkeep = if should_apply_upkeep(game) {
                calc_upkeep(team)
            } else {
                0
            };

            MonthlyTeamExpenses {
                team_id: team.id.clone(),
                player_wages,
                staff_wages,
                facility_upkeep: upkeep,
            }
        })
        .collect();
    let team_positions: Vec<(String, Option<u32>)> = game
        .teams
        .iter()
        .map(|team| (team.id.clone(), current_league_position(game, &team.id)))
        .collect();

    for team in game.teams.iter_mut() {
        let monthly_expenses = team_expenses
            .iter()
            .find(|expenses| expenses.team_id == team.id);

        if let Some(expenses) = monthly_expenses {
            record_transaction(
                team,
                FinanceTransactionInput {
                    date: today.clone(),
                    description: "Monthly player wages".to_string(),
                    amount: -expenses.player_wages,
                    kind: FinancialTransactionKind::Salary,
                    budget_impact: BudgetImpact::None,
                    affects_season_totals: true,
                    source: "monthly".to_string(),
                    source_id: Some("player-wages".to_string()),
                    correlation_id: Some(format!("monthly:{}:{}:player-wages", team.id, today)),
                },
            )
            .ok();
            record_transaction(
                team,
                FinanceTransactionInput {
                    date: today.clone(),
                    description: "Monthly staff wages".to_string(),
                    amount: -expenses.staff_wages,
                    kind: FinancialTransactionKind::StaffWage,
                    budget_impact: BudgetImpact::None,
                    affects_season_totals: true,
                    source: "monthly".to_string(),
                    source_id: Some("staff-wages".to_string()),
                    correlation_id: Some(format!("monthly:{}:{}:staff-wages", team.id, today)),
                },
            )
            .ok();
            record_transaction(
                team,
                FinanceTransactionInput {
                    date: today.clone(),
                    description: "Monthly facility upkeep".to_string(),
                    amount: -expenses.facility_upkeep,
                    kind: FinancialTransactionKind::FacilityUpkeep,
                    budget_impact: BudgetImpact::None,
                    affects_season_totals: true,
                    source: "facility".to_string(),
                    source_id: Some("monthly-upkeep".to_string()),
                    correlation_id: Some(format!("facility-upkeep:{}:{}", team.id, today)),
                },
            )
            .ok();

            if expenses.facility_upkeep > 0 && Some(team.id.as_str()) == user_team_id.as_deref() {
                mail_events.push(MonthlyFinanceMailEvent::FacilityUpkeepSummary {
                    team_id: team.id.clone(),
                    amount: expenses.facility_upkeep,
                });
                if expenses.facility_upkeep >= 100_000 {
                    mail_events.push(MonthlyFinanceMailEvent::FacilityUpkeepSpike {
                        team_id: team.id.clone(),
                        amount: expenses.facility_upkeep,
                    });
                }
            }
        }

        let current_position = team_positions
            .iter()
            .find(|(team_id, _)| team_id == &team.id)
            .and_then(|(_, position)| *position);

        let sponsorship_context = team.sponsorship.as_ref().map(|sponsorship| {
            let base_income = calc_sponsorship_income(current_position, &team.form, sponsorship);
            let bonus_income =
                evaluate_sponsorship_bonus(current_position, &team.form, sponsorship);
            let facility_mult = facility_module_sponsorship_multiplier(&team.facilities);
            // base_value is annual, divide by 12 for monthly payment
            let monthly_income = ((base_income as f64 * facility_mult) / 12.0).round() as i64;
            let monthly_bonus = ((bonus_income as f64 * facility_mult) / 12.0).round() as i64;
            (
                sponsorship.sponsor_name.clone(),
                monthly_income,
                monthly_bonus,
                sponsorship.remaining_months,
            )
        });
        let sponsorship_income = sponsorship_context
            .as_ref()
            .map(|(_, monthly_income, _, _)| *monthly_income)
            .unwrap_or(0);

        if sponsorship_income > 0 {
            record_transaction(
                team,
                FinanceTransactionInput {
                    date: today.clone(),
                    description: "Monthly sponsorship income".to_string(),
                    amount: sponsorship_income,
                    kind: FinancialTransactionKind::Sponsorship,
                    budget_impact: BudgetImpact::None,
                    affects_season_totals: true,
                    source: "sponsor".to_string(),
                    source_id: team.sponsorship.as_ref().map(|s| s.sponsor_name.clone()),
                    correlation_id: Some(format!("sponsor-payout:{}:{}", team.id, today)),
                },
            )
            .ok();
            if Some(team.id.as_str()) == user_team_id.as_deref() {
                if let Some((sponsor_name, monthly_income, monthly_bonus, _)) = &sponsorship_context
                {
                    mail_events.push(MonthlyFinanceMailEvent::SponsorPayout {
                        team_id: team.id.clone(),
                        sponsor_name: sponsor_name.clone(),
                        amount: *monthly_income,
                    });
                    if *monthly_bonus > 0 {
                        mail_events.push(MonthlyFinanceMailEvent::SponsorBonus {
                            team_id: team.id.clone(),
                            sponsor_name: sponsor_name.clone(),
                            bonus_amount: *monthly_bonus,
                            total_amount: *monthly_income,
                        });
                    }
                }
            }
        }

        if let Some(sponsorship) = team.sponsorship.as_mut() {
            let sponsor_name = sponsorship.sponsor_name.clone();
            sponsorship.remaining_months = sponsorship.remaining_months.saturating_sub(1);
            if sponsorship.remaining_months == 0 {
                team.sponsorship = None;
                if Some(team.id.as_str()) == user_team_id.as_deref() {
                    mail_events.push(MonthlyFinanceMailEvent::SponsorExpired {
                        team_id: team.id.clone(),
                        sponsor_name,
                    });
                }
            }
        }
    }

    for event in mail_events {
        match event {
            MonthlyFinanceMailEvent::SponsorPayout {
                team_id,
                sponsor_name,
                amount,
            } => {
                push_sponsor_payout_mail(game, &team_id, &sponsor_name, amount, &today);
            }
            MonthlyFinanceMailEvent::SponsorBonus {
                team_id,
                sponsor_name,
                bonus_amount,
                total_amount,
            } => {
                push_sponsor_bonus_mail(
                    game,
                    &team_id,
                    &sponsor_name,
                    bonus_amount,
                    total_amount,
                    &today,
                );
            }
            MonthlyFinanceMailEvent::SponsorExpired {
                team_id,
                sponsor_name,
            } => {
                push_sponsor_expired_mail(game, &team_id, &sponsor_name, &today);
            }
            MonthlyFinanceMailEvent::FacilityUpkeepSummary { team_id, amount } => {
                push_facility_upkeep_summary_mail(game, &team_id, amount, &today);
            }
            MonthlyFinanceMailEvent::FacilityUpkeepSpike { team_id, amount } => {
                push_facility_upkeep_spike_mail(game, &team_id, amount, &today);
            }
        }
    }

    // --- Matchday income for home matches completed in last ~28 days ---
    if !game.leagues.is_empty() {
        let home_match_counts: Vec<(String, i64)> = game
            .teams
            .iter()
            .map(|team| (team.id.clone(), count_recent_home_matches(game, &team.id)))
            .collect();

        for team in game.teams.iter_mut() {
            let home_count = home_match_counts
                .iter()
                .find(|(team_id, _)| team_id == &team.id)
                .map(|(_, count)| *count)
                .unwrap_or(0);

            if home_count > 0 {
                let mut rng = rand::rng();
                let attendance_pct = rng.random_range(15..=30) as f64 / 100.0;
                let avg_ticket = rng.random_range(4..=8) as f64;
                let total_revenue = calc_matchday(
                    team.stadium_capacity,
                    home_count,
                    attendance_pct,
                    avg_ticket,
                );

                record_transaction(
                    team,
                    FinanceTransactionInput {
                        date: today.clone(),
                        description: "Monthly matchday revenue".to_string(),
                        amount: total_revenue,
                        kind: FinancialTransactionKind::MatchdayRevenue,
                        budget_impact: BudgetImpact::None,
                        affects_season_totals: true,
                        source: "monthly".to_string(),
                        source_id: Some("matchday".to_string()),
                        correlation_id: Some(format!("matchday:{}:{}", team.id, today)),
                    },
                )
                .ok();
            }
        }
    }

    // --- Financial health warnings for user's team ---
    generate_financial_warnings(game, &today);
}

/// Generate inbox messages warning about financial issues.
fn generate_financial_warnings(game: &mut Game, today: &str) {
    let user_team_id = match &game.manager.team_id {
        Some(id) => id.clone(),
        None => return,
    };

    let team = match game.teams.iter().find(|t| t.id == user_team_id) {
        Some(t) => t,
        None => return,
    };

    let existing_ids: HashSet<String> = game.messages.iter().map(|m| m.id.clone()).collect();

    let mut new_messages: Vec<InboxMessage> = Vec::new();

    let annual_wages = calc_annual_wages(game, &user_team_id);
    let current_position = current_league_position(game, &user_team_id);
    let annual_sponsorship_income = team
        .sponsorship
        .as_ref()
        .map(|s| calc_sponsorship_income(current_position, &team.form, s))
        .unwrap_or(0);
    let _projected_annual_net = annual_sponsorship_income - annual_wages;
    let months_left = {
        let monthly_sponsor = annual_sponsorship_income / 12;
        let monthly_wages = annual_wages / 12;
        let monthly_net = monthly_sponsor - monthly_wages;
        calc_cash_runway_weeks(team.finance, monthly_net).unwrap_or(999)
    };

    // Critical: finances negative
    if team.finance < 0 {
        let msg_id = format!("finance_critical_{}", today);
        if !existing_ids.contains(&msg_id) {
            new_messages.push(
                InboxMessage::new(
                    msg_id,
                    "URGENT: Club in Debt".to_string(),
                    format!(
                        "The club is currently €{} in debt. This is an unsustainable situation.\n\n\
                        The board demands immediate action to address the financial crisis. \
                        Consider selling players, reducing staff, or finding alternative income.\n\n\
                        Failure to resolve this may have serious consequences for your position.",
                        format_money((-team.finance) as u64)
                    ),
                    "Board of Directors".to_string(),
                    today.to_string(),
                )
                .with_category(MessageCategory::Finance)
                .with_priority(MessagePriority::Urgent)
                .with_sender_role("Chairman")
                .with_i18n(
                    "be.msg.financeCritical.subject",
                    "be.msg.financeCritical.body",
                    {
                        let mut p = std::collections::HashMap::new();
                        p.insert("amount".to_string(), format_money((-team.finance) as u64));
                        p
                    },
                )
                .with_sender_i18n("be.sender.boardOfDirectors", "be.role.chairman")
                .with_action(action("view_finances", "View Finances", "be.msg.event.ack",
                    ActionType::NavigateTo { route: "/dashboard?tab=Finances".to_string() }))
            );
        }
    }
    // Warning: less than 4 months of runway
    else if (0..4).contains(&months_left) {
        let msg_id = format!("finance_warning_{}", today);
        if !existing_ids.contains(&msg_id) {
            new_messages.push(
                InboxMessage::new(
                    msg_id,
                    "Financial Warning — Low Reserves".to_string(),
                    format!(
                        "Our financial reserves are running low. At the current burn rate (€{}/year in wages), \
                        we have approximately {} months of funding remaining.\n\n\
                        I'd recommend reviewing the wage bill and exploring ways to boost income.",
                        format_money(annual_wages as u64), months_left
                    ),
                    "Financial Director".to_string(),
                    today.to_string(),
                )
                .with_category(MessageCategory::Finance)
                .with_priority(MessagePriority::High)
                .with_sender_role("Financial Director")
                .with_i18n(
                    "be.msg.financeWarning.subject",
                    "be.msg.financeWarning.body",
                    {
                        let mut p = std::collections::HashMap::new();
                        p.insert("annualWages".to_string(), format_money(annual_wages as u64));
                        p.insert("monthsLeft".to_string(), months_left.to_string());
                        p
                    },
                )
                .with_sender_i18n("be.sender.financialDirector", "be.role.financialDirector")
                .with_action(action("view_finances", "View Finances", "be.msg.event.ack",
                    ActionType::NavigateTo { route: "/dashboard?tab=Finances".to_string() }))
            );
        }
    }
    // Over budget warning: wages exceed budget
    else if annual_wages > team.wage_budget {
        let msg_id = format!("wage_over_budget_{}", today);
        if !existing_ids.contains(&msg_id) {
            new_messages.push(
                InboxMessage::new(
                    msg_id,
                    "Wage Bill Exceeds Budget".to_string(),
                    format!(
                        "Our annual wage bill (€{}) currently exceeds the allocated wage budget (€{}).\n\n\
                        While we can sustain this in the short term, the board would prefer \
                        to see the wage bill brought under control.",
                        format_money(annual_wages as u64),
                        format_money(team.wage_budget as u64)
                    ),
                    "Financial Director".to_string(),
                    today.to_string(),
                )
                .with_category(MessageCategory::Finance)
                .with_priority(MessagePriority::Normal)
                .with_sender_role("Financial Director")
                .with_i18n(
                    "be.msg.wageOverBudget.subject",
                    "be.msg.wageOverBudget.body",
                    {
                        let mut p = std::collections::HashMap::new();
                        p.insert("annualWages".to_string(), format_money(annual_wages as u64));
                        p.insert("wageBudget".to_string(), format_money(team.wage_budget as u64));
                        p
                    },
                )
                .with_sender_i18n("be.sender.financialDirector", "be.role.financialDirector")
                .with_action(action("view_finances", "View Finances", "be.msg.event.ack",
                    ActionType::NavigateTo { route: "/dashboard?tab=Finances".to_string() }))
            );
        }
    }

    game.messages.extend(new_messages);
}

fn format_money(amount: u64) -> String {
    if amount >= 1_000_000 {
        format!("{:.1}M", amount as f64 / 1_000_000.0)
    } else if amount >= 1_000 {
        format!("{}K", amount / 1_000)
    } else {
        amount.to_string()
    }
}
