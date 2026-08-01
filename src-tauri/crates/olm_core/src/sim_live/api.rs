use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use super::{LolChampionCombatProfileInput, LolChampionUltimateInput, SimulatorAiMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulatorPolicyConfig {
    #[serde(default = "default_no_dive_hp_min")]
    pub no_dive_hp_min: f64,
    #[serde(default = "default_trade_retreat_hp_ratio")]
    pub trade_retreat_hp_ratio: f64,
    #[serde(default = "default_trade_hp_disadvantage_allowance")]
    pub trade_hp_disadvantage_allowance: f64,
    #[serde(default = "default_lane_chase_leash_radius")]
    pub lane_chase_leash_radius: f64,
    #[serde(default = "default_hybrid_open_trade_confidence_high")]
    pub hybrid_open_trade_confidence_high: f64,
    #[serde(default = "default_hybrid_disengage_confidence_low")]
    pub hybrid_disengage_confidence_low: f64,
}

impl Default for SimulatorPolicyConfig {
    fn default() -> Self {
        Self {
            no_dive_hp_min: default_no_dive_hp_min(),
            trade_retreat_hp_ratio: default_trade_retreat_hp_ratio(),
            trade_hp_disadvantage_allowance: default_trade_hp_disadvantage_allowance(),
            lane_chase_leash_radius: default_lane_chase_leash_radius(),
            hybrid_open_trade_confidence_high: default_hybrid_open_trade_confidence_high(),
            hybrid_disengage_confidence_low: default_hybrid_disengage_confidence_low(),
        }
    }
}

fn default_no_dive_hp_min() -> f64 {
    0.27
}
fn default_trade_retreat_hp_ratio() -> f64 {
    0.20
}
fn default_trade_hp_disadvantage_allowance() -> f64 {
    0.24
}
fn default_lane_chase_leash_radius() -> f64 {
    0.14
}
fn default_hybrid_open_trade_confidence_high() -> f64 {
    0.55
}
fn default_hybrid_disengage_confidence_low() -> f64 {
    0.16
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimLiveInitRequest {
    pub session_id: String,
    pub seed: String,
    pub snapshot: Value,
    #[serde(default)]
    pub champion_by_player_id: HashMap<String, String>,
    #[serde(default)]
    pub champion_profiles_by_id: HashMap<String, LolChampionCombatProfileInput>,
    #[serde(default)]
    pub champion_ultimates_by_id: HashMap<String, LolChampionUltimateInput>,
    pub initial_state: Option<Value>,
    #[serde(default)]
    pub ai_mode: SimulatorAiMode,
    #[serde(default)]
    pub policy: SimulatorPolicyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimLiveTickRequest {
    pub session_id: String,
    pub dt_sec: f64,
    pub running: bool,
    pub speed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimLiveResetRequest {
    pub session_id: String,
    pub seed: String,
    pub initial_state: Option<Value>,
    #[serde(default)]
    pub ai_mode: SimulatorAiMode,
    #[serde(default)]
    pub policy: Option<SimulatorPolicyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimLiveDisposeRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimLiveStateResponse {
    pub session_id: String,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimLiveDisposeResponse {
    pub session_id: String,
    pub disposed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimLiveRunToCompletionRequest {
    pub seed: String,
    pub snapshot: Value,
    #[serde(default)]
    pub champion_by_player_id: HashMap<String, String>,
    #[serde(default)]
    pub champion_profiles_by_id: HashMap<String, LolChampionCombatProfileInput>,
    #[serde(default)]
    pub champion_ultimates_by_id: HashMap<String, LolChampionUltimateInput>,
    #[serde(default)]
    pub ai_mode: SimulatorAiMode,
    #[serde(default)]
    pub policy: SimulatorPolicyConfig,
    #[serde(default = "default_run_to_completion_dt_sec")]
    pub dt_sec: f64,
    #[serde(default = "default_run_to_completion_speed")]
    pub speed: f64,
    #[serde(default = "default_run_to_completion_max_ticks")]
    pub max_ticks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimLiveRunToCompletionResponse {
    pub winner: Option<String>,
    pub ticks: u64,
    pub elapsed_simulated_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimLiveSkipToEndRequest {
    pub session_id: String,
    #[serde(default = "default_skip_to_end_dt_sec")]
    pub dt_sec: f64,
    #[serde(default = "default_skip_to_end_speed")]
    pub speed: f64,
    #[serde(default = "default_skip_to_end_max_ticks")]
    pub max_ticks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimLiveSkipToEndResponse {
    pub session_id: String,
    pub state: Value,
    pub winner: Option<String>,
    pub ticks: u64,
    pub elapsed_simulated_sec: f64,
}

fn default_run_to_completion_dt_sec() -> f64 {
    0.2
}
fn default_run_to_completion_speed() -> f64 {
    12.0
}
fn default_run_to_completion_max_ticks() -> u64 {
    3600
}
fn default_skip_to_end_dt_sec() -> f64 {
    default_run_to_completion_dt_sec()
}
fn default_skip_to_end_speed() -> f64 {
    default_run_to_completion_speed()
}
fn default_skip_to_end_max_ticks() -> u64 {
    default_run_to_completion_max_ticks()
}
