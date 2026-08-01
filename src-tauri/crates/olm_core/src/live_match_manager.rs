mod team_builder;
pub use team_builder::auto_select_team_roles;
use team_builder::build_team_with_bench;

use log::info;
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

use crate::game::Game;

use crate::domain::league::StandingEntry;
use crate::engine::{LiveMatchState, MatchCommand, MatchConfig, MatchSnapshot, MinuteResult, Side};

const LOL_STARTERS_REQUIRED: usize = 5;

// ---------------------------------------------------------------------------
// MatchMode — how the user wants to experience this match
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchMode {
    /// User controls their team live (full interactivity)
    Live,
    /// User watches as spectator (no interaction, can control speed)
    Spectator,
    /// Instantly simulate — no UI, just get the result
    Instant,
}

// ---------------------------------------------------------------------------
// LiveMatchSession — wraps LiveMatchState + metadata for Tauri layer
// ---------------------------------------------------------------------------

pub struct LiveMatchSession {
    pub match_state: LiveMatchState,
    pub rng: StdRng,
    pub mode: MatchMode,
    pub fixture_index: usize,
    pub round_matchday: u32,
    pub round_previous_standings: Vec<StandingEntry>,
    pub home_team_id: String,
    pub away_team_id: String,
    pub user_side: Option<Side>,
}

impl LiveMatchSession {
    /// Step one minute and apply AI decisions for computer-controlled sides.
    pub fn step(&mut self) -> MinuteResult {
        let result = self.match_state.step_minute(&mut self.rng);
        result
    }

    /// Step multiple minutes at once (for fast-forward / instant sim).
    pub fn step_many(&mut self, count: u16) -> Vec<MinuteResult> {
        let mut results = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let result = self.step();
            let finished = result.is_finished;
            results.push(result);
            if finished {
                break;
            }
        }
        results
    }

    /// Run the entire match to completion instantly.
    pub fn run_to_completion(&mut self) -> Vec<MinuteResult> {
        let mut results = Vec::with_capacity(100);
        loop {
            let result = self.step();
            let finished = result.is_finished;
            results.push(result);
            if finished {
                break;
            }
        }
        results
    }

    pub fn snapshot(&self) -> MatchSnapshot {
        self.match_state.snapshot()
    }

    pub fn apply_command(&mut self, cmd: MatchCommand) -> Result<(), String> {
        self.match_state.apply_command(cmd)
    }

    pub fn is_finished(&self) -> bool {
        self.match_state.is_finished()
    }
}

// ---------------------------------------------------------------------------
// Helper: build a LiveMatchSession from the Game state
// ---------------------------------------------------------------------------

/// Create a live match session for a specific fixture.
pub fn create_live_match(
    game: &Game,
    fixture_index: usize,
    mode: MatchMode,
    allows_extra_time: bool,
) -> Result<LiveMatchSession, String> {
    info!(
        "[live_match] create_live_match: fixture={}, mode={:?}, extra_time={}",
        fixture_index, mode, allows_extra_time
    );
    let league = game.active_league().ok_or("No league")?;
    let fixture = league
        .fixtures
        .get(fixture_index)
        .ok_or("Fixture not found")?;

    let home_team_id = fixture.home_team_id.clone();
    let away_team_id = fixture.away_team_id.clone();

    // Build engine TeamData (starting XI = first 11 players by position)
    let (home_xi, home_bench) = build_team_with_bench(game, &home_team_id);
    let (away_xi, away_bench) = build_team_with_bench(game, &away_team_id);

    if home_xi.players.len() < LOL_STARTERS_REQUIRED
        || away_xi.players.len() < LOL_STARTERS_REQUIRED
    {
        return Err(format!(
            "Cannot start match: incomplete lineup (home: {}, away: {}, required: {})",
            home_xi.players.len(),
            away_xi.players.len(),
            LOL_STARTERS_REQUIRED
        ));
    }

    let config = MatchConfig::default();

    let match_state = LiveMatchState::new(
        home_xi,
        away_xi,
        config,
        home_bench,
        away_bench,
        allows_extra_time,
    );
    // Determine user side
    let user_side = game.manager.team_id.as_ref().and_then(|tid| {
        if *tid == home_team_id {
            Some(Side::Home)
        } else if *tid == away_team_id {
            Some(Side::Away)
        } else {
            None
        }
    });

    let home_name = game
        .teams
        .iter()
        .find(|t| t.id == home_team_id)
        .map(|t| t.name.as_str())
        .unwrap_or("?");
    let away_name = game
        .teams
        .iter()
        .find(|t| t.id == away_team_id)
        .map(|t| t.name.as_str())
        .unwrap_or("?");
    info!(
        "[live_match] session created: {} vs {}, user_side={:?}",
        home_name, away_name, user_side
    );

    Ok(LiveMatchSession {
        match_state,
        rng: StdRng::from_rng(&mut rand::rng()),
        mode,
        fixture_index,
        round_matchday: fixture.matchday,
        round_previous_standings: league.standings.clone(),
        home_team_id,
        away_team_id,
        user_side,
    })
}
