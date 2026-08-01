use chrono::{TimeZone, Utc};
use olm_core::academy::{
    AcademyAcquisitionOption, AcademyError, ErlAcademyCandidate, ErlAssignmentRule,
    ErlLeagueDefinition, acquire_academy, eligible_academy_acquisition_options,
    get_acquisition_options, validate_academy_acquisition, validate_parent_academy_link,
};
use olm_core::clock::GameClock;
use olm_core::domain::manager::Manager;
use olm_core::domain::team::{FinancialTransactionKind, Team, TeamKind};
use olm_core::game::Game;

fn team(id: &str, country: &str, finance: i64) -> Team {
    let mut team = Team::new(
        id.to_string(),
        id.to_string(),
        id.to_uppercase(),
        country.to_string(),
        "City".to_string(),
        "Arena".to_string(),
        10_000,
    );
    team.finance = finance;
    team
}

fn erl(
    id: &str,
    country: &str,
    region: &str,
    reputation: u8,
    nearby: &[&str],
) -> ErlLeagueDefinition {
    ErlLeagueDefinition {
        id: id.to_string(),
        name: id.to_string(),
        country_code: country.to_string(),
        region: region.to_string(),
        reputation,
        nearby_country_codes: nearby.iter().map(|country| country.to_string()).collect(),
    }
}

fn candidate(
    id: &str,
    league_id: &str,
    country: &str,
    reputation: u8,
    development_level: u8,
) -> ErlAcademyCandidate {
    ErlAcademyCandidate {
        source_team_id: id.to_string(),
        name: id.to_string(),
        short_name: id.to_uppercase(),
        logo_url: Some(format!("logos/{id}.svg")),
        erl_league_id: league_id.to_string(),
        country_code: country.to_string(),
        reputation,
        development_level,
    }
}

#[test]
fn acquisition_options_include_candidates_from_all_configured_erl_leagues() {
    let options = eligible_academy_acquisition_options(
        "FR",
        &[
            erl("lfl", "FR", "western", 5, &[]),
            erl("superliga", "ES", "western", 4, &["FR"]),
        ],
        &[
            candidate("kcb", "lfl", "FR", 5, 4),
            candidate("heretics", "superliga", "ES", 4, 3),
        ],
    );

    assert_eq!(options.len(), 2);
    assert!(options.iter().any(|option| option.source_team_id == "kcb"));
    assert!(
        options
            .iter()
            .any(|option| option.source_team_id == "heretics")
    );
}

#[test]
#[ignore = "legacy: academy ERL assignment rules changed in LoL migration (see #92)"]
fn assignment_rule_marks_domestic_vs_cross_country_candidates_in_open_pool() {
    let options = eligible_academy_acquisition_options(
        "BE",
        &[
            erl("lfl", "FR", "western", 5, &["BE"]),
            erl("superliga", "ES", "western", 4, &[]),
        ],
        &[
            candidate("kcb", "lfl", "FR", 5, 4),
            candidate("heretics", "superliga", "ES", 4, 3),
        ],
    );

    assert_eq!(options.len(), 2);
    let cross_country = options
        .iter()
        .find(|option| option.source_team_id == "kcb")
        .unwrap();
    let domestic = options
        .iter()
        .find(|option| option.source_team_id == "heretics")
        .unwrap();

    assert_eq!(cross_country.assignment_rule, ErlAssignmentRule::Fallback);
    assert_eq!(cross_country.fallback_reason, None);
    assert_eq!(domestic.assignment_rule, ErlAssignmentRule::Domestic);
}

#[test]
fn higher_reputation_candidate_costs_more_than_lower_reputation_candidate() {
    let options = eligible_academy_acquisition_options(
        "BE",
        &[
            erl("lfl", "FR", "western", 5, &["BE"]),
            erl("elite_series", "NL", "benelux", 2, &["BE"]),
        ],
        &[
            candidate("kcb", "lfl", "FR", 5, 4),
            candidate("elite-academy", "elite_series", "NL", 2, 1),
        ],
    );

    let high_reputation = options
        .iter()
        .find(|option| option.erl_league_id == "lfl")
        .unwrap();
    let low_reputation = options
        .iter()
        .find(|option| option.erl_league_id == "elite_series")
        .unwrap();

    assert!(high_reputation.acquisition_cost > low_reputation.acquisition_cost);
}

#[test]
fn insufficient_funds_blocks_academy_acquisition() {
    let parent = team("lec-team", "FR", 99_999);
    let option = AcademyAcquisitionOption {
        source_team_id: "kcb".to_string(),
        name: "Karmine Corp Blue".to_string(),
        short_name: "KCB".to_string(),
        logo_url: None,
        erl_league_id: "lfl".to_string(),
        erl_league_name: "LFL".to_string(),
        country_code: "FR".to_string(),
        assignment_rule: ErlAssignmentRule::Domestic,
        fallback_reason: None,
        reputation: 5,
        development_level: 4,
        acquisition_cost: 100_000,
    };

    assert_eq!(
        validate_academy_acquisition(&parent, &option),
        Err(AcademyError::InsufficientFunds {
            available: 99_999,
            required: 100_000,
        })
    );
}

#[test]
fn unrelated_parent_academy_movement_is_rejected() {
    let parent = team("lec-team", "FR", 1_000_000);
    let mut academy = team("other-academy", "FR", 0);
    academy.team_kind = TeamKind::Academy;
    academy.parent_team_id = Some("different-parent".to_string());

    assert_eq!(
        validate_parent_academy_link(&parent, &academy),
        Err(AcademyError::UnrelatedAcademy {
            parent_team_id: "lec-team".to_string(),
            academy_team_id: "other-academy".to_string(),
        })
    );
}

#[test]
fn academy_acquisition_records_canonical_ledger_date_and_source() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root should be above olm_core crate");
    std::env::set_current_dir(repo_root).expect("test can use repo data catalog");

    let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 3, 15, 12, 0, 0).unwrap());
    let mut manager = Manager::new(
        "manager-1".to_string(),
        "Jane".to_string(),
        "Doe".to_string(),
        "1980-01-01".to_string(),
        "FR".to_string(),
    );
    manager.hire("lec-team".to_string());
    let parent = team("lec-team", "FR", 2_000_000);
    let mut game = Game::new(clock, manager, vec![parent], vec![], vec![], vec![]);
    let (options, _) = get_acquisition_options(&game, "lec-team");
    let source_team_id = options
        .first()
        .expect("at least one academy option should be available")
        .source_team_id
        .clone();

    acquire_academy(&mut game, "lec-team", &source_team_id, None, None)
        .expect("academy acquisition succeeds");

    let parent = game
        .teams
        .iter()
        .find(|team| team.id == "lec-team")
        .expect("parent team exists");
    assert_eq!(parent.financial_ledger.len(), 1);
    let entry = &parent.financial_ledger[0];
    assert_eq!(entry.date, "2026-03-15");
    assert_eq!(entry.kind, FinancialTransactionKind::AcademyAcquisition);
    assert_eq!(entry.source, "academy");
    assert_eq!(entry.source_id.as_deref(), Some(source_team_id.as_str()));
    let expected_correlation = format!("academy-acquisition:lec-team:{source_team_id}");
    assert_eq!(
        entry.correlation_id.as_deref(),
        Some(expected_correlation.as_str())
    );
}
