use log::info;
use olm_core::domain::negotiation::NegotiationFeedback;
use olm_core::domain::transfer_history::TransferHistoryEntry;
use tauri::State;

use olm_core::game::Game;
use olm_core::state::StateManager;
use olm_core::transfers::{
    get_transfer_history, TransferBidFinancialProjection, TransferDestination,
    TransferNegotiationDecision, TransferNegotiationOutcome,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct TransferNegotiationCommandResponse {
    pub decision: TransferNegotiationDecision,
    pub suggested_fee: Option<u64>,
    pub is_terminal: bool,
    pub feedback: NegotiationFeedback,
    pub game: Game,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WageNegotiationCommandResponse {
    pub decision: TransferNegotiationDecision,
    pub suggested_wage: Option<u32>,
    pub suggested_years: Option<u8>,
    pub is_terminal: bool,
    pub feedback: NegotiationFeedback,
    pub game: Game,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TransferBidFinancialProjectionCommandResponse {
    pub projection: TransferBidFinancialProjection,
}

#[tauri::command]
pub fn toggle_transfer_list(
    state: State<'_, StateManager>,
    player_id: String,
) -> Result<Game, String> {
    toggle_transfer_list_internal(&state, &player_id)
}

fn toggle_transfer_list_internal(state: &StateManager, player_id: &str) -> Result<Game, String> {
    info!("[cmd] toggle_transfer_list: player_id={}", player_id);
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    if let Some(p) = game.players.iter_mut().find(|p| p.id == player_id) {
        p.transfer_listed = !p.transfer_listed;
    } else {
        return Err("Player not found".into());
    }
    state.set_game(game.clone());
    Ok(crate::client_game::game_for_client(&game))
}

#[tauri::command]
pub fn toggle_loan_list(state: State<'_, StateManager>, player_id: String) -> Result<Game, String> {
    toggle_loan_list_internal(&state, &player_id)
}

fn toggle_loan_list_internal(state: &StateManager, player_id: &str) -> Result<Game, String> {
    info!("[cmd] toggle_loan_list: player_id={}", player_id);
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    if let Some(p) = game.players.iter_mut().find(|p| p.id == player_id) {
        p.loan_listed = !p.loan_listed;
    } else {
        return Err("Player not found".into());
    }
    state.set_game(game.clone());
    Ok(crate::client_game::game_for_client(&game))
}

#[tauri::command]
pub fn make_transfer_bid(
    state: State<'_, StateManager>,
    player_id: String,
    fee: u64,
    destination: Option<TransferDestination>,
    included_player_ids: Vec<String>,
) -> Result<TransferNegotiationCommandResponse, String> {
    make_transfer_bid_internal(
        &state,
        &player_id,
        fee,
        destination.unwrap_or_default(),
        &included_player_ids,
    )
}

fn make_transfer_bid_internal(
    state: &StateManager,
    player_id: &str,
    fee: u64,
    destination: TransferDestination,
    included_player_ids: &[String],
) -> Result<TransferNegotiationCommandResponse, String> {
    info!(
        "[cmd] make_transfer_bid: player_id={}, fee={}, included={}",
        player_id,
        fee,
        included_player_ids.len()
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let result = olm_core::transfers::make_transfer_bid(
        &mut game,
        player_id,
        fee,
        destination,
        included_player_ids,
    )?;
    state.set_game(game.clone());

    Ok(map_transfer_negotiation_response(
        result,
        crate::client_game::game_for_client(&game),
    ))
}

#[tauri::command]
pub fn preview_transfer_bid_financial_impact(
    state: State<'_, StateManager>,
    player_id: String,
    fee: u64,
    destination: Option<TransferDestination>,
) -> Result<TransferBidFinancialProjectionCommandResponse, String> {
    preview_transfer_bid_financial_impact_internal(
        &state,
        &player_id,
        fee,
        destination.unwrap_or_default(),
    )
}

fn preview_transfer_bid_financial_impact_internal(
    state: &StateManager,
    player_id: &str,
    fee: u64,
    destination: TransferDestination,
) -> Result<TransferBidFinancialProjectionCommandResponse, String> {
    info!(
        "[cmd] preview_transfer_bid_financial_impact: player_id={}, fee={}",
        player_id, fee
    );

    let game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let projection = olm_core::transfers::project_transfer_bid_financial_impact(
        &game,
        player_id,
        fee,
        destination,
    )?;

    Ok(TransferBidFinancialProjectionCommandResponse { projection })
}

#[tauri::command]
pub fn respond_to_offer(
    state: State<'_, StateManager>,
    player_id: String,
    offer_id: String,
    accept: bool,
) -> Result<Game, String> {
    respond_to_offer_internal(&state, &player_id, &offer_id, accept)
}

fn respond_to_offer_internal(
    state: &StateManager,
    player_id: &str,
    offer_id: &str,
    accept: bool,
) -> Result<Game, String> {
    info!(
        "[cmd] respond_to_offer: player_id={}, offer_id={}, accept={}",
        player_id, offer_id, accept
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    olm_core::transfers::respond_to_offer(&mut game, player_id, offer_id, accept)?;
    state.set_game(game.clone());
    Ok(crate::client_game::game_for_client(&game))
}

#[tauri::command]
pub fn counter_offer(
    state: State<'_, StateManager>,
    player_id: String,
    offer_id: String,
    requested_fee: u64,
    included_player_ids: Vec<String>,
) -> Result<TransferNegotiationCommandResponse, String> {
    counter_offer_internal(
        &state,
        &player_id,
        &offer_id,
        requested_fee,
        &included_player_ids,
    )
}

fn counter_offer_internal(
    state: &StateManager,
    player_id: &str,
    offer_id: &str,
    requested_fee: u64,
    included_player_ids: &[String],
) -> Result<TransferNegotiationCommandResponse, String> {
    info!(
        "[cmd] counter_offer: player_id={}, offer_id={}, requested_fee={}, included={}",
        player_id,
        offer_id,
        requested_fee,
        included_player_ids.len()
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let result = olm_core::transfers::counter_offer(
        &mut game,
        player_id,
        offer_id,
        requested_fee,
        included_player_ids,
    )?;
    state.set_game(game.clone());

    Ok(map_transfer_negotiation_response(
        result,
        crate::client_game::game_for_client(&game),
    ))
}

fn map_transfer_negotiation_response(
    outcome: TransferNegotiationOutcome,
    game: Game,
) -> TransferNegotiationCommandResponse {
    TransferNegotiationCommandResponse {
        decision: outcome.decision,
        suggested_fee: outcome.suggested_fee,
        is_terminal: outcome.is_terminal,
        feedback: outcome.feedback,
        game: crate::client_game::game_for_client(&game),
    }
}

#[tauri::command]
pub fn negotiate_player_wage(
    state: State<'_, StateManager>,
    player_id: String,
    offer_id: String,
    annual_wage: u32,
    contract_years: u8,
) -> Result<WageNegotiationCommandResponse, String> {
    negotiate_player_wage_internal(&state, &player_id, &offer_id, annual_wage, contract_years)
}

fn negotiate_player_wage_internal(
    state: &StateManager,
    player_id: &str,
    offer_id: &str,
    annual_wage: u32,
    contract_years: u8,
) -> Result<WageNegotiationCommandResponse, String> {
    info!(
        "[cmd] negotiate_player_wage: player_id={}, offer_id={}, annual_wage={}, years={}",
        player_id, offer_id, annual_wage, contract_years
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    let result = olm_core::transfers::negotiate_player_wage(
        &mut game,
        player_id,
        offer_id,
        annual_wage,
        contract_years,
    )?;
    state.set_game(game.clone());

    Ok(WageNegotiationCommandResponse {
        decision: result.decision,
        suggested_wage: result.suggested_wage,
        suggested_years: result.suggested_years,
        is_terminal: result.is_terminal,
        feedback: result.feedback,
        game,
    })
}

#[tauri::command]
pub fn send_scout(
    state: State<'_, StateManager>,
    scout_id: String,
    player_id: String,
) -> Result<Game, String> {
    info!(
        "[cmd] send_scout: scout_id={}, player_id={}",
        scout_id, player_id
    );
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    olm_core::scouting::send_scout(&mut game, &scout_id, &player_id)?;
    state.set_game(game.clone());
    Ok(crate::client_game::game_for_client(&game))
}

#[tauri::command]
pub fn release_player_contract(
    state: State<'_, StateManager>,
    player_id: String,
) -> Result<Game, String> {
    info!("[cmd] release_player_contract: player_id={}", player_id);
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    olm_core::transfers::release_player_contract(&mut game, &player_id)?;
    state.set_game(game.clone());
    Ok(crate::client_game::game_for_client(&game))
}

#[tauri::command]
pub fn get_transfer_history_cmd(
    state: State<'_, StateManager>,
) -> Result<Vec<TransferHistoryEntry>, String> {
    let game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;
    Ok(get_transfer_history(&game))
}

#[cfg(test)]
mod tests {
    use super::{
        counter_offer_internal, make_transfer_bid_internal, negotiate_player_wage_internal,
        preview_transfer_bid_financial_impact_internal, respond_to_offer_internal,
        toggle_loan_list_internal, toggle_transfer_list_internal,
    };
    use chrono::{TimeZone, Utc};
    use olm_core::clock::GameClock;
    use olm_core::domain::manager::Manager;
    use olm_core::domain::player::{
        LolRole, Player, PlayerAttributes, TransferOffer, TransferOfferStatus,
        WageNegotiationStatus,
    };
    use olm_core::domain::season::TransferWindowStatus;
    use olm_core::domain::team::Team;
    use olm_core::game::Game;
    use olm_core::state::StateManager;
    use olm_core::transfers::{TransferDestination, TransferNegotiationDecision};

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

    fn make_user_team() -> Team {
        let mut team = Team::new(
            "team-1".to_string(),
            "User FC".to_string(),
            "USR".to_string(),
            "England".to_string(),
            "London".to_string(),
            "User Ground".to_string(),
            25_000,
        );
        team.finance = 5_000_000;
        team.transfer_budget = 2_000_000;
        team.manager_id = Some("manager-1".to_string());
        team
    }

    fn make_buyer_team() -> Team {
        let mut team = Team::new(
            "team-2".to_string(),
            "Buyer FC".to_string(),
            "BUY".to_string(),
            "England".to_string(),
            "Liverpool".to_string(),
            "Buyer Ground".to_string(),
            28_000,
        );
        team.finance = 6_000_000;
        team.transfer_budget = 3_000_000;
        team
    }

    fn make_player_with_offer() -> Player {
        let mut player = Player::new(
            "player-1".to_string(),
            "P. One".to_string(),
            "Player One".to_string(),
            "2000-01-01".to_string(),
            "England".to_string(),
            LolRole::Adc,
            default_attrs(),
        );
        player.team_id = Some("team-1".to_string());
        player.contract_end = Some("2028-06-30".to_string());
        player.market_value = 1_000_000;
        player.transfer_offers.push(TransferOffer {
            id: "offer-1".to_string(),
            from_team_id: "team-2".to_string(),
            destination_team_id: None,
            fee: 900_000,
            wage_offered: 0,
            last_manager_fee: None,
            negotiation_round: 1,
            suggested_counter_fee: None,
            players_included: vec![],
            status: TransferOfferStatus::Pending,
            date: "2026-08-01".to_string(),
            wage_negotiation_status: WageNegotiationStatus::NotStarted,
            contract_years_offered: 0,
            suggested_counter_wage: None,
            suggested_counter_years: None,
            wage_negotiation_round: 0,
        });
        player
    }

    fn make_game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team-1".to_string());

        let mut game = Game::new(
            clock,
            manager,
            vec![make_user_team(), make_buyer_team()],
            vec![make_player_with_offer()],
            vec![],
            vec![],
        );
        game.season_context.transfer_window.status = TransferWindowStatus::Open;
        game
    }

    fn make_bid_target_player() -> Player {
        let mut player = Player::new(
            "player-2".to_string(),
            "P. Two".to_string(),
            "Player Two".to_string(),
            "2000-01-01".to_string(),
            "England".to_string(),
            LolRole::Adc,
            default_attrs(),
        );
        player.team_id = Some("team-2".to_string());
        player.contract_end = Some("2028-06-30".to_string());
        player.market_value = 1_000_000;
        player.morale = 35;
        player.stats.appearances = 1;
        player
    }

    fn make_bid_game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team-1".to_string());

        let mut game = Game::new(
            clock,
            manager,
            vec![make_user_team(), make_buyer_team()],
            vec![make_bid_target_player()],
            vec![],
            vec![],
        );
        game.season_context.transfer_window.status = TransferWindowStatus::Open;
        game.teams[0].reputation = 700;
        game.teams[1].reputation = 350;
        game
    }

    fn make_free_agent_bid_game() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team-1".to_string());

        let mut free_agent = Player::new(
            "player-fa-1".to_string(),
            "FA One".to_string(),
            "Free Agent One".to_string(),
            "2001-02-10".to_string(),
            "England".to_string(),
            LolRole::Adc,
            default_attrs(),
        );
        free_agent.team_id = None;
        free_agent.transfer_listed = true;
        free_agent.market_value = 700_000;
        free_agent.wage = 90_000;

        let mut game = Game::new(
            clock,
            manager,
            vec![make_user_team(), make_buyer_team()],
            vec![free_agent],
            vec![],
            vec![],
        );
        game.season_context.transfer_window.status = TransferWindowStatus::Open;
        game
    }

    #[test]
    fn counter_offer_internal_returns_payload_and_updates_state() {
        let state = StateManager::new();
        state.set_game(make_game());

        let response = counter_offer_internal(&state, "player-1", "offer-1", 1_050_000, &[])
            .expect("response");

        assert_eq!(response.decision, TransferNegotiationDecision::Accepted);
        assert_eq!(response.game.players[0].team_id.as_deref(), Some("team-1"));
        assert_eq!(
            response.game.players[0].transfer_offers[0].status,
            TransferOfferStatus::Accepted
        );
        assert_eq!(
            response.game.players[0].transfer_offers[0].wage_negotiation_status,
            WageNegotiationStatus::Pending
        );

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        assert_eq!(
            stored_game
                .players
                .iter()
                .find(|player| player.id == "player-1")
                .and_then(|player| player.team_id.clone())
                .as_deref(),
            Some("team-1")
        );
    }

    #[test]
    fn negotiate_player_wage_internal_completes_accepted_transfer() {
        let state = StateManager::new();
        state.set_game(make_game());

        counter_offer_internal(&state, "player-1", "offer-1", 1_050_000, &[])
            .expect("accepted club offer");
        let response = negotiate_player_wage_internal(&state, "player-1", "offer-1", 1_000_000, 3)
            .expect("accepted wage offer");

        assert_eq!(response.decision, TransferNegotiationDecision::Accepted);
        assert!(response.is_terminal);
        let player = response
            .game
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("transferred player");
        assert_eq!(player.team_id.as_deref(), Some("team-2"));
        assert_eq!(
            player.transfer_offers[0].wage_negotiation_status,
            WageNegotiationStatus::Agreed
        );
    }

    #[test]
    fn make_transfer_bid_internal_returns_payload_and_updates_state() {
        let state = StateManager::new();
        state.set_game(make_bid_game());

        let response = make_transfer_bid_internal(
            &state,
            "player-2",
            1_050_000,
            TransferDestination::Main,
            &[],
        )
        .expect("response");

        assert_eq!(response.decision, TransferNegotiationDecision::Accepted);
        assert!(!response.is_terminal);
        assert_eq!(response.game.players[0].team_id.as_deref(), Some("team-2"));
        assert_eq!(
            response.game.players[0].transfer_offers[0].status,
            TransferOfferStatus::Accepted
        );
        assert_eq!(
            response.game.players[0].transfer_offers[0]
                .destination_team_id
                .as_deref(),
            Some("team-1")
        );

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-2")
            .expect("stored player");
        assert_eq!(stored_player.team_id.as_deref(), Some("team-2"));
        assert_eq!(
            stored_player.transfer_offers[0]
                .destination_team_id
                .as_deref(),
            Some("team-1")
        );
    }

    #[test]
    fn make_transfer_bid_internal_can_return_counter_offer_feedback() {
        let state = StateManager::new();
        state.set_game(make_bid_game());

        let response =
            make_transfer_bid_internal(&state, "player-2", 900_000, TransferDestination::Main, &[])
                .expect("response");

        assert_eq!(response.decision, TransferNegotiationDecision::CounterOffer);
        assert_eq!(response.suggested_fee, Some(950_000));
        assert!(!response.is_terminal);
        assert_eq!(response.feedback.round, 1);

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        assert_eq!(
            stored_game
                .players
                .iter()
                .find(|player| player.id == "player-2")
                .and_then(|player| player.team_id.clone())
                .as_deref(),
            Some("team-2")
        );
    }

    #[test]
    fn make_transfer_bid_internal_uses_existing_negotiation_round_on_follow_up_bid() {
        let state = StateManager::new();
        state.set_game(make_bid_game());

        let first =
            make_transfer_bid_internal(&state, "player-2", 900_000, TransferDestination::Main, &[])
                .expect("first bid");
        assert_eq!(first.decision, TransferNegotiationDecision::CounterOffer);
        assert_eq!(first.feedback.round, 1);

        let second =
            make_transfer_bid_internal(&state, "player-2", 950_000, TransferDestination::Main, &[])
                .expect("second bid");

        assert_eq!(second.decision, TransferNegotiationDecision::Accepted);
        assert!(!second.is_terminal);
        assert_eq!(second.feedback.round, 2);
        assert_eq!(second.game.players[0].team_id.as_deref(), Some("team-2"));
        assert_eq!(
            second.game.players[0].transfer_offers[0]
                .destination_team_id
                .as_deref(),
            Some("team-1")
        );
    }

    #[test]
    fn make_transfer_bid_internal_accepts_free_agent_signing() {
        let state = StateManager::new();
        state.set_game(make_free_agent_bid_game());

        let response = make_transfer_bid_internal(
            &state,
            "player-fa-1",
            450_000,
            TransferDestination::Main,
            &[],
        )
        .expect("response");

        assert_eq!(response.decision, TransferNegotiationDecision::Accepted);
        assert!(!response.is_terminal);

        let player = response
            .game
            .players
            .iter()
            .find(|p| p.id == "player-fa-1")
            .expect("player should exist");
        let accepted_offer = player
            .transfer_offers
            .iter()
            .find(|o| o.status == TransferOfferStatus::Accepted)
            .expect("accepted offer should be created");
        assert_eq!(
            accepted_offer.destination_team_id.as_deref(),
            Some("team-1")
        );
    }

    #[test]
    fn respond_to_offer_internal_returns_game_and_updates_state() {
        let state = StateManager::new();
        state.set_game(make_game());

        let response =
            respond_to_offer_internal(&state, "player-1", "offer-1", false).expect("response");

        let player = response
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("player should exist");
        assert_eq!(player.team_id.as_deref(), Some("team-1"));
        assert_eq!(
            player.transfer_offers[0].status,
            TransferOfferStatus::Rejected
        );

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("stored player should exist");
        assert_eq!(stored_player.team_id.as_deref(), Some("team-1"));
        assert_eq!(
            stored_player.transfer_offers[0].status,
            TransferOfferStatus::Rejected
        );
    }

    #[test]
    fn toggle_transfer_list_internal_updates_state() {
        let state = StateManager::new();
        state.set_game(make_game());

        let response = toggle_transfer_list_internal(&state, "player-1").expect("response");

        let player = response
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("player should exist");
        assert!(player.transfer_listed);

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("stored player should exist");
        assert!(stored_player.transfer_listed);
    }

    #[test]
    fn toggle_loan_list_internal_updates_state() {
        let state = StateManager::new();
        state.set_game(make_game());

        let response = toggle_loan_list_internal(&state, "player-1").expect("response");

        let player = response
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("player should exist");
        assert!(player.loan_listed);

        let stored_game = state.get_game(|game| game.clone()).expect("stored game");
        let stored_player = stored_game
            .players
            .iter()
            .find(|player| player.id == "player-1")
            .expect("stored player should exist");
        assert!(stored_player.loan_listed);
    }

    #[test]
    fn preview_transfer_bid_financial_impact_internal_returns_projection() {
        let state = StateManager::new();
        state.set_game(make_bid_game());

        let response = preview_transfer_bid_financial_impact_internal(
            &state,
            "player-2",
            1_000_000,
            TransferDestination::Main,
        )
        .expect("response");

        assert_eq!(response.projection.transfer_budget_before, 2_000_000);
        assert_eq!(response.projection.transfer_budget_after, 1_000_000);
        assert_eq!(response.projection.finance_before, 5_000_000);
        assert_eq!(response.projection.finance_after, 4_000_000);
        assert!(!response.projection.exceeds_transfer_budget);
        assert!(!response.projection.exceeds_finance);
    }
}
