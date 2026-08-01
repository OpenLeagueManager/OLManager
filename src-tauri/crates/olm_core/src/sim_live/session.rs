use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

use super::{
    LanerCombatStateRuntime, LolChampionCombatProfileInput, LolChampionUltimateInput, RuntimeState,
    SimulatorAiMode, SimulatorPolicyConfig,
};

#[derive(Default)]
pub struct SimLiveStoreState {
    pub sessions: Mutex<HashMap<String, SimLiveSession>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SimLiveSession {
    pub id: String,
    pub seed: String,
    pub(crate) state: RuntimeState,
    pub tick_index: u64,
    pub wave_spawn_at: f64,
    pub next_minion_id: u64,
    pub snapshot: Value,
    pub champion_by_player_id: HashMap<String, String>,
    pub champion_profiles_by_id: HashMap<String, LolChampionCombatProfileInput>,
    pub champion_ultimates_by_id: HashMap<String, LolChampionUltimateInput>,
    pub lane_combat_state_by_champion: HashMap<String, LanerCombatStateRuntime>,
    pub ai_mode: SimulatorAiMode,
    pub policy: SimulatorPolicyConfig,
}
