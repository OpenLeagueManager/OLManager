use crate::domain::player::{Player, PlayerTrait};
use crate::domain::team::{Team, TeamKind};
use crate::game::Game;
use chrono::NaiveDate;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Satisfaction scoring weights (spec PA-02)
// ---------------------------------------------------------------------------
const W_MORALE: f64 = 0.40;
const W_MANAGER_TRUST: f64 = 0.30;
const W_WAGE_SATISFACTION: f64 = 0.15;
const W_AMBITION_ALIGNMENT: f64 = 0.10;
const W_LOYALTY: f64 = 0.05;

// ---------------------------------------------------------------------------
// Thresholds
// ---------------------------------------------------------------------------

/// Satisfaction below this → transfer request.
const SATISFACTION_LOW: f64 = 0.35;

/// Satisfaction at or above this → completely happy (Silent).
const SATISFACTION_HIGH: f64 = 0.70;

/// Satisfaction below this on a contender → truly miserable threshold.
/// Between [0.20, SATISFACTION_LOW) the contender suppresses transfer requests.
const TRULY_MISERABLE: f64 = 0.20;

/// Contract months remaining below this → renewal demand trigger.
const RENEWAL_CONTRACT_MONTHS: f64 = 6.0;

/// Wage / market_value ratio at or above this → well paid.
const WAGE_FAIR_RATIO: f64 = 0.8;

/// Wage / market_value ratio below this → significantly underpaid.
const WAGE_UNDERPAID_RATIO: f64 = 0.7;

/// Max teams processed per day (mirrors ai_team_agent::TEAMS_PER_DAY).
const TEAMS_PER_DAY: usize = 3;

// ---------------------------------------------------------------------------
// Decision enum
// ---------------------------------------------------------------------------

/// Outcome of a player agent evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerAgentDecision {
    /// Player wants to leave — set transfer_listed = true.
    RequestTransfer,
    /// Player wants to negotiate a new contract — set renewal_state.
    RequestRenewal,
    /// Player is content — no action.
    Silent,
}

// ---------------------------------------------------------------------------
// Internal ambition enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum PlayerAmbition {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TeamAmbition {
    High,   // TitleContender
    Medium, // PlayoffContender / MidTable
    Low,    // Survival
}

// ---------------------------------------------------------------------------
// Helper: contract months remaining
// ---------------------------------------------------------------------------

fn months_remaining(contract_end: &Option<String>, today: NaiveDate) -> f64 {
    match contract_end.as_deref() {
        Some(end_str) => {
            if let Ok(end) = NaiveDate::parse_from_str(end_str, "%Y-%m-%d") {
                (end.signed_duration_since(today).num_days() as f64 / 30.44).max(0.0)
            } else {
                0.0
            }
        }
        None => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Satisfaction components
// ---------------------------------------------------------------------------

/// Wage satisfaction: how happy the player is with their wage vs market value.
fn wage_satisfaction(player: &Player) -> f64 {
    if player.market_value == 0 {
        return 0.5; // neutral when no market data
    }
    let ratio = f64::from(player.wage) / (player.market_value as f64);
    if ratio >= WAGE_FAIR_RATIO {
        1.0
    } else if ratio >= WAGE_UNDERPAID_RATIO {
        0.5
    } else {
        0.2
    }
}

/// Derive player's personal ambition from traits and OVR.
fn derive_player_ambition(player: &Player) -> PlayerAmbition {
    let has_high_ambition_trait = player
        .traits
        .iter()
        .any(|t| matches!(t, PlayerTrait::HyperCarry | PlayerTrait::Intimidator));
    if has_high_ambition_trait {
        return PlayerAmbition::High;
    }

    if player.lol_ovr >= 75 {
        PlayerAmbition::High
    } else if player.lol_ovr >= 60 {
        PlayerAmbition::Medium
    } else {
        PlayerAmbition::Low
    }
}

/// Derive team ambition from roster strength (top-5 avg OVR) using a players slice.
/// Uses the same approach as `board_objectives::team_strength_score`.
fn derive_team_ambition_from_players(team: &Team, players: &[Player]) -> TeamAmbition {
    let mut player_ovrs: Vec<f64> = players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(&team.id))
        .map(|p| crate::player_rating::natural_ovr(p))
        .collect();

    if player_ovrs.is_empty() {
        return if team.reputation >= 75 {
            TeamAmbition::High
        } else if team.reputation >= 50 {
            TeamAmbition::Medium
        } else {
            TeamAmbition::Low
        };
    }

    player_ovrs.sort_by(|a, b| b.total_cmp(a));
    let count = player_ovrs.len().min(5) as f64;
    let avg_strength: f64 = player_ovrs.iter().take(5).sum::<f64>() / count;

    if avg_strength >= 78.0 {
        TeamAmbition::High
    } else if avg_strength >= 68.0 {
        TeamAmbition::Medium
    } else {
        TeamAmbition::Low
    }
}

/// Derive team ambition from roster strength (convenience wrapper passing game.players).
fn derive_team_ambition(team: &Team, game: &Game) -> TeamAmbition {
    derive_team_ambition_from_players(team, &game.players)
}

/// Ambition alignment: how well player ambition matches team trajectory.
/// High-ambition trait on low-ambition team → penalty.
fn ambition_alignment(player: &Player, team_ambition: TeamAmbition) -> f64 {
    let player_ambition = derive_player_ambition(player);

    match (player_ambition, team_ambition) {
        (PlayerAmbition::High, TeamAmbition::High) => 1.0,
        (PlayerAmbition::High, TeamAmbition::Medium) => 0.6,
        (PlayerAmbition::High, TeamAmbition::Low) => 0.2,
        (PlayerAmbition::Medium, TeamAmbition::High) => 0.7,
        (PlayerAmbition::Medium, TeamAmbition::Medium) => 1.0,
        (PlayerAmbition::Medium, TeamAmbition::Low) => 0.6,
        (PlayerAmbition::Low, _) => 0.8,
    }
}

/// Loyalty factor: career tenure at current club + trait bonus.
/// TeamPlayer, Sentinel, IceCold traits add loyalty.
fn loyalty_factor(player: &Player) -> f64 {
    let team_career_count = player
        .career
        .iter()
        .filter(|entry| entry.team_id.as_deref() == player.team_id.as_deref())
        .count();

    let tenure_bonus = (team_career_count as f64 / 5.0).min(1.0);

    let trait_bonus = if player.traits.iter().any(|t| {
        matches!(
            t,
            PlayerTrait::TeamPlayer | PlayerTrait::Sentinel | PlayerTrait::IceCold
        )
    }) {
        0.2
    } else {
        0.0
    };

    (tenure_bonus + trait_bonus).min(1.0)
}

// ---------------------------------------------------------------------------
// Composite satisfaction score
// ---------------------------------------------------------------------------

/// Compute player satisfaction (0.0 = miserable, 1.0 = ecstatic).
///
/// Weights per spec PA-02:
/// - morale × 0.40
/// - manager_trust × 0.30
/// - wage_satisfaction × 0.15
/// - ambition_alignment × 0.10
/// - loyalty × 0.05
pub fn compute_satisfaction(player: &Player, team: &Team, game: &Game) -> f64 {
    let team_ambition = derive_team_ambition(team, game);
    compute_satisfaction_inner(player, team_ambition)
}

/// Inner satisfaction computation that avoids a &Game dependency.
fn compute_satisfaction_inner(player: &Player, team_ambition: TeamAmbition) -> f64 {
    let morale = f64::from(player.morale) / 100.0;
    let manager_trust = f64::from(player.morale_core.manager_trust) / 100.0;
    let wage_sat = wage_satisfaction(player);
    let ambition = ambition_alignment(player, team_ambition);
    let loyalty = loyalty_factor(player);

    morale * W_MORALE
        + manager_trust * W_MANAGER_TRUST
        + wage_sat * W_WAGE_SATISFACTION
        + ambition * W_AMBITION_ALIGNMENT
        + loyalty * W_LOYALTY
}

// ---------------------------------------------------------------------------
// Decision logic
// ---------------------------------------------------------------------------

/// Determine whether a player requests a transfer, demands renewal, or stays silent.
///
/// Thresholds:
/// - satisfaction < 0.35 → transfer request (if contract short or underpaid)
/// - 0.35 ≤ satisfaction < 0.70 + contract < 6 months → renewal demand
/// - satisfaction ≥ 0.70 → silent
///
/// PA-03: Contender teams suppress transfer requests unless truly miserable (< 0.20).
pub fn decide_action(
    satisfaction: f64,
    player: &Player,
    team: &Team,
    game: &Game,
) -> PlayerAgentDecision {
    let today = game.clock.current_date.date_naive();
    let team_ambition = derive_team_ambition(team, game);
    decide_action_inner(satisfaction, player, team_ambition, today)
}

/// Inner decision that avoids a &Game dependency (needs pre-computed params).
fn decide_action_inner(
    satisfaction: f64,
    player: &Player,
    team_ambition: TeamAmbition,
    today: NaiveDate,
) -> PlayerAgentDecision {
    let is_contender = matches!(team_ambition, TeamAmbition::High);
    let contract_short = months_remaining(&player.contract_end, today) < RENEWAL_CONTRACT_MONTHS;
    let underpaid = player.market_value > 0
        && f64::from(player.wage) / (player.market_value as f64) < WAGE_UNDERPAID_RATIO;

    // --- Very low satisfaction → transfer request (PA-04) ---
    if satisfaction < SATISFACTION_LOW {
        // PA-03: Contender teams suppress unless truly miserable
        if is_contender && satisfaction >= TRULY_MISERABLE {
            if contract_short {
                return PlayerAgentDecision::RequestRenewal;
            }
            return PlayerAgentDecision::Silent;
        }

        if contract_short || underpaid {
            return PlayerAgentDecision::RequestTransfer;
        }
    }

    // --- Mid-range satisfaction + short contract → renewal demand (PA-05) ---
    if satisfaction < SATISFACTION_HIGH && contract_short {
        return PlayerAgentDecision::RequestRenewal;
    }

    // --- High satisfaction or no trigger → stay (PA-02) ---
    PlayerAgentDecision::Silent
}

/// Route a player agent decision to mutate game state.
///
/// PA-06: NEVER modifies roster directly — only sets flags.
pub fn route_decision(decision: &PlayerAgentDecision, player: &mut Player) {
    match decision {
        PlayerAgentDecision::RequestTransfer => {
            player.transfer_listed = true;
        }
        PlayerAgentDecision::RequestRenewal => {
            use crate::domain::player::ContractRenewalState;
            if player.morale_core.renewal_state.is_none() {
                player.morale_core.renewal_state = Some(ContractRenewalState {
                    status: crate::domain::player::RenewalSessionStatus::Open,
                    ..ContractRenewalState::default()
                });
            }
        }
        PlayerAgentDecision::Silent => {}
    }
}

// ---------------------------------------------------------------------------
// Orchestration — public entry point called from turn/mod.rs
// ---------------------------------------------------------------------------

/// Select teams that were processed by Team Agent yesterday (day offset - 1).
fn select_yesterday_ai_teams(game: &Game) -> Vec<String> {
    let ai_teams: Vec<&Team> = game
        .teams
        .iter()
        .filter(|t| t.team_kind == TeamKind::Main && t.manager_id.is_none())
        .collect();

    if ai_teams.is_empty() {
        return Vec::new();
    }

    // Use day-of-year - 1 to get yesterday's Team Agent batch
    let day_of_year = game
        .clock
        .current_date
        .format("%j")
        .to_string()
        .parse::<usize>()
        .unwrap_or(0);

    let count = TEAMS_PER_DAY.min(ai_teams.len());
    // Yesterday's offset: if day > 0, use day-1; otherwise 0 (first day)
    let start = if day_of_year > 0 {
        (day_of_year - 1) % ai_teams.len()
    } else {
        0
    };

    (0..count)
        .map(|i| {
            let idx = (start + i) % ai_teams.len();
            ai_teams[idx].id.clone()
        })
        .collect()
}

/// Process player agents for teams that were processed by Team Agent yesterday.
///
/// For each player on those teams:
/// 1. Compute satisfaction
/// 2. Decide action (stay / renewal demand / transfer request)
/// 3. Route the decision (set flags only — PA-06)
pub fn process_ai_player_agents(game: &mut Game) {
    let today = game.clock.current_date.date_naive();
    let team_ids = select_yesterday_ai_teams(game);

    if team_ids.is_empty() {
        return;
    }

    // Pre-compute team ambitions (needs read from game.players) before
    // entering the mutable borrow loop.
    let team_ambitions: HashMap<String, TeamAmbition> = {
        let players = &game.players;
        team_ids
            .iter()
            .filter_map(|team_id| {
                game.teams.iter().find(|t| t.id == *team_id).map(|team| {
                    (
                        team_id.clone(),
                        derive_team_ambition_from_players(team, players),
                    )
                })
            })
            .collect()
    };

    for team_id in &team_ids {
        let team_ambition = team_ambitions
            .get(team_id)
            .copied()
            .unwrap_or(TeamAmbition::Low);

        for player in game.players.iter_mut() {
            if player.team_id.as_deref() != Some(team_id) {
                continue;
            }

            let satisfaction = compute_satisfaction_inner(player, team_ambition);
            let decision = decide_action_inner(satisfaction, player, team_ambition, today);
            route_decision(&decision, player);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::domain::manager::Manager;
    use crate::domain::player::{LolRole, Player, PlayerAttributes, PlayerMoraleCore};
    use crate::domain::team::Team;
    use chrono::{TimeZone, Utc};

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn default_attrs() -> PlayerAttributes {
        PlayerAttributes {
            mechanics: 70,
            laning: 70,
            teamfighting: 70,
            macro_play: 70,
            consistency: 70,
            shotcalling: 70,
            champion_pool: 70,
            discipline: 70,
            mental_resilience: 70,
        }
    }

    fn make_player(
        id: &str,
        name: &str,
        team_id: &str,
        role: LolRole,
        lol_ovr: u8,
        morale: u8,
        manager_trust: u8,
        wage: u32,
        market_value: u64,
        contract_end: Option<&str>,
        traits: Vec<PlayerTrait>,
    ) -> Player {
        let mut player = Player::new(
            id.to_string(),
            name.to_string(),
            format!("Full {name}"),
            "2000-01-01".to_string(),
            "GB".to_string(),
            role,
            default_attrs(),
        );
        player.team_id = Some(team_id.to_string());
        player.lol_ovr = lol_ovr;
        player.morale = morale;
        player.morale_core = PlayerMoraleCore {
            manager_trust,
            ..PlayerMoraleCore::default()
        };
        player.wage = wage;
        player.market_value = market_value;
        player.contract_end = contract_end.map(|s| s.to_string());
        player.traits = traits;
        player
    }

    fn make_team(id: &str, name: &str, reputation: u32) -> Team {
        let mut team = Team::new(
            id.to_string(),
            name.to_string(),
            name[..3].to_string(),
            "DE".to_string(),
            "Berlin".to_string(),
            "Arena".to_string(),
            10_000,
        );
        team.reputation = reputation;
        team
    }

    #[allow(dead_code)]
    fn test_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()
    }

    /// Build a minimal Game with one AI team and one player.
    fn make_game_with_single_ai_team(team: Team, players: Vec<Player>) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap());
        let manager = Manager::new(
            "mgr1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "GB".to_string(),
        );
        Game::new(clock, manager, vec![team], players, vec![], vec![])
    }

    // -----------------------------------------------------------------------
    // Phase 1: Satisfaction scoring
    // -----------------------------------------------------------------------

    #[test]
    fn test_compute_satisfaction_happy_player() {
        // Spec PA-02 "Stay at contender":
        // GIVEN morale > 70, manager_trust > 60, wage >= market_value × 0.8
        // THEN player MUST NOT request transfer (satisfaction ≥ 0.70)
        let team = make_team("t1", "Test Team", 80);
        let player = make_player(
            "p1",
            "Happy",
            "t1",
            LolRole::Mid,
            85,                 // lol_ovr
            85,                 // morale > 70
            70,                 // manager_trust > 60
            80_000,             // wage
            100_000,            // market_value → wage / mv = 0.8 → WAGE_FAIR_RATIO
            Some("2028-06-15"), // long contract
            vec![PlayerTrait::TeamPlayer],
        );

        let game = make_game_with_single_ai_team(team.clone(), vec![player]);
        let p = game.players.iter().find(|p| p.id == "p1").unwrap();
        let t = game.teams.iter().find(|t| t.id == "t1").unwrap();

        let satisfaction = compute_satisfaction(p, t, &game);
        // Expected: high morale (0.85*0.40=0.34) + high trust (0.70*0.30=0.21)
        // + good wage (1.0*0.15=0.15) + ambition alignment (ambitious on strong team = 1.0*0.10=0.10)
        // + loyalty (TeamPlayer trait bonus = (0+0.2)*0.05 = 0.01)
        // Total ≈ 0.34 + 0.21 + 0.15 + 0.10 + 0.01 = 0.81
        assert!(
            satisfaction >= 0.70,
            "happy player on contender should have high satisfaction, got {satisfaction}"
        );
    }

    #[test]
    fn test_compute_satisfaction_unhappy_player() {
        // Spec PA-04 "Leave unhappy team":
        // GIVEN morale < 30, manager_trust < 25 → low satisfaction
        let team = make_team("t2", "Low Team", 40);
        let player = make_player(
            "p2",
            "Unhappy",
            "t2",
            LolRole::Top,
            55,                 // low OVR → low ambition
            20,                 // morale < 30
            15,                 // manager_trust < 25
            20_000,             // wage
            80_000,             // market_value → wage / mv = 0.25 < 0.7 → underpaid
            Some("2026-09-15"), // short contract
            vec![],
        );

        let game = make_game_with_single_ai_team(team.clone(), vec![player]);
        let p = game.players.iter().find(|p| p.id == "p2").unwrap();
        let t = game.teams.iter().find(|t| t.id == "t2").unwrap();

        let satisfaction = compute_satisfaction(p, t, &game);
        // Expected: low morale (0.20*0.40=0.08) + low trust (0.15*0.30=0.045)
        // + low wage_satisfaction (0.2*0.15=0.03) + ambition alignment (low ambition on low team = 0.8*0.10=0.08)
        // + loyalty (0.0*0.05=0.0)
        // Total ≈ 0.08 + 0.045 + 0.03 + 0.08 + 0.0 = 0.235
        assert!(
            satisfaction < 0.35,
            "unhappy player should have low satisfaction, got {satisfaction}"
        );
    }

    #[test]
    fn test_ambition_alignment_hypercarry_on_survival_penalty() {
        // Player with HyperCarry trait on a low-ambition team → alignment penalty.
        // Add filler players with low attrs so natural_ovr avg < 68 → Low ambition.
        let team = make_team("t3", "Survival Team", 30);

        // Star player: high OVR attrs but team's avg is pulled down by filler
        let star = make_player(
            "p3",
            "Star",
            "t3",
            LolRole::Mid,
            90,
            50,
            40,
            50_000,
            200_000,
            Some("2027-06-15"),
            vec![PlayerTrait::HyperCarry],
        );

        // Filler players with low attrs to bring avg below 68
        let filler_attrs = PlayerAttributes {
            mechanics: 40,
            laning: 40,
            teamfighting: 40,
            macro_play: 40,
            consistency: 40,
            shotcalling: 40,
            champion_pool: 40,
            discipline: 40,
            mental_resilience: 40,
        };
        let mut filler1 = Player::new(
            "f1".to_string(),
            "Filler1".to_string(),
            "Full Filler1".to_string(),
            "2000-01-01".to_string(),
            "GB".to_string(),
            LolRole::Top,
            filler_attrs.clone(),
        );
        filler1.team_id = Some("t3".to_string());
        let mut filler2 = filler1.clone();
        filler2.id = "f2".to_string();
        filler2.match_name = "Filler2".to_string();
        filler2.natural_position = LolRole::Jungle;
        let mut filler3 = filler1.clone();
        filler3.id = "f3".to_string();
        filler3.match_name = "Filler3".to_string();
        filler3.natural_position = LolRole::Adc;
        let mut filler4 = filler1.clone();
        filler4.id = "f4".to_string();
        filler4.match_name = "Filler4".to_string();
        filler4.natural_position = LolRole::Support;

        let all_players = vec![star, filler1, filler2, filler3, filler4];
        // avg natural_ovr = (approx 90 + 40+40+40+40) / 5 = 50 → Low

        let game = make_game_with_single_ai_team(team, all_players);
        let t = game.teams.iter().find(|t| t.id == "t3").unwrap();
        let team_ambition = derive_team_ambition(t, &game);

        assert_eq!(
            team_ambition,
            TeamAmbition::Low,
            "team with avg OVR ~50 should be Low ambition, got {:?}",
            team_ambition
        );

        let p = game.players.iter().find(|p| p.id == "p3").unwrap();
        let alignment = ambition_alignment(p, team_ambition);
        // HyperCarry (High Ambition) on Survival (Low Ambition) → 0.2
        assert!(
            (alignment - 0.2).abs() < 0.001,
            "HyperCarry on Survival should get 0.2 alignment, got {alignment}"
        );
    }

    #[test]
    fn test_loyalty_factor_long_tenure() {
        use crate::domain::player::CareerEntry;

        let player = make_player(
            "p4",
            "Loyal",
            "t4",
            LolRole::Support,
            70,
            50,
            50,
            50_000,
            100_000,
            Some("2027-06-15"),
            vec![],
        );

        // Add career entries matching current team (5 seasons → full tenure_bonus)
        let mut loyal_player = player.clone();
        for season in 1..=5 {
            loyal_player.career.push(CareerEntry {
                season,
                team_id: Some("t4".to_string()),
                team_name: "Test Team".to_string(),
                appearances: 30,
                kills: 0,
                deaths: 0,
                assists: 0,
                avg_rating: 7.0,
            });
        }

        let loyalty = loyalty_factor(&loyal_player);
        // 5 seasons → 1.0 tenure, no trait bonus → total 1.0 (capped)
        assert!(
            (loyalty - 1.0).abs() < 0.001,
            "5-season tenure should give max loyalty, got {loyalty}"
        );
    }

    #[test]
    fn test_loyalty_factor_sentinel_trait_bonus() {
        // Player with Sentinel trait gets +0.2 bonus
        let player = make_player(
            "p5",
            "Sentinel",
            "t5",
            LolRole::Top,
            70,
            50,
            50,
            50_000,
            100_000,
            Some("2027-06-15"),
            vec![PlayerTrait::Sentinel],
        );

        // No career entries yet → tenure is 0.0 + trait 0.2 = 0.2
        let loyalty = loyalty_factor(&player);
        assert!(
            (loyalty - 0.2).abs() < 0.001,
            "Sentinel trait should give 0.2 loyalty, got {loyalty}"
        );
    }

    // -----------------------------------------------------------------------
    // Phase 2: Stay/leave/negotiate decisions
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_action_low_satisfaction_transfer_request() {
        // Spec PA-05: low satisfaction + short contract + underpaid → transfer request
        let team = make_team("t1", "Weak Team", 30);
        let player = make_player(
            "p1",
            "Leaver",
            "t1",
            LolRole::Mid,
            60,                 // lol_ovr
            25,                 // morale
            20,                 // manager_trust
            20_000,             // wage
            100_000,            // market_value → wage/mv = 0.2 → underpaid
            Some("2026-09-01"), // < 6 months from test_date
            vec![],
        );

        let game = make_game_with_single_ai_team(team.clone(), vec![player]);
        let p = game.players.iter().find(|p| p.id == "p1").unwrap();
        let t = game.teams.iter().find(|t| t.id == "t1").unwrap();
        let satisfaction = compute_satisfaction(p, t, &game);

        let decision = decide_action(satisfaction, p, t, &game);
        assert_eq!(
            decision,
            PlayerAgentDecision::RequestTransfer,
            "unhappy underpaid player with short contract should request transfer, satisfaction={satisfaction}"
        );
    }

    #[test]
    fn test_decide_action_contender_suppresses_transfer() {
        // PA-03: Player on TitleContender team should NOT request transfer
        // even if somewhat dissatisfied (but not miserable).
        // Setup: moderately unhappy player on a contender team with high avg OVR.
        let mut team = make_team("t1", "Top Team", 90);
        team.manager_id = None;
        team.team_kind = TeamKind::Main;

        // Use high attrs so natural_ovr >= 78 → High ambition
        let high_attrs = PlayerAttributes {
            mechanics: 90,
            laning: 90,
            teamfighting: 90,
            macro_play: 90,
            consistency: 90,
            shotcalling: 90,
            champion_pool: 90,
            discipline: 90,
            mental_resilience: 90,
        };

        fn make_high_ovr_player(
            id: &str,
            team_id: &str,
            role: LolRole,
            morale: u8,
            trust: u8,
            attrs: PlayerAttributes,
        ) -> Player {
            let mut p = Player::new(
                id.to_string(),
                format!("Player {id}"),
                format!("Full {id}"),
                "2000-01-01".to_string(),
                "GB".to_string(),
                role,
                attrs,
            );
            p.team_id = Some(team_id.to_string());
            p.lol_ovr = 90;
            p.morale = morale;
            p.morale_core = PlayerMoraleCore {
                manager_trust: trust,
                ..PlayerMoraleCore::default()
            };
            p.wage = 80_000;
            p.market_value = 100_000;
            p.contract_end = Some("2026-09-01".to_string());
            p
        }

        let players = vec![
            // p1: very unhappy (morale 20, trust 15) + underpaid (20K/100K) → satisfaction << 0.35
            {
                let mut p =
                    make_high_ovr_player("p1", "t1", LolRole::Top, 20, 15, high_attrs.clone());
                p.wage = 20_000; // very underpaid
                p.traits = vec![PlayerTrait::HyperCarry];
                p
            },
            make_high_ovr_player("p2", "t1", LolRole::Jungle, 50, 50, high_attrs.clone()),
            make_high_ovr_player("p3", "t1", LolRole::Mid, 50, 50, high_attrs.clone()),
            make_high_ovr_player("p4", "t1", LolRole::Adc, 50, 50, high_attrs.clone()),
            make_high_ovr_player("p5", "t1", LolRole::Support, 50, 50, high_attrs.clone()),
        ];

        let game = make_game_with_single_ai_team(team, players);

        // Check team ambition (natural_ovr = 90 for all → High)
        let t = game.teams.iter().find(|t| t.id == "t1").unwrap();
        let ambition = derive_team_ambition(t, &game);
        assert_eq!(
            ambition,
            TeamAmbition::High,
            "team with strong roster should be High ambition, got {:?}",
            ambition
        );

        // p1 is very unhappy (morale 20, trust 15) + underpaid + HyperCarry
        // on a contender → should be suppressed (not RequestTransfer)
        let p1 = game.players.iter().find(|p| p.id == "p1").unwrap();
        let satisfaction = compute_satisfaction(p1, t, &game);
        let decision = decide_action(satisfaction, p1, t, &game);

        // The satisfaction should be < 0.35 (unhappy) but as a contender,
        // the transfer should be suppressed
        assert!(
            satisfaction < 0.35,
            "p1 should be dissatisfied, got {satisfaction}"
        );
        assert_ne!(
            decision,
            PlayerAgentDecision::RequestTransfer,
            "contender team should suppress transfer request, satisfaction={satisfaction}"
        );
    }

    #[test]
    fn test_decide_action_pa06_no_direct_roster_modification() {
        // PA-06: Player agent must NOT modify roster directly,
        // only set flags. Verify route_decision only changes
        // transfer_listed or renewal_state, never team_id or other roster fields.
        let _team = make_team("t1", "Test Team", 50);
        let player = make_player(
            "p1",
            "Test",
            "t1",
            LolRole::Mid,
            70,
            50,
            50,
            50_000,
            100_000,
            Some("2026-09-01"),
            vec![],
        );

        let mut test_player = player.clone();
        let original_team_id = test_player.team_id.clone();
        let original_wage = test_player.wage;
        let original_lol_ovr = test_player.lol_ovr;

        // Apply each decision type and verify only flags changed
        route_decision(&PlayerAgentDecision::RequestTransfer, &mut test_player);
        assert!(
            test_player.transfer_listed,
            "transfer_listed should be true"
        );
        assert_eq!(
            test_player.team_id, original_team_id,
            "team_id must not change"
        );
        assert_eq!(test_player.wage, original_wage, "wage must not change");
        assert_eq!(
            test_player.lol_ovr, original_lol_ovr,
            "lol_ovr must not change"
        );

        // Reset and test renewal
        let mut test_player2 = player.clone();
        let original_team_id2 = test_player2.team_id.clone();
        route_decision(&PlayerAgentDecision::RequestRenewal, &mut test_player2);
        assert!(
            test_player2.morale_core.renewal_state.is_some(),
            "renewal_state should be set"
        );
        assert_eq!(
            test_player2.team_id, original_team_id2,
            "team_id must not change"
        );

        // Reset and test silent
        let mut test_player3 = player.clone();
        route_decision(&PlayerAgentDecision::Silent, &mut test_player3);
        assert!(
            !test_player3.transfer_listed,
            "transfer_listed should remain false"
        );
        assert_eq!(
            test_player3.team_id, original_team_id2,
            "team_id must not change"
        );
    }

    #[test]
    fn test_decide_action_high_satisfaction_silent() {
        // Happy player → Silent
        let team = make_team("t1", "Great Team", 80);
        let player = make_player(
            "p1",
            "Happy",
            "t1",
            LolRole::Mid,
            90,
            90,
            80,
            100_000,
            110_000,
            Some("2028-06-15"),
            vec![PlayerTrait::TeamPlayer],
        );

        let game = make_game_with_single_ai_team(team.clone(), vec![player]);
        let p = game.players.iter().find(|p| p.id == "p1").unwrap();
        let t = game.teams.iter().find(|t| t.id == "t1").unwrap();
        let satisfaction = compute_satisfaction(p, t, &game);
        let decision = decide_action(satisfaction, p, t, &game);

        assert_eq!(
            decision,
            PlayerAgentDecision::Silent,
            "happy player should be Silent, satisfaction={satisfaction}"
        );
    }

    #[test]
    fn test_decide_action_mid_satisfaction_renewal() {
        // Mid satisfaction + short contract → renewal demand
        let team = make_team("t1", "Okay Team", 50);
        let player = make_player(
            "p1",
            "Mid",
            "t1",
            LolRole::Mid,
            70,                 // lol_ovr
            60,                 // morale (mid)
            55,                 // manager_trust (mid)
            50_000,             // wage
            60_000,             // market_value → wage/mv = 0.83 → fairly paid
            Some("2026-09-01"), // < 6 months
            vec![],
        );

        let game = make_game_with_single_ai_team(team.clone(), vec![player]);
        let p = game.players.iter().find(|p| p.id == "p1").unwrap();
        let t = game.teams.iter().find(|t| t.id == "t1").unwrap();
        let satisfaction = compute_satisfaction(p, t, &game);
        let decision = decide_action(satisfaction, p, t, &game);

        assert_eq!(
            decision,
            PlayerAgentDecision::RequestRenewal,
            "mid-satisfaction player with short contract should demand renewal, satisfaction={satisfaction}"
        );
    }

    // -----------------------------------------------------------------------
    // Phase 3: Orchestration
    // -----------------------------------------------------------------------

    #[test]
    fn test_process_ai_player_agents_no_panic() {
        // Minimal test: run process_ai_player_agents with empty game state
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap());
        let manager = Manager::new(
            "mgr1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "GB".to_string(),
        );
        let mut team = make_team("ai_team", "AI Team", 50);
        team.team_kind = TeamKind::Main;
        team.manager_id = None;

        let mut game = Game::new(clock, manager, vec![team], vec![], vec![], vec![]);
        // Should not panic
        process_ai_player_agents(&mut game);
    }

    #[test]
    fn test_process_ai_player_agents_sets_flags() {
        // Integration: run process_ai_player_agents and verify
        // that flags are set correctly on applicable players.
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap());
        let manager = Manager::new(
            "mgr1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "GB".to_string(),
        );

        let mut team = make_team("ai_team", "AI Team", 30);
        team.team_kind = TeamKind::Main;
        team.manager_id = None;

        // Unhappy player who should request transfer
        let unhappy = make_player(
            "p1",
            "Unhappy",
            "ai_team",
            LolRole::Mid,
            55,
            20,
            15,
            20_000,
            100_000,
            Some("2026-09-01"),
            vec![],
        );

        // Happy player
        let happy = make_player(
            "p2",
            "Happy",
            "ai_team",
            LolRole::Top,
            85,
            90,
            80,
            100_000,
            100_000,
            Some("2028-06-15"),
            vec![PlayerTrait::TeamPlayer],
        );

        let mut game = Game::new(
            clock,
            manager,
            vec![team],
            vec![unhappy, happy],
            vec![],
            vec![],
        );

        process_ai_player_agents(&mut game);

        let p1 = game.players.iter().find(|p| p.id == "p1").unwrap();
        let p2 = game.players.iter().find(|p| p.id == "p2").unwrap();

        // p1 should be transfer_listed (unhappy, underpaid, short contract)
        // or at minimum have some flag set
        assert!(
            p1.transfer_listed || p1.morale_core.renewal_state.is_some(),
            "unhappy player should have a decision flag set"
        );

        // p2 should be silent (happy)
        assert!(
            !p2.transfer_listed,
            "happy player should not be transfer listed"
        );
    }
}
