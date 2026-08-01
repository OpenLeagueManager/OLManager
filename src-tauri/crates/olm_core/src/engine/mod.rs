// Pre-existing clippy warnings tracked in #92
#![allow(
    clippy::new_without_default,
    clippy::collapsible_if,
    clippy::useless_conversion
)]

pub mod ai;
pub mod engine;
pub mod event;
pub mod report;
pub mod sim_background;
pub mod types;

// Re-export key types for convenience
pub use engine::simulate_lol;
pub use event::{EventType, MatchEvent};
pub use report::{KillDetail, MatchReport, MatchReportEndReason, PlayerMatchStats, TeamStats};
pub use sim_background::LolRole;
pub use sim_background::{
    LiveMatchState, MatchCommand, MatchPhase, MatchSnapshot, MinuteResult, SubstitutionRecord,
    TeamRoles,
};
pub use types::{DraftStrategy, MatchConfig, PlayerData, Side, TeamData, Zone};
