use rand::Rng;

use crate::engine::sim_background::{LiveMatchState, MatchCommand};
use crate::engine::types::Side;

#[derive(Debug, Clone)]
pub struct AiProfile {
    pub reputation: u32,
    pub experience: u8,
}

impl Default for AiProfile {
    fn default() -> Self {
        Self {
            reputation: 500,
            experience: 50,
        }
    }
}

pub fn ai_decide<R: Rng>(
    _match_state: &LiveMatchState,
    _side: Side,
    _profile: &AiProfile,
    _rng: &mut R,
) -> Vec<MatchCommand> {
    Vec::new()
}
