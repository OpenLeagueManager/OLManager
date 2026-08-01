use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::sync::OnceLock;

mod api;
mod combat;
mod economy;
mod events;
mod layout;
mod macro_ai;
mod minions;
mod objectives;
mod pathing;
mod runtime;
mod session;
mod state_init;
mod structures;
mod trading;
mod types;
mod util;
mod vision;
mod waves;

pub use api::*;
use economy::{champion_kill_rewards, jungle_camp_cs_reward, jungle_camp_reward};
use events::{log_event, push_event};
use layout::{
    BASE_POSITION_BLUE, BASE_POSITION_RED, LANE_PATH_BOT_BLUE, LANE_PATH_MID_BLUE,
    LANE_PATH_TOP_BLUE, ROLE_SEEDS, STRUCTURE_LAYOUT,
};
use objectives::{
    NeutralCaptureKind, process_dragon_capture, resolve_neutral_capture_decision,
    sync_objectives_from_neutral_timers, tick_neutral_timers,
};
pub use runtime::{
    dispose, init, init_session_from_request, reset, run_to_completion, skip_to_end, tick,
};
pub use session::*;
use state_init::{build_neutral_timers_state, create_initial_state, ensure_runtime_state_defaults};
use structures::{
    apply_damage_to_structure, create_structures, is_structure_targetable, resolve_structure_combat,
};
pub use types::*;
use types::{
    RuntimeEvent, RuntimeStats, RuntimeSummonerSpellSlot, RuntimeTeamStats, RuntimeUltimateSlot,
    Vec2, WardRuntime,
};
use util::{as_mut_object, clamp, dist, normalize, ratio_or_zero, read_time_sec, read_winner};
use vision::{place_wards, process_sweepers, team_has_vision_at};

fn default_visible_stat() -> f64 {
    70.0
}

fn default_staff_execution() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SimulatorAiMode {
    Rules,
    #[default]
    Hybrid,
}

impl SimulatorAiMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Rules => "rules",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Clone)]
struct SnapshotPlayer {
    id: String,
    name: String,
    dribbling: f64,
    agility: f64,
    pace: f64,
    composure: f64,
    shooting: f64,
    positioning: f64,
    teamwork: f64,
    stamina: f64,
    decisions: f64,
    vision: f64,
    passing: f64,
    leadership: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeState {
    time_sec: f64,
    running: bool,
    speed: f64,
    #[serde(default)]
    ai_mode: SimulatorAiMode,
    #[serde(default, skip)]
    policy: SimulatorPolicyConfig,
    winner: Option<String>,
    show_walls: bool,
    champions: Vec<ChampionRuntime>,
    minions: Vec<MinionRuntime>,
    structures: Vec<StructureRuntime>,
    #[serde(default)]
    wards: Vec<WardRuntime>,
    objectives: Value,
    neutral_timers: Value,
    stats: RuntimeStats,
    events: Vec<RuntimeEvent>,
    #[serde(default, skip)]
    lane_combat_state_by_champion: HashMap<String, LanerCombatStateRuntime>,
    #[serde(default)]
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChampionRuntime {
    id: String,
    name: String,
    #[serde(default)]
    champion_id: String,
    team: String,
    role: String,
    lane: String,
    pos: Vec2,
    hp: f64,
    max_hp: f64,
    alive: bool,
    respawn_at: f64,
    attack_cd_until: f64,
    move_speed: f64,
    attack_range: f64,
    attack_type: String,
    attack_damage: f64,
    target_path: Vec<Vec2>,
    target_path_index: usize,
    next_decision_at: f64,
    kills: i64,
    deaths: i64,
    assists: i64,
    gold: i64,
    #[serde(default)]
    spent_gold: i64,
    xp: i64,
    level: i64,
    #[serde(default)]
    cs: i64,
    #[serde(default)]
    has_left_base_once: bool,
    #[serde(default)]
    last_support_cs_at: f64,
    #[serde(default)]
    items: Vec<String>,
    #[serde(default = "default_visible_stat")]
    gameplay_score: f64,
    #[serde(default = "default_visible_stat")]
    iq_score: f64,
    #[serde(default = "default_visible_stat")]
    competitive_score: f64,
    #[serde(default = "default_staff_execution")]
    staff_execution: f64,
    #[serde(default)]
    summoner_spells: Vec<RuntimeSummonerSpellSlot>,
    #[serde(default)]
    ultimate: Option<RuntimeUltimateSlot>,
    #[serde(default)]
    ignite_dot_until: f64,
    #[serde(default)]
    ignite_source_id: Option<String>,
    last_damaged_by_champion_id: Option<String>,
    #[serde(default)]
    last_damaged_by_champion_at: f64,
    last_damaged_at: f64,
    state: String,
    recall_anchor: Option<Vec2>,
    recall_channel_until: f64,
    #[serde(default)]
    realm_banished_until: f64,
    #[serde(default)]
    realm_return_pos: Option<Vec2>,
    #[serde(default)]
    ward_cd_until: f64,
    #[serde(default)]
    sweeper_cd_until: f64,
    #[serde(default)]
    sweeper_active_until: f64,
    #[serde(default)]
    trinket_key: String,
    #[serde(default)]
    trinket_swapped: bool,
    #[serde(default)]
    support_roam_uses: i64,
    #[serde(default)]
    support_roam_cd_until: f64,
    #[serde(default)]
    support_last_roam_role: String,
    #[serde(default)]
    path_stuck_for_sec: f64,
    #[serde(default)]
    forced_lane_recall_cd_until: f64,
    #[serde(default)]
    debug_ai_decision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MinionRuntime {
    id: String,
    team: String,
    lane: String,
    pos: Vec2,
    hp: f64,
    max_hp: f64,
    alive: bool,
    kind: String,
    last_hit_by_champion_id: Option<String>,
    #[serde(default)]
    owner_champion_id: Option<String>,
    #[serde(default)]
    summon_kind: Option<String>,
    #[serde(default)]
    summon_expires_at: f64,
    attack_cd_until: f64,
    move_speed: f64,
    attack_range: f64,
    attack_damage: f64,
    path: Vec<Vec2>,
    path_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StructureRuntime {
    id: String,
    team: String,
    lane: String,
    kind: String,
    pos: Vec2,
    hp: f64,
    max_hp: f64,
    alive: bool,
    attack_cd_until: f64,
    #[serde(default)]
    forced_target_champion_id: Option<String>,
    #[serde(default)]
    forced_target_until: f64,
}

#[derive(Clone, Copy)]
struct StructureSeed {
    id: &'static str,
    team: &'static str,
    lane: &'static str,
    kind: &'static str,
    pos: Vec2,
}

#[derive(Clone, Copy)]
struct NeutralTimerTemplate {
    key: &'static str,
    label: &'static str,
    first_spawn_at: f64,
    max_hp: f64,
    respawn_delay_sec: Option<f64>,
    one_shot: bool,
    window_close_at: Option<f64>,
    combat_grace_until: Option<f64>,
    unlocked: bool,
    pos: Vec2,
}

#[derive(Clone, Copy)]
struct ItemTemplate {
    key: &'static str,
    cost: i64,
    attack_damage: f64,
    max_hp: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeTeamTactics {
    strong_side: String,
    game_timing: String,
    jungle_style: String,
    jungle_pathing: String,
    fight_plan: String,
    support_roaming: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeRoleImpact {
    modifier: f64,
    variance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeStaffEffects {
    #[serde(default = "default_staff_execution")]
    execution: f64,
    #[serde(default = "default_staff_execution")]
    tactics: f64,
    #[serde(default = "default_staff_execution")]
    analysis: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RuntimeScrimPrepSide {
    #[serde(default)]
    preparation: f64,
    #[serde(default)]
    focus: Option<String>,
    #[serde(default)]
    comfort_by_player: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeTeamBuffState {
    baron_until: f64,
    elder_until: f64,
    infernal_stacks: i64,
    mountain_stacks: i64,
    ocean_stacks: i64,
    cloud_stacks: i64,
    hextech_stacks: i64,
    chemtech_stacks: i64,
    #[serde(default)]
    voidgrub_stacks: i64,
    dragon_stacks: i64,
    #[serde(default)]
    dragon_history: Vec<String>,
    soul_kind: Option<String>,
}

impl Default for RuntimeTeamBuffState {
    fn default() -> Self {
        Self {
            baron_until: 0.0,
            elder_until: 0.0,
            infernal_stacks: 0,
            mountain_stacks: 0,
            ocean_stacks: 0,
            cloud_stacks: 0,
            hextech_stacks: 0,
            chemtech_stacks: 0,
            voidgrub_stacks: 0,
            dragon_stacks: 0,
            dragon_history: Vec::new(),
            soul_kind: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RuntimeBuffState {
    blue: RuntimeTeamBuffState,
    red: RuntimeTeamBuffState,
}

impl Default for RuntimeTeamTactics {
    fn default() -> Self {
        Self {
            strong_side: "Bot".to_string(),
            game_timing: "Mid".to_string(),
            jungle_style: "Enabler".to_string(),
            jungle_pathing: "TopToBot".to_string(),
            fight_plan: "FrontToBack".to_string(),
            support_roaming: "Lane".to_string(),
        }
    }
}

#[derive(Clone, Copy)]
enum ItemBuildCategory {
    Tank,
    Bruiser,
    Colossus,
    AssassinAd,
    AssassinAp,
    ControlMage,
    BattleMage,
    AdcCrit,
    AdcAttackSpeed,
    LethalityMarksman,
    SupportEngage,
    SupportEnchanter,
    SupportDamage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NeutralTimersRuntime {
    dragon_soul_unlocked: bool,
    elder_unlocked: bool,
    entities: HashMap<String, NeutralTimerRuntime>,
    #[serde(default)]
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NeutralTimerRuntime {
    key: String,
    label: String,
    alive: bool,
    hp: f64,
    max_hp: f64,
    next_spawn_at: Option<f64>,
    first_spawn_at: f64,
    respawn_delay_sec: Option<f64>,
    one_shot: bool,
    window_close_at: Option<f64>,
    combat_grace_until: Option<f64>,
    unlocked: bool,
    last_spawn_at: Option<f64>,
    last_taken_at: Option<f64>,
    #[serde(default)]
    times_spawned: i64,
    #[serde(default)]
    times_taken: i64,
    pos: Vec2,
    #[serde(default)]
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct WallFile {
    walls: Vec<WallPolygon>,
}

#[derive(Debug, Clone, Deserialize)]
struct WallPolygon {
    id: String,
    #[serde(default)]
    closed: bool,
    points: Vec<Vec2>,
}

#[derive(Debug, Clone)]
struct NavGrid {
    grid_size: usize,
    blocked: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
struct GridCell {
    cx: usize,
    cy: usize,
}

// moved to application/sim_live/layout.rs

const MINION_FIRST_WAVE_AT: f64 = 30.0;
const LANE_COMBAT_UNLOCK_AT: f64 = MINION_FIRST_WAVE_AT + 8.0;
const FIRST_WAVE_CONTEST_UNTIL: f64 = MINION_FIRST_WAVE_AT + 45.0;
const CHAMPION_DECISION_CADENCE_SEC: f64 = 0.85;
const MINION_DAMAGE_TO_MINION_MULTIPLIER: f64 = 0.52;
const MINION_DAMAGE_TO_CHAMPION_MULTIPLIER: f64 = 0.24;
const MINION_DAMAGE_TO_STRUCTURE_MULTIPLIER: f64 = 0.58;
const CHAMPION_DAMAGE_TO_MINION_MULTIPLIER: f64 = 2.2;
const CHAMPION_DAMAGE_TO_CHAMPION_MULTIPLIER: f64 = 1.12;
const RECALL_TRIGGER_HP_RATIO: f64 = 0.34;
const RECALL_CHANNEL_SEC: f64 = 6.5;
const RECALL_REACH_BUFFER_SEC: f64 = 0.8;
const RECALL_SAFE_ENEMY_RADIUS: f64 = 0.2;
const RECALL_CANCEL_ENEMY_RADIUS: f64 = 0.085;
const LANE_CHAMPION_TRADE_RADIUS: f64 = 0.19;
const LANE_REENGAGE_COOLDOWN_SEC: f64 = 2.2;
const LANE_RECENT_TRADE_LOCK_SEC: f64 = 1.7;
const TRADE_HP_DISADVANTAGE_ALLOWANCE: f64 = 0.2;
const LANE_LOCAL_PRESSURE_RADIUS: f64 = 0.1;
const LANE_MINION_CONTEXT_RADIUS: f64 = 0.105;
const LANE_CHASE_MINION_CONTEXT_RADIUS: f64 = 0.12;
const LOCAL_COMBAT_ENGAGE_RADIUS: f64 = 0.16;
const LOCAL_STRUCTURE_ENGAGE_RADIUS: f64 = 0.12;
const LANE_STRUCTURE_PRESSURE_RADIUS: f64 = 0.12;
const LANE_HEALTHY_RETREAT_HP_RATIO: f64 = 0.52;
const LANE_STRONG_UNFAVORABLE_PRESSURE_DELTA: f64 = 0.9;
const LANE_EMPTY_ANCHOR_PROGRESS_MAX_INDEX: usize = 4;
const HYBRID_TRADE_DEBUG_LOG_COOLDOWN_SEC: f64 = 8.0;
const TRADE_SCORE_WEIGHT_BIAS: f64 = -0.18;
const TRADE_SCORE_WEIGHT_SELF_HP: f64 = 1.55;
const TRADE_SCORE_WEIGHT_ENEMY_HP: f64 = -1.45;
const TRADE_SCORE_WEIGHT_CHAMP_NUMBERS: f64 = 0.62;
const TRADE_SCORE_WEIGHT_MINION_NUMBERS: f64 = 0.38;
const TRADE_SCORE_WEIGHT_TOWER_DISTANCE: f64 = 0.56;
const TRADE_SCORE_WEIGHT_ENEMY_OVEREXTENDED: f64 = 0.74;
const TRADE_SCORE_WEIGHT_FIRST_WAVE: f64 = -0.22;
const ASSIST_RADIUS: f64 = 0.11;
const CHAMPION_KILL_GOLD: i64 = 220;
const CHAMPION_ASSIST_GOLD_TOTAL: i64 = 110;
const CHAMPION_KILL_XP: i64 = 180;
const CHAMPION_LAST_DAMAGE_KILL_CREDIT_SEC: f64 = 60.0;
const CHAMPION_KILL_GOLD_MIN: i64 = 150;
const CHAMPION_KILL_GOLD_MAX: i64 = 450;
const CHAMPION_KILL_XP_MIN: i64 = 150;
const CHAMPION_KILL_XP_MAX: i64 = 360;
const CHAMPION_RESPAWN_BASE_SEC: f64 = 18.0;
const CHAMPION_RESPAWN_PER_LEVEL_SEC: f64 = 1.8;
const BARON_BUFF_DURATION_SEC: f64 = 180.0;
const ELDER_BUFF_DURATION_SEC: f64 = 150.0;
const ELDER_EXECUTE_HP_RATIO: f64 = 0.20;
const BARON_MINION_AURA_RADIUS: f64 = 0.12;
const BARON_MINION_DAMAGE_MULTIPLIER: f64 = 1.28;
const BARON_MINION_DAMAGE_REDUCTION: f64 = 0.58;
const BARON_SIEGE_CHAMPION_MINION_DAMAGE_MULTIPLIER: f64 = 1.35;
const CHAMPION_MAX_LEVEL: i64 = 18;
const CHAMPION_LEVEL_UP_HP_GAIN: f64 = 92.0;
const CHAMPION_LEVEL_UP_AD_GAIN: f64 = 3.8;
const TOWER_OUTER_HP: f64 = 4590.0;
const TOWER_INNER_HP: f64 = 4590.0;
const TOWER_INHIB_HP: f64 = 4590.0;
const TOWER_NEXUS_HP: f64 = 4590.0;
const INHIBITOR_HP: f64 = 4000.0;
const NEXUS_HP: f64 = 5500.0;
const EARLY_TOWER_FORTIFICATION_END_AT: f64 = 14.0 * 60.0;
const TOWER_FORTIFICATION_REDUCTION_AT_14: f64 = 0.90;
const TOWER_FORTIFICATION_REDUCTION_AT_20: f64 = 0.80;
const TOWER_FORTIFICATION_REDUCTION_AT_25: f64 = 0.70;
const TOWER_FORTIFICATION_REDUCTION_AT_30: f64 = 0.60;
const TOWER_FORTIFICATION_REDUCTION_AT_35: f64 = 0.35;
const CHAMPION_ATTACK_CADENCE_SEC: f64 = 1.0;
const TOWER_SHOT_DAMAGE: f64 = 40.0;
const TOWER_SHOT_DAMAGE_TO_MINION: f64 = 24.0;
const TOWER_ATTACK_RANGE: f64 = 0.08;
const TOWER_ATTACK_CADENCE_SEC: f64 = 1.0;
const TOWER_AGGRO_LOCK_SEC: f64 = 2.6;
const TOWER_AGGRO_VICTIM_RADIUS: f64 = 0.09;
const TOWER_AGGRO_ATTACKER_RADIUS: f64 = 0.10;
const EVENT_CAP: usize = 200;
const SKIP_FAST_MODE_EXTRA_KEY: &str = "skipFastMode";
const MINION_MELEE_MAX_HP: f64 = 118.0;
const MINION_MELEE_MOVE_SPEED: f64 = 0.068;
const MINION_MELEE_ATTACK_RANGE: f64 = 0.035;
const MINION_MELEE_ATTACK_DAMAGE: f64 = 5.0;
const MINION_MELEE_ATTACK_CADENCE: f64 = 1.05;
const MINION_RANGED_MAX_HP: f64 = 92.0;
const MINION_RANGED_MOVE_SPEED: f64 = 0.071;
const MINION_RANGED_ATTACK_RANGE: f64 = 0.055;
const MINION_RANGED_ATTACK_DAMAGE: f64 = 5.5;
const MINION_RANGED_ATTACK_CADENCE: f64 = 1.14;
const MINION_STRUCTURE_AGGRO_RANGE: f64 = 0.05;
const MINION_STRUCTURE_BLOCKER_APPROACH_RANGE: f64 = 0.24;
const MINION_STRUCTURE_BLOCKER_ATTACK_RANGE: f64 = 0.13;
const MINION_CHAMPION_AGGRO_MIN_RANGE: f64 = 0.055;
const JUNGLE_INITIAL_SPAWN_AT: f64 = MINION_FIRST_WAVE_AT;
const SCUTTLE_INITIAL_SPAWN_AT: f64 = 210.0;
const JUNGLE_CAMP_ENGAGE_RADIUS: f64 = 0.09;
const JUNGLE_CAMP_ATTACK_RADIUS: f64 = 0.062;
const JUNGLE_STICKY_CAMP_RADIUS: f64 = 0.13;
const JUNGLE_CAMP_WAIT_FOR_SPAWN_SEC: f64 = 35.0;
const OBJECTIVE_ATTEMPT_RADIUS: f64 = 0.12;
const OBJECTIVE_ASSIST_RADIUS: f64 = 0.24;
const MAJOR_OBJECTIVE_TEAM_ASSIST_RADIUS: f64 = 0.52;
const BASE_DEFENSE_RECALL_DISTANCE: f64 = 0.34;
const NEXUS_DEFENSE_THREAT_RADIUS: f64 = 0.13;
const ALLY_HELP_RADIUS: f64 = 0.205;
const ALLY_HELP_DAMAGE_RECENT_SEC: f64 = 3.2;
const OFFROLE_JUNGLE_REWARD_MULTIPLIER: f64 = 0.65;
const JGL_JUNGLE_GOLD_MULTIPLIER: f64 = 0.78;
const JGL_JUNGLE_XP_MULTIPLIER: f64 = 0.90;
const OBJECTIVE_PATH_MIN_TARGET_DELTA: f64 = 0.014;
const JUNGLE_DISENGAGE_THREAT_AVOID_RADIUS: f64 = 0.1;
const VOIDGRUBS_SOFT_CLOSE_AT: f64 = 14.0 * 60.0 + 45.0;
const VOIDGRUBS_HARD_CLOSE_AT: f64 = 14.0 * 60.0 + 55.0;
const HERALD_SOFT_CLOSE_AT: f64 = 19.0 * 60.0 + 45.0;
const HERALD_HARD_CLOSE_AT: f64 = 19.0 * 60.0 + 55.0;
const DRAGON_SECURE_GOLD: i64 = 40;
const DRAGON_SECURE_XP: i64 = 90;
const BARON_SECURE_GOLD: i64 = 60;
const BARON_SECURE_XP: i64 = 120;
const OBJECTIVE_SECURE_GOLD: i64 = 45;
const OBJECTIVE_SECURE_XP: i64 = 90;
const VOIDGRUB_TOWER_DAMAGE_PER_STACK: f64 = 0.03;
const VOIDGRUB_TOWER_DAMAGE_MAX: f64 = 0.09;
const OBJECTIVE_NEXT_SPAWN_FALLBACK: f64 = 9_999_999.0;
const NAV_GRID_SIZE: usize = 196;
const NAV_PATH_MIN_DIRECT_DIST: f64 = 0.012;
const NAV_PATH_TRIVIAL_NODE_EPSILON: f64 = 0.0095;
const CHAMPION_REPATH_MIN_TARGET_DELTA: f64 = 0.018;
const CHAMPION_STUCK_PROGRESS_EPSILON: f64 = 0.00045;
const CHAMPION_STUCK_TRIGGER_SEC: f64 = 7.0;
const ITEM_COST_MULTIPLIER: f64 = 1.0;
const ITEM_COST_MIN: i64 = 300;
const SUPPORT_CS_MIN_INTERVAL_SEC: f64 = 24.0;
const MINION_XP_SHARE_RADIUS: f64 = 0.11;
const SUPPORT_ROAM_UNLOCK_AT_SEC: f64 = 15.0 * 60.0;
const SUPPORT_OPEN_ROAM_AT_SEC: f64 = 15.0 * 60.0;
const SUMMONER_FLASH_CD_SEC: f64 = 300.0;
const SUMMONER_IGNITE_CD_SEC: f64 = 180.0;
const SUMMONER_HEAL_CD_SEC: f64 = 240.0;
const SUMMONER_SMITE_CD_SEC: f64 = 90.0;
const SUMMONER_TP_CD_SEC: f64 = 300.0;
const SUMMONER_TP_UNLOCK_AT_SEC: f64 = 6.0 * 60.0;
const SUMMONER_FLASH_RANGE: f64 = 0.085;
const SUMMONER_IGNITE_RANGE: f64 = 0.072;
const SUMMONER_IGNITE_DURATION_SEC: f64 = 5.0;
const SUMMONER_IGNITE_DPS: f64 = 18.0;
const SUMMONER_HEAL_RADIUS: f64 = 0.085;
const SUMMONER_HEAL_SELF_RATIO: f64 = 0.22;
const SUMMONER_HEAL_ALLY_RATIO: f64 = 0.18;
const SUMMONER_SMITE_RANGE: f64 = 0.095;
const SUMMONER_SMITE_DAMAGE: f64 = 600.0;
const ULTIMATE_UNLOCK_LEVEL: i64 = 6;
const ULTIMATE_BASE_CD_SEC: f64 = 120.0;
const ULTIMATE_BURST_RANGE: f64 = 0.085;
const ULTIMATE_GLOBAL_RANGE: f64 = 0.18;
const ULTIMATE_MORDE_REALM_DURATION_SEC: f64 = 30.0;
const ULTIMATE_SUMMON_DAMAGE_RATIO: f64 = 0.45;
const ULTIMATE_SUMMON_HP_RATIO: f64 = 0.5;
const WARD_UNLOCK_AT_SEC: f64 = 90.0;
const WARD_DURATION_SEC: f64 = 95.0;
const WARD_COOLDOWN_SEC: f64 = 120.0;
const WARD_VISION_RADIUS: f64 = 0.18;
const CHAMPION_VISION_RADIUS: f64 = 0.145;
const MINION_VISION_RADIUS: f64 = 0.10;
const STRUCTURE_VISION_RADIUS: f64 = 0.16;
const SWEEPER_COOLDOWN_SEC: f64 = 95.0;
const SWEEPER_DURATION_SEC: f64 = 10.0;
const SWEEPER_CLEAR_RADIUS: f64 = 0.145;
const TRINKET_SWAP_UNLOCK_AT_SEC: f64 = 6.0 * 60.0;
const TRINKET_WARDING_TOTEM: &str = "WardingTotem";
const TRINKET_ORACLE_LENS: &str = "OracleLens";

fn summon_profile(champion_key: &str) -> (&'static str, f64, f64, f64) {
    match champion_key {
        "yorick" => ("maiden", 0.55, 0.50, 45.0),
        "annie" => ("tibbers", 0.50, 0.52, 45.0),
        "ivern" => ("daisy", 0.58, 0.44, 60.0),
        "shaco" => ("clone", 0.45, 0.48, 20.0),
        _ => (
            "summon",
            ULTIMATE_SUMMON_HP_RATIO,
            ULTIMATE_SUMMON_DAMAGE_RATIO,
            35.0,
        ),
    }
}

const LEVEL_XP_THRESHOLDS: [i64; 18] = [
    0, 280, 660, 1080, 1560, 2100, 2700, 3360, 4080, 4860, 5700, 6600, 7560, 8580, 9660, 10800,
    12000, 13260,
];

const TANK_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "sunfire_aegis",
        cost: 2700,
        attack_damage: 10.0,
        max_hp: 350.0,
    },
    ItemTemplate {
        key: "warmogs_armor",
        cost: 3100,
        attack_damage: 0.0,
        max_hp: 1000.0,
    },
    ItemTemplate {
        key: "iceborn_gauntlet",
        cost: 2900,
        attack_damage: 18.0,
        max_hp: 300.0,
    },
    ItemTemplate {
        key: "randuins_omen",
        cost: 3000,
        attack_damage: 0.0,
        max_hp: 350.0,
    },
    ItemTemplate {
        key: "spirit_visage",
        cost: 2900,
        attack_damage: 0.0,
        max_hp: 450.0,
    },
    ItemTemplate {
        key: "plated_steelcaps",
        cost: 1200,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const BRUISER_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "sundered_sky",
        cost: 3100,
        attack_damage: 40.0,
        max_hp: 300.0,
    },
    ItemTemplate {
        key: "deaths_dance",
        cost: 3300,
        attack_damage: 55.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "steraks_gage",
        cost: 3200,
        attack_damage: 32.0,
        max_hp: 450.0,
    },
    ItemTemplate {
        key: "titanic_hydra",
        cost: 3300,
        attack_damage: 42.0,
        max_hp: 550.0,
    },
    ItemTemplate {
        key: "maw_of_malmortius",
        cost: 3100,
        attack_damage: 50.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "mercurys_treads",
        cost: 1250,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const COLOSSUS_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "black_cleaver",
        cost: 3000,
        attack_damage: 40.0,
        max_hp: 400.0,
    },
    ItemTemplate {
        key: "steraks_gage",
        cost: 3200,
        attack_damage: 32.0,
        max_hp: 450.0,
    },
    ItemTemplate {
        key: "hullbreaker",
        cost: 3000,
        attack_damage: 40.0,
        max_hp: 500.0,
    },
    ItemTemplate {
        key: "titanic_hydra",
        cost: 3300,
        attack_damage: 42.0,
        max_hp: 550.0,
    },
    ItemTemplate {
        key: "dead_mans_plate",
        cost: 2900,
        attack_damage: 10.0,
        max_hp: 350.0,
    },
    ItemTemplate {
        key: "plated_steelcaps",
        cost: 1200,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const ASSASSIN_AD_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "voltaic_cyclosword",
        cost: 2900,
        attack_damage: 55.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "opportunity",
        cost: 2700,
        attack_damage: 55.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "immortal_shieldbow",
        cost: 3000,
        attack_damage: 50.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "seryldas_grudge",
        cost: 3200,
        attack_damage: 45.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "profane_hydra",
        cost: 3300,
        attack_damage: 60.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "boots_of_swiftness",
        cost: 1000,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const ASSASSIN_AP_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "stormsurge",
        cost: 2900,
        attack_damage: 36.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "lich_bane",
        cost: 3200,
        attack_damage: 32.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "shadowflame",
        cost: 3200,
        attack_damage: 35.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "zhonyas_hourglass",
        cost: 3250,
        attack_damage: 25.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "rabadons_deathcap",
        cost: 3600,
        attack_damage: 45.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "sorcerers_shoes",
        cost: 1100,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const CONTROL_MAGE_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "ludens_companion",
        cost: 2900,
        attack_damage: 35.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "void_staff",
        cost: 3000,
        attack_damage: 30.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "zhonyas_hourglass",
        cost: 3250,
        attack_damage: 25.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "seraphs_embrace",
        cost: 3000,
        attack_damage: 28.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "rabadons_deathcap",
        cost: 3600,
        attack_damage: 45.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "sorcerers_shoes",
        cost: 1100,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const BATTLE_MAGE_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "liandrys_torment",
        cost: 3000,
        attack_damage: 33.0,
        max_hp: 300.0,
    },
    ItemTemplate {
        key: "rylais_crystal_scepter",
        cost: 2600,
        attack_damage: 25.0,
        max_hp: 400.0,
    },
    ItemTemplate {
        key: "seraphs_embrace",
        cost: 3000,
        attack_damage: 28.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "cosmic_drive",
        cost: 3000,
        attack_damage: 30.0,
        max_hp: 350.0,
    },
    ItemTemplate {
        key: "zhonyas_hourglass",
        cost: 3250,
        attack_damage: 25.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "mercurys_treads",
        cost: 1250,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const ADC_CRIT_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "bloodthirster",
        cost: 3400,
        attack_damage: 70.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "infinity_edge",
        cost: 3400,
        attack_damage: 65.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "mortal_reminder",
        cost: 3200,
        attack_damage: 40.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "rapid_firecannon",
        cost: 2600,
        attack_damage: 24.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "phantom_dancer",
        cost: 2600,
        attack_damage: 24.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "berserkers_greaves",
        cost: 1100,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const ADC_ATTACK_SPEED_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "blade_of_the_ruined_king",
        cost: 3200,
        attack_damage: 42.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "wits_end",
        cost: 2900,
        attack_damage: 34.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "runaans_hurricane",
        cost: 2650,
        attack_damage: 24.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "guinsoos_rageblade",
        cost: 3000,
        attack_damage: 36.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "terminus",
        cost: 3000,
        attack_damage: 35.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "berserkers_greaves",
        cost: 1100,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const LETHALITY_MARKSMAN_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "the_collector",
        cost: 3100,
        attack_damage: 55.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "opportunity",
        cost: 2700,
        attack_damage: 55.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "seryldas_grudge",
        cost: 3200,
        attack_damage: 45.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "edge_of_night",
        cost: 3000,
        attack_damage: 50.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "profane_hydra",
        cost: 3300,
        attack_damage: 60.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "ionian_boots_of_lucidity",
        cost: 900,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const SUPPORT_ENGAGE_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "trailblazer",
        cost: 2400,
        attack_damage: 8.0,
        max_hp: 350.0,
    },
    ItemTemplate {
        key: "zekes_convergence",
        cost: 2200,
        attack_damage: 8.0,
        max_hp: 250.0,
    },
    ItemTemplate {
        key: "knights_vow",
        cost: 2300,
        attack_damage: 0.0,
        max_hp: 350.0,
    },
    ItemTemplate {
        key: "locket_of_the_iron_solari",
        cost: 2200,
        attack_damage: 0.0,
        max_hp: 250.0,
    },
    ItemTemplate {
        key: "thornmail",
        cost: 2450,
        attack_damage: 0.0,
        max_hp: 350.0,
    },
    ItemTemplate {
        key: "mobility_boots",
        cost: 1000,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const SUPPORT_ENCHANTER_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "shurelyas_battlesong",
        cost: 2200,
        attack_damage: 10.0,
        max_hp: 300.0,
    },
    ItemTemplate {
        key: "ardent_censer",
        cost: 2300,
        attack_damage: 18.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "moonstone_renewer",
        cost: 2200,
        attack_damage: 14.0,
        max_hp: 250.0,
    },
    ItemTemplate {
        key: "redemption",
        cost: 2300,
        attack_damage: 12.0,
        max_hp: 250.0,
    },
    ItemTemplate {
        key: "staff_of_flowing_water",
        cost: 2250,
        attack_damage: 18.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "ionian_boots_of_lucidity",
        cost: 900,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

const SUPPORT_DAMAGE_ITEM_PLAN: [ItemTemplate; 6] = [
    ItemTemplate {
        key: "rylais_crystal_scepter",
        cost: 2600,
        attack_damage: 25.0,
        max_hp: 400.0,
    },
    ItemTemplate {
        key: "liandrys_torment",
        cost: 3000,
        attack_damage: 33.0,
        max_hp: 300.0,
    },
    ItemTemplate {
        key: "morellonomicon",
        cost: 2950,
        attack_damage: 28.0,
        max_hp: 350.0,
    },
    ItemTemplate {
        key: "zhonyas_hourglass",
        cost: 3250,
        attack_damage: 25.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "cryptbloom",
        cost: 2850,
        attack_damage: 27.0,
        max_hp: 0.0,
    },
    ItemTemplate {
        key: "sorcerers_shoes",
        cost: 1100,
        attack_damage: 0.0,
        max_hp: 0.0,
    },
];

fn create_champions(
    seed: &str,
    snapshot: &Value,
    champion_by_player_id: &HashMap<String, String>,
    champion_profiles_by_id: &HashMap<String, LolChampionCombatProfileInput>,
    champion_ultimates_by_id: &HashMap<String, LolChampionUltimateInput>,
) -> Vec<Value> {
    let mut rng = Mulberry32::new(hash_seed(seed));
    let mut champions = Vec::new();

    let home_players = snapshot_team_players(snapshot, "home_team");
    let away_players = snapshot_team_players(snapshot, "away_team");
    let home_tactics = extract_runtime_team_tactics(snapshot, "home", "home_team");
    let away_tactics = extract_runtime_team_tactics(snapshot, "away", "away_team");

    seed_team(
        &mut champions,
        &home_players,
        "home",
        "blue",
        BASE_POSITION_BLUE,
        &home_tactics,
        snapshot,
        champion_by_player_id,
        champion_profiles_by_id,
        champion_ultimates_by_id,
        &mut rng,
    );

    seed_team(
        &mut champions,
        &away_players,
        "away",
        "red",
        BASE_POSITION_RED,
        &away_tactics,
        snapshot,
        champion_by_player_id,
        champion_profiles_by_id,
        champion_ultimates_by_id,
        &mut rng,
    );

    champions
}

fn seed_team(
    champions: &mut Vec<Value>,
    players: &[SnapshotPlayer],
    side_key: &str,
    team: &str,
    base_pos: Vec2,
    team_tactics: &RuntimeTeamTactics,
    snapshot: &Value,
    champion_by_player_id: &HashMap<String, String>,
    champion_profiles_by_id: &HashMap<String, LolChampionCombatProfileInput>,
    champion_ultimates_by_id: &HashMap<String, LolChampionUltimateInput>,
    rng: &mut Mulberry32,
) {
    for (index, player) in players.iter().take(5).enumerate() {
        let Some(role_seed) = ROLE_SEEDS.get(index) else {
            break;
        };

        let champion_id = champion_by_player_id.get(&player.id);
        let profile = champion_id.and_then(|id| champion_profiles_by_id.get(id));
        let attack_type = profile
            .map(|p| normalize_attack_type(&p.attack_type))
            .unwrap_or("melee");
        let max_hp = champion_max_hp_from_base(profile.map(|p| p.base_hp).unwrap_or(560.0));
        let attack_range = profile
            .map(|p| p.attack_range)
            .unwrap_or(if attack_type == "ranged" {
                0.056
            } else {
                0.049
            });
        let role_impact = extract_runtime_role_impact(snapshot, side_key, &player.id);
        let role_modifier = role_impact
            .as_ref()
            .map(|impact| impact.modifier.clamp(-4.0, 4.0))
            .unwrap_or(0.0);
        let tuned_role_modifier = if role_seed.role == "JGL" {
            role_modifier * 0.65
        } else {
            role_modifier
        };
        let role_variance = role_impact
            .as_ref()
            .map(|impact| impact.variance.clamp(0.5, 4.5))
            .unwrap_or(1.0);
        let staff_effects = extract_runtime_staff_effects(snapshot, side_key);
        let scrim_prep = extract_runtime_scrim_prep(snapshot, side_key);
        let scrim_preparation = scrim_prep.preparation.clamp(0.0, 3.0);
        let scrim_comfort = scrim_prep
            .comfort_by_player
            .get(&player.id)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 2.0);
        let scrim_focus = scrim_prep
            .focus
            .as_deref()
            .map(normalize_champion_key)
            .unwrap_or_default();
        let scrim_execution_bonus =
            (scrim_preparation * 0.006 + scrim_comfort * 0.005).clamp(0.0, 0.026);
        let scrim_gameplay_bonus = if scrim_focus == "teamfighting" || scrim_focus == "earlygame" {
            scrim_preparation * 0.004
        } else {
            0.0
        };
        let scrim_iq_bonus = if scrim_focus == "macro" || scrim_focus == "draftprep" {
            scrim_preparation * 0.006
        } else {
            0.0
        };
        let scrim_mental_bonus = if scrim_focus == "mental" {
            scrim_preparation * 0.005
        } else {
            0.0
        };
        let staff_execution = staff_effects.execution.clamp(0.96, 1.10);
        let staff_tactics_modifier = ((staff_effects.tactics - 1.0) * 1.2
            + (staff_effects.analysis - 1.0) * 0.8)
            .clamp(-0.18, 0.24);

        let (
            mechanics,
            laning,
            teamfighting,
            macro_stat,
            consistency,
            shotcalling,
            champion_pool,
            discipline,
            mental_resilience,
        ) = player_visible_stats(player);

        let gameplay_score = (mechanics + laning + teamfighting) / 3.0;
        let iq_score = (macro_stat + consistency + shotcalling) / 3.0;
        let competitive_score = (champion_pool + discipline + mental_resilience) / 3.0;

        let gameplay_delta = stat_delta(gameplay_score);
        let iq_delta = stat_delta(iq_score);
        let competitive_delta = stat_delta(competitive_score);
        let mechanics_delta = stat_delta(mechanics);
        let laning_delta = stat_delta(laning);
        let teamfighting_delta = stat_delta(teamfighting);
        let consistency_delta = stat_delta(consistency);
        let discipline_delta = stat_delta(discipline);
        let champion_pool_delta = stat_delta(champion_pool);

        let max_hp = (max_hp
            * (1.0
                + tuned_role_modifier * 0.012
                + competitive_delta * 0.04
                + teamfighting_delta * 0.02
                + scrim_mental_bonus))
            .clamp(120.0, 340.0);
        let attack_damage = (14.0 + rng.next_f64() * 5.0)
            * (1.0
                + tuned_role_modifier * 0.016
                + gameplay_delta * 0.06
                + mechanics_delta * 0.03
                + staff_tactics_modifier * 0.015
                + scrim_execution_bonus
                + scrim_gameplay_bonus);
        let move_speed = (0.043
            + rng.next_f64() * 0.008
            + (tuned_role_modifier * 0.00035)
            + iq_delta * 0.001
            + laning_delta * 0.0006
            + staff_tactics_modifier * 0.0004
            + scrim_iq_bonus * 0.0007)
            .clamp(0.036, 0.062);

        let spawn_pos = Vec2 {
            x: base_pos.x + role_seed.offset.x,
            y: base_pos.y + role_seed.offset.y,
        };

        let jgl_start = if role_seed.role == "JGL" {
            if normalized_team(team) == "blue" {
                if team_tactics.jungle_pathing == "BotToTop" {
                    Vec2 {
                        x: 0.568359375,
                        y: 0.828125,
                    }
                } else {
                    Vec2 {
                        x: 0.14908854166666666,
                        y: 0.43359375,
                    }
                }
            } else if team_tactics.jungle_pathing == "BotToTop" {
                Vec2 {
                    x: 0.8483072916666666,
                    y: 0.56640625,
                }
            } else {
                Vec2 {
                    x: 0.4381510416666667,
                    y: 0.16536458333333334,
                }
            }
        } else {
            spawn_pos
        };

        let initial_target_path = if role_seed.role == "JGL" {
            vec![json!({ "x": jgl_start.x, "y": jgl_start.y })]
        } else {
            Vec::new()
        };
        let initial_state = if role_seed.role == "JGL" {
            "jungle"
        } else {
            "lane"
        };
        let consistency_factor =
            (1.0 - consistency_delta * 0.26 - discipline_delta * 0.12 - champion_pool_delta * 0.08)
                .clamp(0.65, 1.35);
        let decision_jitter = (((role_variance - 1.0).max(0.0) * 0.35) + rng.next_f64() * 0.08)
            * consistency_factor
            / (staff_execution * (1.0 + scrim_preparation * 0.012 + scrim_comfort * 0.01));
        let initial_next_decision_at = if role_seed.role == "JGL" {
            6.0 + decision_jitter
        } else {
            decision_jitter
        };
        let summoner_spells = default_summoner_spells_for_role(role_seed.role);
        let ultimate = champion_id
            .and_then(|id| champion_ultimates_by_id.get(id))
            .map(|slot| {
                json!({
                    "archetype": slot.archetype,
                    "icon": slot.icon,
                    "cdUntil": 0.0,
                })
            })
            .unwrap_or_else(|| {
                json!({
                    "archetype": default_ultimate_archetype_for_role(role_seed.role),
                    "icon": "",
                    "cdUntil": 0.0,
                })
            });

        // Keep this object built manually instead of one huge `json!` call.
        // The champion runtime payload is large enough that serde_json's macro can
        // hit the crate recursion limit when new fields are added.
        let mut champion_obj = Map::new();
        champion_obj.insert("id".to_string(), Value::from(player.id.clone()));
        champion_obj.insert("name".to_string(), Value::from(player.name.clone()));
        champion_obj.insert(
            "championId".to_string(),
            Value::from(champion_id.cloned().unwrap_or_default()),
        );
        champion_obj.insert("team".to_string(), Value::from(team));
        champion_obj.insert("role".to_string(), Value::from(role_seed.role));
        champion_obj.insert("lane".to_string(), Value::from(role_seed.lane));
        champion_obj.insert(
            "pos".to_string(),
            json!({
                "x": spawn_pos.x,
                "y": spawn_pos.y,
            }),
        );
        champion_obj.insert("hp".to_string(), Value::from(max_hp));
        champion_obj.insert("maxHp".to_string(), Value::from(max_hp));
        champion_obj.insert("alive".to_string(), Value::from(true));
        champion_obj.insert("respawnAt".to_string(), Value::from(0.0));
        champion_obj.insert("attackCdUntil".to_string(), Value::from(0.0));
        champion_obj.insert("moveSpeed".to_string(), Value::from(move_speed));
        champion_obj.insert("attackRange".to_string(), Value::from(attack_range));
        champion_obj.insert("attackType".to_string(), Value::from(attack_type));
        champion_obj.insert("attackDamage".to_string(), Value::from(attack_damage));
        champion_obj.insert("targetPath".to_string(), Value::Array(initial_target_path));
        champion_obj.insert("targetPathIndex".to_string(), Value::from(0));
        champion_obj.insert(
            "nextDecisionAt".to_string(),
            Value::from(initial_next_decision_at),
        );
        champion_obj.insert("kills".to_string(), Value::from(0));
        champion_obj.insert("deaths".to_string(), Value::from(0));
        champion_obj.insert("assists".to_string(), Value::from(0));
        champion_obj.insert("gold".to_string(), Value::from(500));
        champion_obj.insert("spentGold".to_string(), Value::from(0));
        champion_obj.insert("xp".to_string(), Value::from(0));
        champion_obj.insert("level".to_string(), Value::from(1));
        champion_obj.insert("cs".to_string(), Value::from(0));
        champion_obj.insert("hasLeftBaseOnce".to_string(), Value::from(false));
        champion_obj.insert("lastSupportCsAt".to_string(), Value::from(-999.0));
        champion_obj.insert("items".to_string(), Value::Array(Vec::new()));
        champion_obj.insert("gameplayScore".to_string(), Value::from(gameplay_score));
        champion_obj.insert("iqScore".to_string(), Value::from(iq_score));
        champion_obj.insert(
            "competitiveScore".to_string(),
            Value::from(competitive_score),
        );
        champion_obj.insert("staffExecution".to_string(), Value::from(staff_execution));
        champion_obj.insert("summonerSpells".to_string(), Value::Array(summoner_spells));
        champion_obj.insert("igniteDotUntil".to_string(), Value::from(0.0));
        champion_obj.insert("igniteSourceId".to_string(), Value::Null);
        champion_obj.insert("lastDamagedByChampionId".to_string(), Value::Null);
        champion_obj.insert("lastDamagedAt".to_string(), Value::from(-999.0));
        champion_obj.insert("state".to_string(), Value::from(initial_state));
        champion_obj.insert("recallAnchor".to_string(), Value::Null);
        champion_obj.insert("recallChannelUntil".to_string(), Value::from(0.0));

        let mut champion_json = Value::Object(champion_obj);

        if let Some(obj) = champion_json.as_object_mut() {
            obj.insert("ultimate".to_string(), ultimate);
            obj.insert("realmBanishedUntil".to_string(), Value::from(0.0));
            obj.insert("realmReturnPos".to_string(), Value::Null);
            obj.insert("wardCdUntil".to_string(), Value::from(0.0));
            obj.insert("sweeperCdUntil".to_string(), Value::from(0.0));
            obj.insert("sweeperActiveUntil".to_string(), Value::from(0.0));
            obj.insert("trinketKey".to_string(), Value::from(TRINKET_WARDING_TOTEM));
            obj.insert("trinketSwapped".to_string(), Value::from(false));
            obj.insert("supportRoamUses".to_string(), Value::from(0));
            obj.insert("supportRoamCdUntil".to_string(), Value::from(0.0));
            obj.insert("supportLastRoamRole".to_string(), Value::from(""));
        }

        champions.push(champion_json);
    }
}

fn default_summoner_spells_for_role(role: &str) -> Vec<Value> {
    let keys: [&str; 2] = match role {
        "JGL" => ["Smite", "Flash"],
        "TOP" => ["Teleport", "Flash"],
        "MID" => ["Ignite", "Flash"],
        "ADC" => ["Heal", "Flash"],
        _ => ["Ignite", "Flash"],
    };
    keys.iter()
        .map(|key| json!({ "key": key, "cdUntil": 0.0 }))
        .collect()
}

fn default_ultimate_archetype_for_role(role: &str) -> &'static str {
    match role {
        "TOP" => "engage",
        "JGL" => "global",
        "MID" => "burst",
        "ADC" => "execute",
        _ => "utility",
    }
}

fn neutral_timer_templates() -> Vec<NeutralTimerTemplate> {
    vec![
        NeutralTimerTemplate {
            key: "blue-buff-blue",
            label: "Blue Blue Buff",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 470.0,
            respawn_delay_sec: Some(300.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.24934895833333334,
                y: 0.4622395833333333,
            },
        },
        NeutralTimerTemplate {
            key: "blue-buff-red",
            label: "Red Blue Buff",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 470.0,
            respawn_delay_sec: Some(300.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.478515625,
                y: 0.26171875,
            },
        },
        NeutralTimerTemplate {
            key: "red-buff-blue",
            label: "Blue Red Buff",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 500.0,
            respawn_delay_sec: Some(300.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.5266927083333334,
                y: 0.7421875,
            },
        },
        NeutralTimerTemplate {
            key: "red-buff-red",
            label: "Red Red Buff",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 500.0,
            respawn_delay_sec: Some(300.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.7545572916666666,
                y: 0.5403645833333334,
            },
        },
        NeutralTimerTemplate {
            key: "wolves-blue",
            label: "Blue Wolves",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 380.0,
            respawn_delay_sec: Some(135.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.2584635416666667,
                y: 0.56640625,
            },
        },
        NeutralTimerTemplate {
            key: "wolves-red",
            label: "Red Wolves",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 380.0,
            respawn_delay_sec: Some(135.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.525390625,
                y: 0.3528645833333333,
            },
        },
        NeutralTimerTemplate {
            key: "raptors-blue",
            label: "Blue Raptors",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 390.0,
            respawn_delay_sec: Some(135.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.4759114583333333,
                y: 0.6432291666666666,
            },
        },
        NeutralTimerTemplate {
            key: "raptors-red",
            label: "Red Raptors",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 390.0,
            respawn_delay_sec: Some(135.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.748046875,
                y: 0.4361979166666667,
            },
        },
        NeutralTimerTemplate {
            key: "gromp-blue",
            label: "Blue Gromp",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 520.0,
            respawn_delay_sec: Some(135.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.14908854166666666,
                y: 0.43359375,
            },
        },
        NeutralTimerTemplate {
            key: "gromp-red",
            label: "Red Gromp",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 520.0,
            respawn_delay_sec: Some(135.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.4381510416666667,
                y: 0.16536458333333334,
            },
        },
        NeutralTimerTemplate {
            key: "krugs-blue",
            label: "Blue Krugs",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 560.0,
            respawn_delay_sec: Some(135.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.568359375,
                y: 0.828125,
            },
        },
        NeutralTimerTemplate {
            key: "krugs-red",
            label: "Red Krugs",
            first_spawn_at: JUNGLE_INITIAL_SPAWN_AT,
            max_hp: 560.0,
            respawn_delay_sec: Some(135.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.8483072916666666,
                y: 0.56640625,
            },
        },
        NeutralTimerTemplate {
            key: "scuttle-top",
            label: "Scuttle Top",
            first_spawn_at: SCUTTLE_INITIAL_SPAWN_AT,
            max_hp: 560.0,
            respawn_delay_sec: Some(150.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.2845052083333333,
                y: 0.34765625,
            },
        },
        NeutralTimerTemplate {
            key: "scuttle-bot",
            label: "Scuttle Bot",
            first_spawn_at: SCUTTLE_INITIAL_SPAWN_AT,
            max_hp: 560.0,
            respawn_delay_sec: Some(150.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.6998697916666666,
                y: 0.6419270833333334,
            },
        },
        NeutralTimerTemplate {
            key: "dragon",
            label: "Dragon",
            first_spawn_at: 5.0 * 60.0,
            max_hp: 3600.0,
            respawn_delay_sec: Some(5.0 * 60.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.673828125,
                y: 0.703125,
            },
        },
        NeutralTimerTemplate {
            key: "voidgrubs",
            label: "Voidgrubs",
            first_spawn_at: 8.0 * 60.0,
            max_hp: 2800.0,
            respawn_delay_sec: None,
            one_shot: true,
            window_close_at: Some(VOIDGRUBS_SOFT_CLOSE_AT),
            combat_grace_until: Some(VOIDGRUBS_HARD_CLOSE_AT),
            unlocked: true,
            pos: Vec2 {
                x: 0.3274739583333333,
                y: 0.2981770833333333,
            },
        },
        NeutralTimerTemplate {
            key: "herald",
            label: "Rift Herald",
            first_spawn_at: 15.0 * 60.0,
            max_hp: 5500.0,
            respawn_delay_sec: None,
            one_shot: true,
            window_close_at: Some(HERALD_SOFT_CLOSE_AT),
            combat_grace_until: Some(HERALD_HARD_CLOSE_AT),
            unlocked: true,
            pos: Vec2 {
                x: 0.3274739583333333,
                y: 0.2981770833333333,
            },
        },
        NeutralTimerTemplate {
            key: "baron",
            label: "Baron",
            first_spawn_at: 20.0 * 60.0,
            max_hp: 9000.0,
            respawn_delay_sec: Some(6.0 * 60.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: true,
            pos: Vec2 {
                x: 0.3274739583333333,
                y: 0.2981770833333333,
            },
        },
        NeutralTimerTemplate {
            key: "elder",
            label: "Elder Dragon",
            first_spawn_at: 0.0,
            max_hp: 7200.0,
            respawn_delay_sec: Some(6.0 * 60.0),
            one_shot: false,
            window_close_at: None,
            combat_grace_until: None,
            unlocked: false,
            pos: Vec2 {
                x: 0.673828125,
                y: 0.703125,
            },
        },
    ]
}

fn snapshot_team_players(snapshot: &Value, team_key: &str) -> Vec<SnapshotPlayer> {
    snapshot
        .get(team_key)
        .and_then(Value::as_object)
        .and_then(|team| team.get("players"))
        .and_then(Value::as_array)
        .map(|players| {
            players
                .iter()
                .filter_map(|player| {
                    let id = player.get("id").and_then(Value::as_str)?.to_string();
                    let name = player
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(&id)
                        .to_string();
                    let stat = |key: &str| {
                        player
                            .get(key)
                            .and_then(Value::as_f64)
                            .unwrap_or(70.0)
                            .clamp(1.0, 99.0)
                    };
                    Some(SnapshotPlayer {
                        id,
                        name,
                        dribbling: stat("dribbling"),
                        agility: stat("agility"),
                        pace: stat("pace"),
                        composure: stat("composure"),
                        shooting: stat("shooting"),
                        positioning: stat("positioning"),
                        teamwork: stat("teamwork"),
                        stamina: stat("stamina"),
                        decisions: stat("decisions"),
                        vision: stat("vision"),
                        passing: stat("passing"),
                        leadership: stat("leadership"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn avg4(a: f64, b: f64, c: f64, d: f64) -> f64 {
    (a + b + c + d) / 4.0
}

fn player_visible_stats(player: &SnapshotPlayer) -> (f64, f64, f64, f64, f64, f64, f64, f64, f64) {
    let mechanics = avg4(
        player.dribbling,
        player.agility,
        player.pace,
        player.composure,
    );
    let laning = avg4(
        player.shooting,
        player.positioning,
        player.dribbling,
        player.composure,
    );
    let teamfighting = avg4(
        player.teamwork,
        player.stamina,
        player.decisions,
        player.composure,
    );
    let macro_stat = avg4(
        player.vision,
        player.decisions,
        player.positioning,
        player.passing,
    );
    let consistency = avg4(
        player.decisions,
        player.vision,
        player.composure,
        player.teamwork,
    );
    let shotcalling = avg4(
        player.leadership,
        player.teamwork,
        player.vision,
        player.decisions,
    );
    let champion_pool = avg4(
        player.dribbling,
        player.agility,
        player.vision,
        player.passing,
    );
    let discipline = avg4(
        player.decisions,
        player.composure,
        player.teamwork,
        player.leadership,
    );
    let mental_resilience = avg4(
        player.composure,
        player.teamwork,
        player.leadership,
        player.stamina,
    );
    (
        mechanics,
        laning,
        teamfighting,
        macro_stat,
        consistency,
        shotcalling,
        champion_pool,
        discipline,
        mental_resilience,
    )
}

fn stat_delta(score: f64) -> f64 {
    ((score - 70.0) / 30.0).clamp(-1.0, 1.0)
}

fn champion_micro_damage_multiplier(champion: &ChampionRuntime) -> f64 {
    let gameplay = stat_delta(champion.gameplay_score);
    (1.0 + gameplay * 0.07).clamp(0.84, 1.10)
}

fn champion_lane_damage_multiplier(champion: &ChampionRuntime) -> f64 {
    let gameplay = stat_delta(champion.gameplay_score);
    (1.0 + gameplay * 0.11).clamp(0.86, 1.18)
}

fn champion_structure_focus_multiplier(champion: &ChampionRuntime) -> f64 {
    let iq_delta = stat_delta(champion.iq_score);
    (1.08 + iq_delta * 0.08).clamp(0.94, 1.22)
}

fn extract_runtime_team_tactics(
    snapshot: &Value,
    side_key: &str,
    team_key: &str,
) -> RuntimeTeamTactics {
    let from_root = snapshot
        .get("lol_tactics")
        .and_then(Value::as_object)
        .and_then(|obj| obj.get(side_key))
        .cloned();
    let from_team = snapshot
        .get(team_key)
        .and_then(Value::as_object)
        .and_then(|obj| obj.get("lol_tactics"))
        .cloned();

    let payload = from_root.or(from_team);
    payload
        .and_then(|value| serde_json::from_value::<RuntimeTeamTactics>(value).ok())
        .unwrap_or_default()
}

fn extract_runtime_role_impact(
    snapshot: &Value,
    side_key: &str,
    player_id: &str,
) -> Option<RuntimeRoleImpact> {
    snapshot
        .get("lol_role_impact_by_player")
        .and_then(Value::as_object)
        .and_then(|obj| obj.get(side_key))
        .and_then(Value::as_object)
        .and_then(|by_player| by_player.get(player_id))
        .cloned()
        .and_then(|value| serde_json::from_value::<RuntimeRoleImpact>(value).ok())
}

fn extract_runtime_staff_effects(snapshot: &Value, side_key: &str) -> RuntimeStaffEffects {
    snapshot
        .get("lol_staff_effects")
        .and_then(Value::as_object)
        .and_then(|obj| obj.get(side_key))
        .cloned()
        .and_then(|value| serde_json::from_value::<RuntimeStaffEffects>(value).ok())
        .unwrap_or(RuntimeStaffEffects {
            execution: 1.0,
            tactics: 1.0,
            analysis: 1.0,
        })
}

fn extract_runtime_scrim_prep(snapshot: &Value, side_key: &str) -> RuntimeScrimPrepSide {
    snapshot
        .get("lol_scrim_prep")
        .and_then(Value::as_object)
        .and_then(|obj| obj.get(side_key))
        .cloned()
        .and_then(|value| serde_json::from_value::<RuntimeScrimPrepSide>(value).ok())
        .unwrap_or_default()
}

fn team_tactics_for_runtime(team_tactics: Option<&Value>, team: &str) -> RuntimeTeamTactics {
    team_tactics
        .and_then(Value::as_object)
        .and_then(|obj| obj.get(normalized_team(team)))
        .cloned()
        .and_then(|value| serde_json::from_value::<RuntimeTeamTactics>(value).ok())
        .unwrap_or_default()
}

fn team_buffs_for_runtime(team_buffs: Option<&Value>, team: &str) -> RuntimeTeamBuffState {
    team_buffs
        .and_then(Value::as_object)
        .and_then(|obj| obj.get(normalized_team(team)))
        .cloned()
        .and_then(|value| serde_json::from_value::<RuntimeTeamBuffState>(value).ok())
        .unwrap_or_default()
}

fn runtime_buffs_from_extra(team_buffs: Option<&Value>) -> RuntimeBuffState {
    team_buffs
        .cloned()
        .and_then(|value| serde_json::from_value::<RuntimeBuffState>(value).ok())
        .unwrap_or_default()
}

fn set_runtime_buffs(runtime: &mut RuntimeState, buffs: &RuntimeBuffState) {
    if let Ok(value) = serde_json::to_value(buffs) {
        runtime.extra.insert("teamBuffs".to_string(), value);
    }
}

fn team_buffs_mut<'a>(buffs: &'a mut RuntimeBuffState, team: &str) -> &'a mut RuntimeTeamBuffState {
    if normalized_team(team) == "red" {
        &mut buffs.red
    } else {
        &mut buffs.blue
    }
}

fn team_buffs_ref<'a>(buffs: &'a RuntimeBuffState, team: &str) -> &'a RuntimeTeamBuffState {
    if normalized_team(team) == "red" {
        &buffs.red
    } else {
        &buffs.blue
    }
}

fn normalize_attack_type(raw: &str) -> &'static str {
    if raw.eq_ignore_ascii_case("ranged") {
        "ranged"
    } else {
        "melee"
    }
}

fn champion_max_hp_from_base(base_hp: f64) -> f64 {
    (base_hp / 4.0).round().clamp(120.0, 240.0)
}

fn hash_seed(seed: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for ch in seed.encode_utf16() {
        h ^= ch as u32;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

struct Mulberry32 {
    a: u32,
}

impl Mulberry32 {
    fn new(seed: u32) -> Self {
        Self { a: seed }
    }

    fn next_f64(&mut self) -> f64 {
        self.a = self.a.wrapping_add(0x6d2b79f5);
        let mut t = self.a;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4_294_967_296.0
    }
}

fn wave_interval_sec(at_time_sec: f64) -> f64 {
    if at_time_sec < 14.0 * 60.0 {
        30.0
    } else if at_time_sec < 30.0 * 60.0 {
        25.0
    } else {
        20.0
    }
}

fn normalized_lane(lane: &str) -> &'static str {
    match lane {
        "top" => "top",
        "mid" => "mid",
        "bot" => "bot",
        _ => "mid",
    }
}

fn normalized_team(team: &str) -> &'static str {
    if team == "red" { "red" } else { "blue" }
}

fn lane_path_blue(lane: &str) -> &'static [Vec2] {
    match normalized_lane(lane) {
        "top" => &LANE_PATH_TOP_BLUE,
        "bot" => &LANE_PATH_BOT_BLUE,
        _ => &LANE_PATH_MID_BLUE,
    }
}

fn lane_path_for(team: &str, lane: &str) -> Vec<Vec2> {
    let mut path = lane_path_blue(lane).to_vec();
    if normalized_team(team) == "red" {
        path.reverse();
    }
    path
}

fn base_position_for(team: &str) -> Vec2 {
    if normalized_team(team) == "red" {
        BASE_POSITION_RED
    } else {
        BASE_POSITION_BLUE
    }
}

fn active_nav_walls() -> &'static [WallPolygon] {
    static WALLS: OnceLock<Vec<WallPolygon>> = OnceLock::new();
    WALLS
        .get_or_init(|| {
            let raw = include_str!("./engine/sim_background/lol_walls.json");
            let Ok(file) = serde_json::from_str::<WallFile>(raw) else {
                return Vec::new();
            };
            file.walls
                .into_iter()
                .filter(|wall| wall.closed && wall.points.len() >= 3 && !wall.id.is_empty())
                .collect()
        })
        .as_slice()
}

fn nav_grid() -> &'static NavGrid {
    static NAV: OnceLock<NavGrid> = OnceLock::new();
    NAV.get_or_init(|| NavGrid::new(active_nav_walls(), NAV_GRID_SIZE))
}

impl NavGrid {
    fn new(walls: &[WallPolygon], grid_size: usize) -> Self {
        let mut blocked = vec![0u8; grid_size * grid_size];
        for y in 0..grid_size {
            for x in 0..grid_size {
                let p = Vec2 {
                    x: Self::to_norm_with_size(x, grid_size),
                    y: Self::to_norm_with_size(y, grid_size),
                };
                let is_blocked = walls.iter().any(|w| point_in_polygon(p, &w.points));
                blocked[y * grid_size + x] = if is_blocked { 1 } else { 0 };
            }
        }

        Self { grid_size, blocked }
    }

    fn idx(&self, cx: usize, cy: usize) -> usize {
        cy * self.grid_size + cx
    }

    fn in_bounds(&self, cx: isize, cy: isize) -> bool {
        cx >= 0 && cy >= 0 && cx < self.grid_size as isize && cy < self.grid_size as isize
    }

    fn is_blocked_cell(&self, cx: usize, cy: usize) -> bool {
        self.blocked[self.idx(cx, cy)] == 1
    }

    fn to_cell_with_size(v: f64, grid_size: usize) -> usize {
        let scaled = (v * grid_size as f64).floor();
        clamp(scaled, 0.0, grid_size.saturating_sub(1) as f64) as usize
    }

    fn to_cell(&self, v: f64) -> usize {
        Self::to_cell_with_size(v, self.grid_size)
    }

    fn to_norm_with_size(c: usize, grid_size: usize) -> f64 {
        (c as f64 + 0.5) / grid_size as f64
    }

    fn to_norm(&self, c: usize) -> f64 {
        Self::to_norm_with_size(c, self.grid_size)
    }

    fn nearest_free_cell(&self, cx: usize, cy: usize) -> GridCell {
        if !self.is_blocked_cell(cx, cy) {
            return GridCell { cx, cy };
        }

        let mut queue = VecDeque::new();
        let mut seen = vec![false; self.grid_size * self.grid_size];
        let start_idx = self.idx(cx, cy);
        queue.push_back(GridCell { cx, cy });
        seen[start_idx] = true;

        let dirs: [(isize, isize); 8] = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ];

        while let Some(cur) = queue.pop_front() {
            if !self.is_blocked_cell(cur.cx, cur.cy) {
                return cur;
            }

            for (dx, dy) in dirs {
                let nx = cur.cx as isize + dx;
                let ny = cur.cy as isize + dy;
                if !self.in_bounds(nx, ny) {
                    continue;
                }
                let nx = nx as usize;
                let ny = ny as usize;
                let idx = self.idx(nx, ny);
                if seen[idx] {
                    continue;
                }
                seen[idx] = true;
                queue.push_back(GridCell { cx: nx, cy: ny });
            }
        }

        GridCell { cx, cy }
    }

    fn has_line_of_sight(&self, a: Vec2, b: Vec2) -> bool {
        let mut x0 = self.to_cell(a.x) as isize;
        let mut y0 = self.to_cell(a.y) as isize;
        let x1 = self.to_cell(b.x) as isize;
        let y1 = self.to_cell(b.y) as isize;

        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;

        loop {
            if self.is_blocked_cell(x0 as usize, y0 as usize) {
                return false; // Line of sight is blocked by a wall
            }
            if x0 == x1 && y0 == y1 {
                break; // Arrived at the target cell
            }

            let e2 = 2 * err;

            // Strictly check adjacent cells for diagonal movement to prevent corner-cutting through walls
            if e2 > -dy && e2 < dx {
                if self.is_blocked_cell((x0 + sx) as usize, y0 as usize)
                    || self.is_blocked_cell(x0 as usize, (y0 + sy) as usize)
                {
                    return false;
                }
            }

            if e2 > -dy {
                err -= dy;
                x0 += sx;
            }
            if e2 < dx {
                err += dx;
                y0 += sy;
            }
        }
        true
    }

    fn smooth_path(&self, path: Vec<Vec2>) -> Vec<Vec2> {
        if path.len() <= 2 {
            return path;
        }

        let mut out = vec![path[0]];
        let mut i = 0usize;
        while i < path.len().saturating_sub(1) {
            let mut j = path.len().saturating_sub(1);
            while j > i + 1 {
                if self.has_line_of_sight(path[i], path[j]) {
                    break;
                }
                j = j.saturating_sub(1);
            }
            out.push(path[j]);
            i = j;
        }
        out
    }

    fn find_path(&self, start: Vec2, end: Vec2) -> Vec<Vec2> {
        let s = self.nearest_free_cell(self.to_cell(start.x), self.to_cell(start.y));
        let e = self.nearest_free_cell(self.to_cell(end.x), self.to_cell(end.y));

        let total = self.grid_size * self.grid_size;
        let mut g_score = vec![f64::INFINITY; total];
        let mut parent = vec![usize::MAX; total];
        let mut closed = vec![false; total];
        let mut open: BinaryHeap<OpenNode> = BinaryHeap::new();

        let start_idx = self.idx(s.cx, s.cy);
        let end_idx = self.idx(e.cx, e.cy);

        g_score[start_idx] = 0.0;

        let heuristic = |idx: usize| -> f64 {
            let cx = idx % self.grid_size;
            let cy = idx / self.grid_size;
            ((e.cx as f64 - cx as f64).powi(2) + (e.cy as f64 - cy as f64).powi(2)).sqrt()
        };
        let start_h = heuristic(start_idx);
        open.push(OpenNode {
            idx: start_idx,
            f_score: start_h,
            h_score: start_h,
        });

        let dirs: [(isize, isize, f64); 8] = [
            (1, 0, 1.0),
            (-1, 0, 1.0),
            (0, 1, 1.0),
            (0, -1, 1.0),
            (1, 1, 1.414),
            (-1, -1, 1.414),
            (1, -1, 1.414),
            (-1, 1, 1.414),
        ];

        while let Some(node) = open.pop() {
            let current = node.idx;
            if closed[current] {
                continue;
            }
            if current == end_idx {
                let mut cell_path = Vec::new();
                let mut at = current;
                loop {
                    let cx = at % self.grid_size;
                    let cy = at / self.grid_size;
                    cell_path.push(Vec2 {
                        x: self.to_norm(cx),
                        y: self.to_norm(cy),
                    });
                    let p = parent[at];
                    if p == usize::MAX {
                        break;
                    }
                    at = p;
                }
                cell_path.reverse();
                return self.smooth_path(cell_path);
            }
            closed[current] = true;
            let cur_x = current % self.grid_size;
            let cur_y = current / self.grid_size;

            for (dx, dy, step_cost) in dirs {
                let nx = cur_x as isize + dx;
                let ny = cur_y as isize + dy;
                if !self.in_bounds(nx, ny) {
                    continue;
                }
                let nx = nx as usize;
                let ny = ny as usize;

                let is_diagonal = dx != 0 && dy != 0;
                if is_diagonal {
                    let side_x = self.is_blocked_cell((cur_x as isize + dx) as usize, cur_y);
                    let side_y = self.is_blocked_cell(cur_x, (cur_y as isize + dy) as usize);
                    if side_x && side_y {
                        continue; // Can't move diagonally if both adjacent sides are blocked (prevents corner-cutting through walls)
                    }
                }

                if self.is_blocked_cell(nx, ny) {
                    continue;
                }

                let neighbor_idx = self.idx(nx, ny);
                if closed[neighbor_idx] {
                    continue;
                }

                let tentative_g = g_score[current] + step_cost;
                if tentative_g < g_score[neighbor_idx] {
                    g_score[neighbor_idx] = tentative_g;
                    parent[neighbor_idx] = current;
                    let h_score = heuristic(neighbor_idx);
                    open.push(OpenNode {
                        idx: neighbor_idx,
                        f_score: tentative_g + h_score,
                        h_score,
                    });
                }
            }
        }

        vec![start, end]
    }
}

#[derive(Clone, Copy, Debug)]
struct OpenNode {
    idx: usize,
    f_score: f64,
    h_score: f64,
}

impl PartialEq for OpenNode {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
            && self.f_score.to_bits() == other.f_score.to_bits()
            && self.h_score.to_bits() == other.h_score.to_bits()
    }
}

impl Eq for OpenNode {}

impl PartialOrd for OpenNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OpenNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f_score
            .partial_cmp(&self.f_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                other
                    .h_score
                    .partial_cmp(&self.h_score)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| other.idx.cmp(&self.idx))
    }
}

fn point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let xi = polygon[i].x;
        let yi = polygon[i].y;
        let xj = polygon[j].x;
        let yj = polygon[j].y;
        let intersects = ((yi > point.y) != (yj > point.y))
            && (point.x < (xj - xi) * (point.y - yi) / (yj - yi + 1e-9) + xi);
        if intersects {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn set_champion_direct_path(champion: &mut ChampionRuntime, target: Vec2) {
    if let Some(current_target) = current_champion_path_target(champion) {
        if dist(current_target, target) <= CHAMPION_REPATH_MIN_TARGET_DELTA {
            return;
        }
    }

    let grid = nav_grid();
    if grid.has_line_of_sight(champion.pos, target) {
        champion.target_path = vec![target];
        champion.target_path_index = 0;
        champion.path_stuck_for_sec = 0.0;
        return;
    }

    let mut path = grid.find_path(champion.pos, target);

    if path.len() > 1 {
        let first_useful = path
            .iter()
            .position(|node| dist(*node, champion.pos) >= NAV_PATH_TRIVIAL_NODE_EPSILON)
            .unwrap_or_else(|| path.len().saturating_sub(1));
        if first_useful > 0 {
            path.drain(0..first_useful);
        }
    }

    if path.len() <= 1 && dist(champion.pos, target) > NAV_PATH_MIN_DIRECT_DIST {
        champion.target_path = vec![target];
    } else {
        champion.target_path = path;
    }
    champion.target_path_index = 0;
    champion.path_stuck_for_sec = 0.0;
}

fn current_champion_path_target(champion: &ChampionRuntime) -> Option<Vec2> {
    champion
        .target_path
        .get(champion.target_path_index)
        .copied()
        .or_else(|| champion.target_path.last().copied())
}

fn set_champion_direct_path_hysteresis(
    champion: &mut ChampionRuntime,
    target: Vec2,
    min_target_delta: f64,
) {
    if let Some(current_target) = current_champion_path_target(champion) {
        if dist(current_target, target) <= min_target_delta {
            return;
        }
    }
    set_champion_direct_path(champion, target);
}

fn jungle_disengage_fallback_order_for_team(team: &str, jungle_pathing: &str) -> Vec<&'static str> {
    let (own_top, own_bot) = if normalized_team(team) == "red" {
        (
            ["gromp-red", "blue-buff-red", "wolves-red"],
            ["raptors-red", "red-buff-red", "krugs-red"],
        )
    } else {
        (
            ["gromp-blue", "blue-buff-blue", "wolves-blue"],
            ["raptors-blue", "red-buff-blue", "krugs-blue"],
        )
    };

    if jungle_pathing == "BotToTop" {
        vec![
            own_bot[0],
            own_bot[1],
            own_bot[2],
            "scuttle-bot",
            own_top[0],
            own_top[1],
            own_top[2],
            "scuttle-top",
        ]
    } else {
        vec![
            own_top[0],
            own_top[1],
            own_top[2],
            "scuttle-top",
            own_bot[0],
            own_bot[1],
            own_bot[2],
            "scuttle-bot",
        ]
    }
}

fn pick_jungle_farm_fallback_pos(
    champion: &ChampionRuntime,
    neutral_timers: &NeutralTimersRuntime,
    jungle_pathing: &str,
    threat_pos: Option<Vec2>,
) -> Option<Vec2> {
    let mut first_alive_fallback: Option<Vec2> = None;

    for key in jungle_disengage_fallback_order_for_team(&champion.team, jungle_pathing) {
        let Some(timer) = neutral_timers.entities.get(key) else {
            continue;
        };
        if !(timer.alive && timer.unlocked && is_jungle_camp_key(&timer.key)) {
            continue;
        }
        if first_alive_fallback.is_none() {
            first_alive_fallback = Some(timer.pos);
        }

        if let Some(threat) = threat_pos {
            if dist(timer.pos, threat) <= JUNGLE_DISENGAGE_THREAT_AVOID_RADIUS {
                continue;
            }
        }

        return Some(timer.pos);
    }

    first_alive_fallback
}

fn jgl_disengage_fallback_pos(
    runtime: &RuntimeState,
    champion: &ChampionRuntime,
    threat_pos: Vec2,
) -> Vec2 {
    let neutral_timers = decode_neutral_timers_state(&runtime.neutral_timers)
        .unwrap_or_else(|| neutral_timers_default_runtime_state());
    let team_tactics = team_tactics_for_runtime(runtime.extra.get("teamTactics"), &champion.team);
    if let Some(camp_pos) = pick_jungle_farm_fallback_pos(
        champion,
        &neutral_timers,
        &team_tactics.jungle_pathing,
        Some(threat_pos),
    ) {
        return camp_pos;
    }
    recall_fallback_toward_base(champion, None)
}

fn closest_lane_path_index(pos: Vec2, path: &[Vec2]) -> usize {
    path.iter()
        .enumerate()
        .min_by(|(idx_a, a), (idx_b, b)| {
            dist(**a, pos)
                .partial_cmp(&dist(**b, pos))
                .unwrap_or(Ordering::Equal)
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn lane_fallback_pos_from_tower(
    champion: &ChampionRuntime,
    tower_pos: Vec2,
    toward_base: bool,
) -> Vec2 {
    pathing::lane_fallback_pos_from_tower(champion, tower_pos, toward_base)
}

fn lane_pre_wave_hold_pos(champion: &ChampionRuntime, structures: &[StructureRuntime]) -> Vec2 {
    pathing::lane_pre_wave_hold_pos(champion, structures)
}

#[derive(Clone, Copy)]
struct LaneRoleProfile {
    chase_leash: f64,
    approach_leash: f64,
    retreat_hp: f64,
    outnumber_tolerance: f64,
}

#[derive(Clone, Copy)]
struct LanePressure {
    ally_champions: usize,
    enemy_champions: usize,
    ally_lane_minions: usize,
    enemy_lane_minions: usize,
    ally_score: f64,
    enemy_score: f64,
}

fn lane_role_profile(champion: &ChampionRuntime) -> Option<LaneRoleProfile> {
    pathing::lane_role_profile(champion)
}

fn is_first_wave_contest_active(champion: &ChampionRuntime, now: f64) -> bool {
    pathing::is_first_wave_contest_active(champion, now)
}

fn lane_anchor_pos(
    champion: &ChampionRuntime,
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> Vec2 {
    pathing::lane_anchor_pos(champion, minions, structures)
}

fn lane_wave_front_pos(
    champion: &ChampionRuntime,
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> Vec2 {
    pathing::lane_wave_front_pos(champion, minions, structures)
}

fn lane_pressure_at(
    champion: &ChampionRuntime,
    pos: Vec2,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    radius: f64,
) -> LanePressure {
    pathing::lane_pressure_at(champion, pos, champions, minions, radius)
}

fn lane_minion_context_distance(
    champion: &ChampionRuntime,
    pos: Vec2,
    minions: &[MinionRuntime],
) -> f64 {
    pathing::lane_minion_context_distance(champion, pos, minions)
}

fn in_lane_trade_context(
    champion: &ChampionRuntime,
    pos: Vec2,
    for_chase: bool,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> bool {
    trading::in_lane_trade_context(champion, pos, for_chase, champions, minions, structures)
}

fn is_deep_enemy_tower_zone(
    _champion: &ChampionRuntime,
    _target_pos: Vec2,
    _structures: &[StructureRuntime],
    _minions: &[MinionRuntime],
) -> bool {
    // Towers should not block traversal. Walls remain blocked by nav mesh.
    false
}

fn is_inside_laner_trade_leash(
    champion: &ChampionRuntime,
    target_pos: Vec2,
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> bool {
    let Some(profile) = lane_role_profile(champion) else {
        return true;
    };
    let lane_anchor = lane_anchor_pos(champion, minions, structures);
    let wave_front = lane_wave_front_pos(champion, minions, structures);
    dist(target_pos, lane_anchor) <= profile.chase_leash
        && dist(target_pos, wave_front) <= profile.chase_leash * 1.15
}

fn should_force_laner_disengage(
    champion: &ChampionRuntime,
    target_pos: Vec2,
    enemy: Option<&ChampionRuntime>,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> bool {
    if champion.role == "JGL" {
        let hp_ratio = if champion.max_hp <= 0.0 {
            1.0
        } else {
            champion.hp / champion.max_hp
        };
        if hp_ratio <= 0.40 {
            return true;
        }
        if is_deep_enemy_tower_zone(champion, target_pos, structures, minions) {
            return true;
        }
        let pressure = lane_pressure_at(
            champion,
            target_pos,
            champions,
            minions,
            LANE_LOCAL_PRESSURE_RADIUS,
        );
        if pressure.enemy_score > pressure.ally_score + 0.15 {
            return true;
        }
        return false;
    }
    let Some(profile) = lane_role_profile(champion) else {
        return false;
    };

    let hp_ratio = if champion.max_hp <= 0.0 {
        1.0
    } else {
        champion.hp / champion.max_hp
    };
    if hp_ratio <= profile.retreat_hp {
        return true;
    }
    if !is_inside_laner_trade_leash(champion, target_pos, minions, structures) {
        return true;
    }
    if is_deep_enemy_tower_zone(champion, target_pos, structures, minions) {
        return true;
    }

    let pressure = lane_pressure_at(
        champion,
        target_pos,
        champions,
        minions,
        LANE_LOCAL_PRESSURE_RADIUS,
    );
    let pressure_margin = if champion.role == "MID" {
        profile.outnumber_tolerance + 0.20
    } else {
        profile.outnumber_tolerance
    };
    if pressure.enemy_score > pressure.ally_score + pressure_margin
        && pressure.enemy_champions >= pressure.ally_champions + 1
    {
        return true;
    }

    if let Some(enemy_champion) = enemy {
        let enemy_hp_ratio = if enemy_champion.max_hp <= 0.0 {
            1.0
        } else {
            enemy_champion.hp / enemy_champion.max_hp
        };
        if hp_ratio + TRADE_HP_DISADVANTAGE_ALLOWANCE < enemy_hp_ratio {
            return true;
        }
    }

    false
}

fn lane_combat_state_mut<'a>(
    lane_combat_state_by_champion: &'a mut HashMap<String, LanerCombatStateRuntime>,
    champion_id: &str,
) -> &'a mut LanerCombatStateRuntime {
    lane_combat_state_by_champion
        .entry(champion_id.to_string())
        .or_default()
}

fn mark_lane_disengage(
    champion: &ChampionRuntime,
    now: f64,
    lane_combat_state_by_champion: &mut HashMap<String, LanerCombatStateRuntime>,
) {
    let state = lane_combat_state_mut(lane_combat_state_by_champion, &champion.id);
    state.last_disengage_at = now;
    state.reengage_at = f64::max(state.reengage_at, now + LANE_REENGAGE_COOLDOWN_SEC);
    state.recent_trade_until = f64::max(state.recent_trade_until, now + LANE_RECENT_TRADE_LOCK_SEC);
}

fn mark_lane_trade_hit(
    champion: &ChampionRuntime,
    now: f64,
    lane_combat_state_by_champion: &mut HashMap<String, LanerCombatStateRuntime>,
) {
    let state = lane_combat_state_mut(lane_combat_state_by_champion, &champion.id);
    state.recent_trade_until = f64::max(state.recent_trade_until, now + LANE_RECENT_TRADE_LOCK_SEC);
}

fn lane_trade_cooldown_active(
    champion: &ChampionRuntime,
    now: f64,
    lane_combat_state_by_champion: &HashMap<String, LanerCombatStateRuntime>,
) -> bool {
    lane_combat_state_by_champion
        .get(&champion.id)
        .map(|state| now < state.reengage_at)
        .unwrap_or(false)
}

fn lane_recent_trade_lock_active(
    champion: &ChampionRuntime,
    now: f64,
    lane_combat_state_by_champion: &HashMap<String, LanerCombatStateRuntime>,
) -> bool {
    lane_combat_state_by_champion
        .get(&champion.id)
        .map(|state| now < state.recent_trade_until)
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy)]
struct TradeConfidenceFeatures {
    self_hp_ratio: f64,
    enemy_hp_ratio: f64,
    ally_champions_local: usize,
    enemy_champions_local: usize,
    ally_minions_local: usize,
    enemy_minions_local: usize,
    nearest_enemy_tower_distance: f64,
    enemy_overextended: bool,
    first_wave_window: bool,
}

#[derive(Debug, Clone, Copy)]
struct TradeDecisionEvaluation {
    decision: bool,
    rule_decision: bool,
    confidence: f64,
    flipped_by_hybrid: bool,
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

fn clamp_ratio_01(value: f64) -> f64 {
    clamp(value, 0.0, 1.0)
}

fn maybe_log_hybrid_trade_flip(
    runtime: &mut RuntimeState,
    champion: &ChampionRuntime,
    decision_kind: &str,
    confidence: f64,
    rule_decision: bool,
    hybrid_decision: bool,
) {
    if runtime.ai_mode != SimulatorAiMode::Hybrid || rule_decision == hybrid_decision {
        return;
    }

    let state = lane_combat_state_mut(&mut runtime.lane_combat_state_by_champion, &champion.id);
    if runtime.time_sec < state.last_ai_debug_at + HYBRID_TRADE_DEBUG_LOG_COOLDOWN_SEC {
        return;
    }
    state.last_ai_debug_at = runtime.time_sec;

    log_event(
        runtime,
        &format!(
            "[ai-hybrid] {} {} flip: {} -> {} (score={:.2})",
            champion.name,
            decision_kind,
            if rule_decision {
                "rules-open"
            } else {
                "rules-close"
            },
            if hybrid_decision {
                "hybrid-open"
            } else {
                "hybrid-close"
            },
            confidence
        ),
        "info",
    );
}

fn should_commit_all_in_trade(
    champion: &ChampionRuntime,
    enemy: &ChampionRuntime,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
) -> bool {
    trading::should_commit_all_in_trade(champion, enemy, champions, minions)
}

fn evaluate_open_trade_window(
    champion: &ChampionRuntime,
    enemy: &ChampionRuntime,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
    lane_combat_state_by_champion: &HashMap<String, LanerCombatStateRuntime>,
    ai_mode: SimulatorAiMode,
    policy: &SimulatorPolicyConfig,
) -> TradeDecisionEvaluation {
    trading::evaluate_open_trade_window(
        champion,
        enemy,
        now,
        champions,
        minions,
        structures,
        lane_combat_state_by_champion,
        ai_mode,
        policy,
    )
}

fn can_open_trade_window(
    champion: &ChampionRuntime,
    enemy: &ChampionRuntime,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
    lane_combat_state_by_champion: &HashMap<String, LanerCombatStateRuntime>,
    ai_mode: SimulatorAiMode,
    policy: &SimulatorPolicyConfig,
) -> bool {
    evaluate_open_trade_window(
        champion,
        enemy,
        now,
        champions,
        minions,
        structures,
        lane_combat_state_by_champion,
        ai_mode,
        policy,
    )
    .decision
}

fn evaluate_disengage_champion_trade(
    champion: &ChampionRuntime,
    enemy: &ChampionRuntime,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
    ai_mode: SimulatorAiMode,
    policy: &SimulatorPolicyConfig,
) -> TradeDecisionEvaluation {
    trading::evaluate_disengage_champion_trade(
        champion, enemy, now, champions, minions, structures, ai_mode, policy,
    )
}

fn should_disengage_champion_trade(
    champion: &ChampionRuntime,
    enemy: &ChampionRuntime,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
    ai_mode: SimulatorAiMode,
    policy: &SimulatorPolicyConfig,
) -> bool {
    evaluate_disengage_champion_trade(
        champion, enemy, now, champions, minions, structures, ai_mode, policy,
    )
    .decision
}

fn lane_farm_anchor_pos_v2(
    champion: &ChampionRuntime,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> Vec2 {
    let lane_anchor = lane_anchor_pos(champion, minions, structures);
    let wave_front = lane_wave_front_pos(champion, minions, structures);

    if is_first_wave_contest_active(champion, now) {
        let to_wave = normalize(Vec2 {
            x: wave_front.x - lane_anchor.x,
            y: wave_front.y - lane_anchor.y,
        });
        let approach = lane_role_profile(champion)
            .map(|profile| profile.approach_leash)
            .unwrap_or(0.058);
        let contest_advance = f64::max(
            0.014,
            f64::min(approach * 0.95, dist(lane_anchor, wave_front) * 0.6),
        );
        return Vec2 {
            x: clamp(lane_anchor.x + to_wave.x * contest_advance, 0.01, 0.99),
            y: clamp(lane_anchor.y + to_wave.y * contest_advance, 0.01, 0.99),
        };
    }

    if champion.role == "SUP" {
        let allied_adc = champions
            .iter()
            .filter(|ally| {
                ally.alive
                    && ally.id != champion.id
                    && normalized_team(&ally.team) == normalized_team(&champion.team)
                    && ally.role == "ADC"
            })
            .min_by(|a, b| {
                dist(champion.pos, a.pos)
                    .partial_cmp(&dist(champion.pos, b.pos))
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.id.cmp(&b.id))
            });

        if let Some(adc) = allied_adc {
            let to_wave_from_adc = normalize(Vec2 {
                x: wave_front.x - adc.pos.x,
                y: wave_front.y - adc.pos.y,
            });
            let tethered = Vec2 {
                x: adc.pos.x - to_wave_from_adc.x * 0.012,
                y: adc.pos.y - to_wave_from_adc.y * 0.012,
            };
            if dist(tethered, wave_front) <= 0.14 {
                return Vec2 {
                    x: clamp(tethered.x, 0.01, 0.99),
                    y: clamp(tethered.y, 0.01, 0.99),
                };
            }
        }
    }

    // After lane start, MID should play tightly around wave instead of static lane anchor.
    if champion.role == "MID" && now > FIRST_WAVE_CONTEST_UNTIL {
        return Vec2 {
            x: clamp(wave_front.x, 0.01, 0.99),
            y: clamp(wave_front.y, 0.01, 0.99),
        };
    }

    let to_wave = normalize(Vec2 {
        x: wave_front.x - lane_anchor.x,
        y: wave_front.y - lane_anchor.y,
    });
    let role_leash = lane_role_profile(champion)
        .map(|profile| profile.approach_leash)
        .unwrap_or(0.058);

    let allied_lane_tower = structures
        .iter()
        .filter(|s| {
            s.alive
                && s.kind == "tower"
                && normalized_team(&s.team) == normalized_team(&champion.team)
                && normalized_lane(&s.lane) == normalized_lane(&champion.lane)
        })
        .min_by(|a, b| {
            dist(champion.pos, a.pos)
                .partial_cmp(&dist(champion.pos, b.pos))
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
    let wave_at_own_tower = allied_lane_tower
        .map(|tower| dist(wave_front, tower.pos) <= 0.11)
        .unwrap_or(false);

    if wave_at_own_tower && champion.role != "SUP" {
        if let Some(tower) = allied_lane_tower {
            let to_wave_from_tower = normalize(Vec2 {
                x: wave_front.x - tower.pos.x,
                y: wave_front.y - tower.pos.y,
            });
            let front_offset = clamp(champion.attack_range * 0.7, 0.02, 0.034);
            return Vec2 {
                x: clamp(
                    tower.pos.x + to_wave_from_tower.x * front_offset,
                    0.01,
                    0.99,
                ),
                y: clamp(
                    tower.pos.y + to_wave_from_tower.y * front_offset,
                    0.01,
                    0.99,
                ),
            };
        }
    }

    let emergency_farm_boost = if wave_at_own_tower { 1.55 } else { 1.0 };
    let advance = f64::min(
        role_leash * emergency_farm_boost,
        f64::max(0.01, dist(lane_anchor, wave_front) * 0.7),
    );

    Vec2 {
        x: clamp(lane_anchor.x + to_wave.x * advance, 0.01, 0.99),
        y: clamp(lane_anchor.y + to_wave.y * advance, 0.01, 0.99),
    }
}

fn lane_trade_approach_pos(
    champion: &ChampionRuntime,
    enemy: &ChampionRuntime,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> Vec2 {
    let anchor = lane_farm_anchor_pos_v2(champion, now, champions, minions, structures);
    let leash = lane_role_profile(champion)
        .map(|profile| profile.approach_leash)
        .unwrap_or(0.058);
    let enemy_from_anchor = normalize(Vec2 {
        x: enemy.pos.x - anchor.x,
        y: enemy.pos.y - anchor.y,
    });
    let desired_spacing = f64::max(0.025, champion.attack_range * 0.9);

    let ideal = Vec2 {
        x: enemy.pos.x - enemy_from_anchor.x * desired_spacing,
        y: enemy.pos.y - enemy_from_anchor.y * desired_spacing,
    };

    let delta = Vec2 {
        x: ideal.x - anchor.x,
        y: ideal.y - anchor.y,
    };
    let dist_from_anchor = dist(ideal, anchor);
    if dist_from_anchor <= leash {
        return Vec2 {
            x: clamp(ideal.x, 0.01, 0.99),
            y: clamp(ideal.y, 0.01, 0.99),
        };
    }

    let capped = normalize(delta);
    Vec2 {
        x: clamp(anchor.x + capped.x * leash, 0.01, 0.99),
        y: clamp(anchor.y + capped.y * leash, 0.01, 0.99),
    }
}

fn lane_retreat_anchor_pos(
    champion: &ChampionRuntime,
    threat_pos: Vec2,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> Vec2 {
    macro_ai::lane_retreat_anchor_pos(champion, threat_pos, now, champions, minions, structures)
}

fn issue_lane_disengage(runtime: &mut RuntimeState, champion_idx: usize, threat_pos: Vec2) {
    if champion_idx >= runtime.champions.len() {
        return;
    }

    let now = runtime.time_sec;
    let champion_snapshot = runtime.champions[champion_idx].clone();
    let fallback = if champion_snapshot.role == "JGL" {
        jgl_disengage_fallback_pos(runtime, &champion_snapshot, threat_pos)
    } else {
        lane_retreat_anchor_pos(
            &champion_snapshot,
            threat_pos,
            now,
            &runtime.champions,
            &runtime.minions,
            &runtime.structures,
        )
    };

    let champion = &mut runtime.champions[champion_idx];
    if champion.role != "JGL" {
        mark_lane_disengage(champion, now, &mut runtime.lane_combat_state_by_champion);
    }
    champion.state = "lane".to_string();
    set_champion_direct_path(champion, fallback);
}

fn nearest_enemy_champion_snapshot<'a>(
    champion: &ChampionRuntime,
    champions: &'a [ChampionRuntime],
    radius: f64,
) -> Option<&'a ChampionRuntime> {
    macro_ai::nearest_enemy_champion_snapshot(champion, champions, radius)
}

fn should_recall_in_place(champion: &ChampionRuntime, champions: &[ChampionRuntime]) -> bool {
    macro_ai::should_recall_in_place(champion, champions)
}

fn recall_fallback_toward_base(
    champion: &ChampionRuntime,
    threat: Option<&ChampionRuntime>,
) -> Vec2 {
    macro_ai::recall_fallback_toward_base(champion, threat)
}

fn safe_recall_anchor(
    champion: &ChampionRuntime,
    proposed: Vec2,
    structures: &[StructureRuntime],
) -> Vec2 {
    if champion.role == "JGL" {
        return proposed;
    }

    let enemy_tower = structures
        .iter()
        .filter(|structure| {
            structure.alive
                && structure.kind == "tower"
                && normalized_team(&structure.team) != normalized_team(&champion.team)
                && dist(proposed, structure.pos) <= TOWER_ATTACK_RANGE + 0.045
        })
        .min_by(|a, b| {
            dist(proposed, a.pos)
                .partial_cmp(&dist(proposed, b.pos))
                .unwrap_or(Ordering::Equal)
        });

    let Some(tower) = enemy_tower else {
        return proposed;
    };

    let base = base_position_for(&champion.team);
    let away_from_tower_to_base = normalize(Vec2 {
        x: base.x - tower.pos.x,
        y: base.y - tower.pos.y,
    });
    Vec2 {
        x: clamp(
            tower.pos.x + away_from_tower_to_base.x * (TOWER_ATTACK_RANGE + 0.065),
            0.01,
            0.99,
        ),
        y: clamp(
            tower.pos.y + away_from_tower_to_base.y * (TOWER_ATTACK_RANGE + 0.065),
            0.01,
            0.99,
        ),
    }
}

fn start_recall(
    champion: &mut ChampionRuntime,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) {
    if champion.state == "recall" {
        return;
    }

    champion.state = "recall".to_string();
    champion.recall_channel_until = 0.0;
    champion.target_path.clear();
    champion.target_path_index = 0;
    champion.recall_anchor = if should_recall_in_place(champion, champions) {
        Some(safe_recall_anchor(champion, champion.pos, structures))
    } else {
        let nearest =
            nearest_enemy_champion_snapshot(champion, champions, RECALL_SAFE_ENEMY_RADIUS)
                .or_else(|| nearest_enemy_champion_snapshot(champion, champions, f64::INFINITY));
        if let Some(threat) = nearest {
            if champion.role == "JGL" {
                Some(recall_fallback_toward_base(champion, Some(threat)))
            } else {
                Some(safe_recall_anchor(
                    champion,
                    lane_retreat_anchor_pos(
                        champion, threat.pos, now, champions, minions, structures,
                    ),
                    structures,
                ))
            }
        } else {
            if champion.role == "JGL" {
                Some(base_position_for(&champion.team))
            } else {
                Some(safe_recall_anchor(
                    champion,
                    lane_retreat_anchor_pos(
                        champion,
                        champion.pos,
                        now,
                        champions,
                        minions,
                        structures,
                    ),
                    structures,
                ))
            }
        }
    };
}

fn cancel_recall(champion: &mut ChampionRuntime, now: f64, events: &mut Vec<RuntimeEvent>) {
    if champion.state != "recall" {
        return;
    }

    let was_channeling = champion.recall_channel_until > now;
    champion.state = "lane".to_string();
    champion.recall_anchor = None;
    champion.recall_channel_until = 0.0;

    if was_channeling {
        push_event(
            events,
            now,
            &format!("{} recall interrupted", champion.name),
            "recall",
        );
    }
}

fn tick_recall(
    champion: &mut ChampionRuntime,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
    events: &mut Vec<RuntimeEvent>,
) -> bool {
    if champion.state != "recall" {
        return false;
    }

    if champion.recall_channel_until > 0.0 && now >= champion.recall_channel_until {
        champion.pos = base_position_for(&champion.team);
        champion.hp = champion.max_hp;
        maybe_upgrade_trinket_to_oracle(champion, now);
        champion.state = "lane".to_string();
        champion.recall_anchor = None;
        champion.recall_channel_until = 0.0;
        champion.target_path.clear();
        champion.target_path_index = 0;
        champion.next_decision_at = now;
        push_event(
            events,
            now,
            &format!("{} recalled", champion.name),
            "recall",
        );
        return false;
    }

    if champion.recall_channel_until > now {
        return true;
    }

    let anchor = safe_recall_anchor(
        champion,
        champion.recall_anchor.unwrap_or(champion.pos),
        structures,
    );
    champion.recall_anchor = Some(anchor);
    if dist(champion.pos, anchor) > 0.012 {
        set_champion_direct_path(champion, anchor);
        return true;
    }

    if !should_recall_in_place(champion, champions) {
        let threat = nearest_enemy_champion_snapshot(champion, champions, RECALL_SAFE_ENEMY_RADIUS)
            .or_else(|| nearest_enemy_champion_snapshot(champion, champions, f64::INFINITY));
        let fallback_anchor = if champion.role == "JGL" {
            recall_fallback_toward_base(champion, threat)
        } else {
            let threat_pos = threat.map(|enemy| enemy.pos).unwrap_or(champion.pos);
            safe_recall_anchor(
                champion,
                lane_retreat_anchor_pos(champion, threat_pos, now, champions, minions, structures),
                structures,
            )
        };
        champion.recall_anchor = Some(fallback_anchor);
        set_champion_direct_path(champion, fallback_anchor);
        return true;
    }

    champion.recall_channel_until = now + RECALL_CHANNEL_SEC;
    champion.target_path.clear();
    champion.target_path_index = 0;
    push_event(
        events,
        now,
        &format!("{} started recall", champion.name),
        "recall",
    );
    true
}

fn decide_champion_state(
    champion: &mut ChampionRuntime,
    now: f64,
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
    champions: &[ChampionRuntime],
    neutral_timers: Option<&NeutralTimersRuntime>,
    team_tactics: &RuntimeTeamTactics,
    team_buffs: &RuntimeTeamBuffState,
) {
    macro_ai::decide_champion_state(
        champion,
        now,
        minions,
        structures,
        champions,
        neutral_timers,
        team_tactics,
        team_buffs,
    );
}

fn is_objective_neutral_key(key: &str) -> bool {
    macro_ai::is_objective_neutral_key(key)
}

fn is_jungle_camp_key(key: &str) -> bool {
    macro_ai::is_jungle_camp_key(key)
}

fn contested_dragon_attempt_for_team<'a>(
    team: &str,
    champions: &[ChampionRuntime],
    neutral_timers: &'a NeutralTimersRuntime,
) -> Option<&'a NeutralTimerRuntime> {
    macro_ai::contested_dragon_attempt_for_team(team, champions, neutral_timers)
}

fn nearby_neutral_objective_key(
    champion: &ChampionRuntime,
    neutral_timers: &NeutralTimersRuntime,
) -> Option<String> {
    macro_ai::nearby_neutral_objective_key(champion, neutral_timers)
}

fn active_objective_attempt_for_team<'a>(
    team: &str,
    champions: &[ChampionRuntime],
    neutral_timers: &'a NeutralTimersRuntime,
) -> Option<&'a NeutralTimerRuntime> {
    macro_ai::active_objective_attempt_for_team(team, champions, neutral_timers)
}

fn should_assist_objective_attempt(
    champion: &ChampionRuntime,
    champions: &[ChampionRuntime],
    neutral_timers: &NeutralTimersRuntime,
) -> bool {
    macro_ai::should_assist_objective_attempt(champion, champions, neutral_timers)
}

fn should_hard_assist_contested_dragon(
    champion: &ChampionRuntime,
    contested_dragon: Option<&NeutralTimerRuntime>,
) -> bool {
    macro_ai::should_hard_assist_contested_dragon(champion, contested_dragon)
}

fn minion_stats(kind: &str) -> (f64, f64, f64, f64) {
    if kind == "ranged" {
        (
            MINION_RANGED_MOVE_SPEED,
            MINION_RANGED_ATTACK_RANGE,
            MINION_RANGED_ATTACK_DAMAGE,
            MINION_RANGED_ATTACK_CADENCE,
        )
    } else {
        (
            MINION_MELEE_MOVE_SPEED,
            MINION_MELEE_ATTACK_RANGE,
            MINION_MELEE_ATTACK_DAMAGE,
            MINION_MELEE_ATTACK_CADENCE,
        )
    }
}

fn spawn_waves_if_due(runtime: &mut RuntimeState, session: &mut SimLiveSession) {
    waves::spawn_waves_if_due(runtime, session);
}

fn spawn_formation_position(path: &[Vec2], kind: &str, slot: i32) -> Vec2 {
    let origin = path.first().copied().unwrap_or(Vec2 { x: 0.5, y: 0.5 });
    let next = path.get(1).copied().unwrap_or(origin);
    let direction = normalize(Vec2 {
        x: next.x - origin.x,
        y: next.y - origin.y,
    });
    let perpendicular = Vec2 {
        x: -direction.y,
        y: direction.x,
    };
    let row = if kind == "melee" { 0.0 } else { 1.0 };
    let column = f64::from(slot) - 1.0;
    let depth = row * 0.0105 + column.abs() * 0.002;
    let lateral = column * 0.0048;

    Vec2 {
        x: clamp(
            origin.x - direction.x * depth + perpendicular.x * lateral,
            0.01,
            0.99,
        ),
        y: clamp(
            origin.y - direction.y * depth + perpendicular.y * lateral,
            0.01,
            0.99,
        ),
    }
}

fn move_champions(runtime: &mut RuntimeState, dt: f64) {
    pathing::move_champions(runtime, dt);
}

fn move_minions(runtime: &mut RuntimeState, dt: f64) {
    minions::move_minions(runtime, dt);
}

fn resolve_minion_combat(runtime: &mut RuntimeState) {
    minions::resolve_minion_combat(runtime);
}

#[derive(Clone)]
enum CombatTarget {
    Champion(usize),
    Minion(usize),
    Structure(usize),
    Neutral(String),
}

fn laner_farm_search_radius(champion: &ChampionRuntime) -> f64 {
    if champion.role == "JGL" {
        return 0.13;
    }
    match champion.role.as_str() {
        "TOP" => 0.14,
        "MID" => 0.15,
        "ADC" => 0.145,
        _ => 0.12,
    }
}

fn has_local_numbers_advantage(
    champion: &ChampionRuntime,
    pos: Vec2,
    champions: &[ChampionRuntime],
    radius: f64,
) -> bool {
    let ally = champions
        .iter()
        .filter(|u| {
            u.alive
                && normalized_team(&u.team) == normalized_team(&champion.team)
                && dist(u.pos, pos) <= radius
        })
        .count();
    let enemy = champions
        .iter()
        .filter(|u| {
            u.alive
                && normalized_team(&u.team) != normalized_team(&champion.team)
                && dist(u.pos, pos) <= radius
        })
        .count();
    ally > enemy
}

fn enemy_pressuring_allied_tower_idx(
    champion: &ChampionRuntime,
    champions: &[ChampionRuntime],
    structures: &[StructureRuntime],
) -> Option<usize> {
    let allied_towers: Vec<&StructureRuntime> = structures
        .iter()
        .filter(|s| {
            s.alive
                && s.kind == "tower"
                && normalized_team(&s.team) == normalized_team(&champion.team)
        })
        .collect();
    if allied_towers.is_empty() {
        return None;
    }

    champions
        .iter()
        .enumerate()
        .filter(|(_, enemy)| {
            enemy.alive
                && normalized_team(&enemy.team) != normalized_team(&champion.team)
                && dist(champion.pos, enemy.pos) <= LANE_CHAMPION_TRADE_RADIUS
                && allied_towers.iter().any(|tower| {
                    normalized_lane(&tower.lane) == normalized_lane(&enemy.lane)
                        && dist(enemy.pos, tower.pos) <= 0.095
                })
        })
        .min_by(|(idx_a, a), (idx_b, b)| {
            a.hp.partial_cmp(&b.hp)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    dist(champion.pos, a.pos)
                        .partial_cmp(&dist(champion.pos, b.pos))
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx)
}

fn resolve_champion_combat(runtime: &mut RuntimeState) {
    combat::resolve_champion_combat(runtime);
}

fn champion_has_spell(champion: &ChampionRuntime, key: &str) -> bool {
    champion
        .summoner_spells
        .iter()
        .any(|spell| spell.key.eq_ignore_ascii_case(key))
}

fn spell_ready(champion: &ChampionRuntime, key: &str, now: f64) -> bool {
    champion
        .summoner_spells
        .iter()
        .find(|spell| spell.key.eq_ignore_ascii_case(key))
        .map(|spell| now >= spell.cd_until)
        .unwrap_or(false)
}

fn set_spell_cd(champion: &mut ChampionRuntime, key: &str, now: f64, cooldown_sec: f64) -> bool {
    let Some(spell) = champion
        .summoner_spells
        .iter_mut()
        .find(|spell| spell.key.eq_ignore_ascii_case(key))
    else {
        return false;
    };
    spell.cd_until = now + cooldown_sec;
    true
}

fn champion_is_banished(champion: &ChampionRuntime) -> bool {
    champion.realm_banished_until > 0.0
}

fn maybe_upgrade_trinket_to_oracle(champion: &mut ChampionRuntime, now: f64) {
    if champion.trinket_swapped || now < TRINKET_SWAP_UNLOCK_AT_SEC {
        return;
    }
    if champion.role != "JGL" && champion.role != "SUP" {
        return;
    }
    champion.trinket_key = TRINKET_ORACLE_LENS.to_string();
    champion.trinket_swapped = true;
}

fn ultimate_ready(champion: &ChampionRuntime, now: f64) -> bool {
    champion
        .ultimate
        .as_ref()
        .map(|ultimate| now >= ultimate.cd_until)
        .unwrap_or(false)
}

fn set_ultimate_cd(champion: &mut ChampionRuntime, now: f64, cooldown_sec: f64) -> bool {
    let Some(ultimate) = champion.ultimate.as_mut() else {
        return false;
    };
    ultimate.cd_until = now + cooldown_sec;
    true
}

fn nearest_enemy_in_range(
    runtime: &RuntimeState,
    champion_idx: usize,
    range: f64,
) -> Option<usize> {
    if champion_idx >= runtime.champions.len() {
        return None;
    }
    let champion = &runtime.champions[champion_idx];
    runtime
        .champions
        .iter()
        .enumerate()
        .filter(|(idx, enemy)| {
            *idx != champion_idx
                && enemy.alive
                && !champion_is_banished(enemy)
                && normalized_team(&enemy.team) != normalized_team(&champion.team)
                && team_has_vision_at(runtime, &champion.team, enemy.pos)
                && dist(enemy.pos, champion.pos) <= range
        })
        .min_by(|(_, a), (_, b)| {
            dist(a.pos, champion.pos)
                .partial_cmp(&dist(b.pos, champion.pos))
                .unwrap_or(Ordering::Equal)
        })
        .map(|(idx, _)| idx)
}

fn recent_attacker_target_idx(
    runtime: &RuntimeState,
    champion_idx: usize,
    range: f64,
    max_age_sec: f64,
) -> Option<usize> {
    if champion_idx >= runtime.champions.len() {
        return None;
    }

    let champion = &runtime.champions[champion_idx];
    let attacker_id = champion.last_damaged_by_champion_id.as_deref()?;
    if runtime.time_sec - champion.last_damaged_by_champion_at > max_age_sec {
        return None;
    }

    runtime
        .champions
        .iter()
        .enumerate()
        .find(|(idx, enemy)| {
            *idx != champion_idx
                && enemy.alive
                && !champion_is_banished(enemy)
                && enemy.id == attacker_id
                && normalized_team(&enemy.team) != normalized_team(&champion.team)
                && team_has_vision_at(runtime, &champion.team, enemy.pos)
                && dist(enemy.pos, champion.pos) <= range
        })
        .map(|(idx, _)| idx)
}

fn next_summon_id(runtime: &mut RuntimeState) -> String {
    let next = runtime
        .extra
        .get("nextSummonId")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(1);
    runtime
        .extra
        .insert("nextSummonId".to_string(), Value::from(next + 1));
    format!("summon-{next}")
}

fn set_rift_herald_charge(runtime: &mut RuntimeState, killer_team: &str, killer_id: &str) {
    runtime
        .extra
        .insert("heraldReady".to_string(), Value::from(true));
    runtime.extra.insert(
        "heraldTeam".to_string(),
        Value::from(normalized_team(killer_team)),
    );
    runtime
        .extra
        .insert("heraldCarrierId".to_string(), Value::from(killer_id));
}

fn clear_rift_herald_charge(runtime: &mut RuntimeState) {
    runtime
        .extra
        .insert("heraldReady".to_string(), Value::from(false));
    runtime.extra.remove("heraldTeam");
    runtime.extra.remove("heraldCarrierId");
}

fn maybe_deploy_rift_herald_charge(runtime: &mut RuntimeState) {
    let ready = runtime
        .extra
        .get("heraldReady")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !ready {
        return;
    }

    let carrier_id = runtime
        .extra
        .get("heraldCarrierId")
        .and_then(Value::as_str)
        .map(|value| value.to_string());
    let herald_team = runtime
        .extra
        .get("heraldTeam")
        .and_then(Value::as_str)
        .map(normalized_team)
        .unwrap_or("blue")
        .to_string();

    let carrier_idx = if let Some(carrier_id) = carrier_id {
        runtime
            .champions
            .iter()
            .position(|champion| champion.alive && champion.id == carrier_id)
    } else {
        runtime.champions.iter().position(|champion| {
            champion.alive
                && normalized_team(&champion.team) == normalized_team(&herald_team)
                && champion.role == "JGL"
        })
    };

    let Some(carrier_idx) = carrier_idx else {
        return;
    };

    let carrier = runtime.champions[carrier_idx].clone();
    let enemy_tower_idx = runtime
        .structures
        .iter()
        .enumerate()
        .filter(|(_, structure)| {
            structure.alive
                && structure.kind == "tower"
                && normalized_lane(&structure.lane) == normalized_lane(&carrier.lane)
                && is_structure_targetable(&runtime.structures, &carrier.team, structure)
        })
        .min_by(|(idx_a, a), (idx_b, b)| {
            dist(a.pos, carrier.pos)
                .partial_cmp(&dist(b.pos, carrier.pos))
                .unwrap_or(Ordering::Equal)
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx);

    let Some(enemy_tower_idx) = enemy_tower_idx else {
        clear_rift_herald_charge(runtime);
        return;
    };

    let enemy_tower_pos = runtime.structures[enemy_tower_idx].pos;
    if dist(carrier.pos, enemy_tower_pos) > 0.12 {
        return;
    }

    let summon = MinionRuntime {
        id: format!("herald-{}", next_summon_id(runtime)),
        team: carrier.team.clone(),
        lane: carrier.lane.clone(),
        pos: Vec2 {
            x: clamp(carrier.pos.x + 0.012, 0.01, 0.99),
            y: clamp(carrier.pos.y + 0.012, 0.01, 0.99),
        },
        hp: 420.0,
        max_hp: 420.0,
        alive: true,
        kind: "summon".to_string(),
        last_hit_by_champion_id: None,
        owner_champion_id: None,
        summon_kind: Some("herald".to_string()),
        summon_expires_at: runtime.time_sec + 55.0,
        attack_cd_until: runtime.time_sec,
        move_speed: 0.058,
        attack_range: 0.065,
        attack_damage: 34.0,
        path: lane_path_for(&carrier.team, &carrier.lane),
        path_index: 0,
    };

    runtime.minions.push(summon);
    log_event(
        runtime,
        &format!(
            "{} deployed rift herald",
            normalized_team(&carrier.team).to_uppercase()
        ),
        "info",
    );
    clear_rift_herald_charge(runtime);
}

fn try_cast_special_ultimate(
    runtime: &mut RuntimeState,
    champion_idx: usize,
    now: f64,
) -> Option<bool> {
    let champion = runtime.champions.get(champion_idx)?.clone();
    let key = champion.champion_id.to_lowercase();

    if ["yorick", "annie", "ivern", "shaco"].contains(&key.as_str()) {
        let (summon_kind, hp_ratio, damage_ratio, duration_sec) = summon_profile(&key);

        let already_alive = runtime.minions.iter().any(|minion| {
            minion.alive
                && minion.kind == "summon"
                && minion.owner_champion_id.as_deref() == Some(champion.id.as_str())
        });
        if already_alive {
            return Some(false);
        }

        let summon = MinionRuntime {
            id: format!("{}-{}", summon_kind, next_summon_id(runtime)),
            team: champion.team.clone(),
            lane: champion.lane.clone(),
            pos: Vec2 {
                x: clamp(champion.pos.x + 0.014, 0.01, 0.99),
                y: clamp(champion.pos.y + 0.01, 0.01, 0.99),
            },
            hp: (champion.max_hp * hp_ratio).max(35.0),
            max_hp: (champion.max_hp * hp_ratio).max(35.0),
            alive: true,
            kind: "summon".to_string(),
            last_hit_by_champion_id: None,
            owner_champion_id: Some(champion.id.clone()),
            summon_kind: Some(summon_kind.to_string()),
            summon_expires_at: now + duration_sec,
            attack_cd_until: now,
            move_speed: (champion.move_speed * 0.95).max(0.038),
            attack_range: champion.attack_range.max(0.045),
            attack_damage: (champion.attack_damage * damage_ratio).max(4.0),
            path: vec![champion.pos],
            path_index: 0,
        };

        runtime.minions.push(summon);
        log_event(
            runtime,
            &format!("{} summoned {}", champion.name, summon_kind),
            "info",
        );
        return Some(true);
    }

    if key == "shen" {
        let ally_idx = runtime
            .champions
            .iter()
            .enumerate()
            .filter(|(idx, ally)| {
                *idx != champion_idx
                    && ally.alive
                    && !champion_is_banished(ally)
                    && normalized_team(&ally.team) == normalized_team(&champion.team)
            })
            .min_by(|(idx_a, a), (idx_b, b)| {
                let ratio_a = if a.max_hp <= 0.0 {
                    1.0
                } else {
                    a.hp / a.max_hp
                };
                let ratio_b = if b.max_hp <= 0.0 {
                    1.0
                } else {
                    b.hp / b.max_hp
                };
                ratio_a
                    .partial_cmp(&ratio_b)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| idx_a.cmp(idx_b))
            })
            .map(|(idx, _)| idx);

        let Some(ally_idx) = ally_idx else {
            return Some(false);
        };

        let shield = runtime.champions[ally_idx].max_hp * 0.30;
        let ally_pos = runtime.champions[ally_idx].pos;
        runtime.champions[ally_idx].hp =
            (runtime.champions[ally_idx].hp + shield).min(runtime.champions[ally_idx].max_hp);
        runtime.champions[champion_idx].pos = ally_pos;
        runtime.champions[champion_idx].target_path.clear();
        runtime.champions[champion_idx].target_path_index = 0;
        runtime.champions[champion_idx].next_decision_at = now;
        log_event(
            runtime,
            &format!("{} cast Stand United", champion.name),
            "info",
        );
        return Some(true);
    }

    if key == "mordekaiser" {
        let Some(target_idx) =
            nearest_enemy_in_range(runtime, champion_idx, ULTIMATE_BURST_RANGE + 0.03)
        else {
            return Some(false);
        };
        let caster_pos = runtime.champions[champion_idx].pos;
        let target_pos = runtime.champions[target_idx].pos;

        runtime.champions[champion_idx].realm_banished_until =
            now + ULTIMATE_MORDE_REALM_DURATION_SEC;
        runtime.champions[champion_idx].realm_return_pos = Some(caster_pos);
        runtime.champions[target_idx].realm_banished_until =
            now + ULTIMATE_MORDE_REALM_DURATION_SEC;
        runtime.champions[target_idx].realm_return_pos = Some(target_pos);

        runtime.champions[champion_idx].pos = Vec2 { x: -5.0, y: -5.0 };
        runtime.champions[target_idx].pos = Vec2 { x: -6.0, y: -6.0 };
        runtime.champions[champion_idx].target_path.clear();
        runtime.champions[target_idx].target_path.clear();
        runtime.champions[champion_idx].target_path_index = 0;
        runtime.champions[target_idx].target_path_index = 0;

        log_event(
            runtime,
            &format!("{} cast Realm of Death", champion.name),
            "info",
        );
        return Some(true);
    }

    None
}

fn try_cast_ultimate(runtime: &mut RuntimeState, champion_idx: usize, now: f64) -> bool {
    if champion_idx >= runtime.champions.len() || !runtime.champions[champion_idx].alive {
        return false;
    }

    let champion_snapshot = runtime.champions[champion_idx].clone();
    if champion_snapshot.role == "JGL" {
        return false;
    }
    if champion_snapshot.level < ULTIMATE_UNLOCK_LEVEL || !ultimate_ready(&champion_snapshot, now) {
        return false;
    }

    if let Some(casted_special) = try_cast_special_ultimate(runtime, champion_idx, now) {
        if casted_special {
            if set_ultimate_cd(
                &mut runtime.champions[champion_idx],
                now,
                ULTIMATE_BASE_CD_SEC,
            ) {
                return true;
            }
        }
        return false;
    }

    let archetype = champion_snapshot
        .ultimate
        .as_ref()
        .map(|ultimate| ultimate.archetype.to_lowercase())
        .unwrap_or_else(|| {
            default_ultimate_archetype_for_role(&champion_snapshot.role).to_string()
        });

    let casted = match archetype.as_str() {
        "execute" => {
            let Some(target_idx) =
                nearest_enemy_in_range(runtime, champion_idx, ULTIMATE_BURST_RANGE)
            else {
                return false;
            };
            let hp_ratio = if runtime.champions[target_idx].max_hp <= 0.0 {
                1.0
            } else {
                runtime.champions[target_idx].hp / runtime.champions[target_idx].max_hp
            };
            if hp_ratio > 0.38 {
                return false;
            }
            attack_enemy_champion(runtime, champion_idx, target_idx);
            attack_enemy_champion(runtime, champion_idx, target_idx);
            true
        }
        "engage" => {
            let Some(target_idx) =
                nearest_enemy_in_range(runtime, champion_idx, ULTIMATE_GLOBAL_RANGE)
            else {
                return false;
            };
            let target = runtime.champions[target_idx].pos;
            runtime.champions[champion_idx].pos = target;
            runtime.champions[champion_idx].target_path.clear();
            runtime.champions[champion_idx].target_path_index = 0;
            attack_enemy_champion(runtime, champion_idx, target_idx);
            true
        }
        "utility" | "sustain" | "defensive" => {
            if champion_snapshot.max_hp <= 0.0 {
                return false;
            }
            let hp_ratio = champion_snapshot.hp / champion_snapshot.max_hp;
            if hp_ratio > 0.55 {
                return false;
            }
            let heal_amount = champion_snapshot.max_hp * 0.26;
            runtime.champions[champion_idx].hp = (runtime.champions[champion_idx].hp + heal_amount)
                .min(runtime.champions[champion_idx].max_hp);
            true
        }
        "global" | "zone" => {
            let Some(target_idx) =
                nearest_enemy_in_range(runtime, champion_idx, ULTIMATE_GLOBAL_RANGE)
            else {
                return false;
            };
            attack_enemy_champion(runtime, champion_idx, target_idx);
            true
        }
        _ => {
            let Some(target_idx) =
                nearest_enemy_in_range(runtime, champion_idx, ULTIMATE_BURST_RANGE)
            else {
                return false;
            };
            attack_enemy_champion(runtime, champion_idx, target_idx);
            true
        }
    };

    if !casted {
        return false;
    }

    if set_ultimate_cd(
        &mut runtime.champions[champion_idx],
        now,
        ULTIMATE_BASE_CD_SEC,
    ) {
        log_event(
            runtime,
            &format!("{} cast Ultimate ({})", champion_snapshot.name, archetype),
            "info",
        );
        return true;
    }
    false
}

fn tick_ignite_dot_effects(runtime: &mut RuntimeState, now: f64) {
    for idx in 0..runtime.champions.len() {
        if !runtime.champions[idx].alive {
            runtime.champions[idx].ignite_dot_until = 0.0;
            runtime.champions[idx].ignite_source_id = None;
            continue;
        }
        if runtime.champions[idx].ignite_dot_until <= now {
            runtime.champions[idx].ignite_source_id = None;
            continue;
        }

        runtime.champions[idx].hp -= SUMMONER_IGNITE_DPS * 0.2;
        runtime.champions[idx].last_damaged_at = now;

        if runtime.champions[idx].hp > 0.0 {
            continue;
        }

        runtime.champions[idx].hp = 0.0;
        runtime.champions[idx].alive = false;
        runtime.champions[idx].deaths += 1;
        runtime.champions[idx].respawn_at =
            now + champion_respawn_seconds(runtime.champions[idx].level, now);

        let victim_name = runtime.champions[idx].name.clone();
        let killer_id = runtime.champions[idx].ignite_source_id.clone();
        runtime.champions[idx].ignite_dot_until = 0.0;
        runtime.champions[idx].ignite_source_id = None;

        if let Some(killer_id) = killer_id {
            if let Some(killer_idx) = runtime
                .champions
                .iter()
                .position(|champion| champion.id == killer_id)
            {
                if runtime.champions[killer_idx].alive {
                    runtime.champions[killer_idx].kills += 1;
                    let killer_team = runtime.champions[killer_idx].team.clone();
                    team_stats_mut(&mut runtime.stats, &killer_team).kills += 1;
                    add_gold_xp_to_champion(
                        runtime,
                        &killer_id,
                        CHAMPION_KILL_GOLD,
                        CHAMPION_KILL_XP,
                    );
                    log_event(
                        runtime,
                        &format!(
                            "{} ignited {}",
                            runtime.champions[killer_idx].name, victim_name
                        ),
                        "kill",
                    );
                    continue;
                }
            }
        }
    }
}

fn best_lane_tp_target(
    champion: &ChampionRuntime,
    structures: &[StructureRuntime],
    minions: &[MinionRuntime],
) -> Option<Vec2> {
    let lane_path = lane_path_for(&champion.team, &champion.lane);
    let max_idx = lane_path.len().saturating_sub(1).max(1);

    let tower_target = structures
        .iter()
        .filter(|structure| {
            structure.alive
                && normalized_team(&structure.team) == normalized_team(&champion.team)
                && structure.kind == "tower"
                && normalized_lane(&structure.lane) == normalized_lane(&champion.lane)
        })
        .max_by(|a, b| {
            let a_idx = closest_lane_path_index(a.pos, &lane_path) as f64 / max_idx as f64;
            let b_idx = closest_lane_path_index(b.pos, &lane_path) as f64 / max_idx as f64;
            a_idx
                .partial_cmp(&b_idx)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        })
        .map(|structure| {
            let progress =
                closest_lane_path_index(structure.pos, &lane_path) as f64 / max_idx as f64;
            (structure.pos, progress)
        });

    let minion_target = minions
        .iter()
        .filter(|minion| {
            minion.alive
                && normalized_team(&minion.team) == normalized_team(&champion.team)
                && normalized_lane(&minion.lane) == normalized_lane(&champion.lane)
        })
        .max_by(|a, b| {
            let a_idx = closest_lane_path_index(a.pos, &lane_path) as f64 / max_idx as f64;
            let b_idx = closest_lane_path_index(b.pos, &lane_path) as f64 / max_idx as f64;
            a_idx
                .partial_cmp(&b_idx)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        })
        .map(|minion| {
            let progress = closest_lane_path_index(minion.pos, &lane_path) as f64 / max_idx as f64;
            (minion.pos, progress)
        });

    match (tower_target, minion_target) {
        (Some((tower_pos, tower_progress)), Some((minion_pos, minion_progress))) => {
            if minion_progress > tower_progress {
                Some(minion_pos)
            } else {
                Some(tower_pos)
            }
        }
        (Some((tower_pos, _)), None) => Some(tower_pos),
        (None, Some((minion_pos, _))) => Some(minion_pos),
        (None, None) => None,
    }
}

fn try_cast_summoner_spells(
    runtime: &mut RuntimeState,
    neutral_timers: &mut NeutralTimersRuntime,
    champion_idx: usize,
    now: f64,
) -> bool {
    if champion_idx >= runtime.champions.len() || !runtime.champions[champion_idx].alive {
        return false;
    }

    if try_cast_heal(runtime, champion_idx, now) {
        return true;
    }
    if try_cast_flash(runtime, champion_idx, now) {
        return true;
    }
    if try_cast_ignite(runtime, champion_idx, now) {
        return true;
    }
    if try_cast_smite(runtime, neutral_timers, champion_idx, now) {
        return true;
    }
    if try_cast_teleport(runtime, champion_idx, now) {
        return true;
    }

    false
}

fn try_cast_heal(runtime: &mut RuntimeState, champion_idx: usize, now: f64) -> bool {
    let champion_snapshot = runtime.champions[champion_idx].clone();
    if !champion_has_spell(&champion_snapshot, "Heal")
        || !spell_ready(&champion_snapshot, "Heal", now)
    {
        return false;
    }

    let self_ratio = if champion_snapshot.max_hp <= 0.0 {
        1.0
    } else {
        champion_snapshot.hp / champion_snapshot.max_hp
    };

    let low_ally_exists = runtime.champions.iter().any(|ally| {
        ally.alive
            && ally.id != champion_snapshot.id
            && normalized_team(&ally.team) == normalized_team(&champion_snapshot.team)
            && dist(ally.pos, champion_snapshot.pos) <= SUMMONER_HEAL_RADIUS
            && ally.max_hp > 0.0
            && (ally.hp / ally.max_hp) <= 0.35
    });

    if self_ratio > 0.34 && !low_ally_exists {
        return false;
    }

    for ally in runtime.champions.iter_mut() {
        if !ally.alive || normalized_team(&ally.team) != normalized_team(&champion_snapshot.team) {
            continue;
        }
        if ally.id != champion_snapshot.id
            && dist(ally.pos, champion_snapshot.pos) > SUMMONER_HEAL_RADIUS
        {
            continue;
        }
        let ratio = if ally.id == champion_snapshot.id {
            SUMMONER_HEAL_SELF_RATIO
        } else {
            SUMMONER_HEAL_ALLY_RATIO
        };
        ally.hp = (ally.hp + ally.max_hp * ratio).min(ally.max_hp);
    }

    if set_spell_cd(
        &mut runtime.champions[champion_idx],
        "Heal",
        now,
        SUMMONER_HEAL_CD_SEC,
    ) {
        log_event(
            runtime,
            &format!("{} cast Heal", champion_snapshot.name),
            "info",
        );
        return true;
    }
    false
}

fn try_cast_flash(runtime: &mut RuntimeState, champion_idx: usize, now: f64) -> bool {
    let champion_snapshot = runtime.champions[champion_idx].clone();
    if !champion_has_spell(&champion_snapshot, "Flash")
        || !spell_ready(&champion_snapshot, "Flash", now)
    {
        return false;
    }

    let self_ratio = if champion_snapshot.max_hp <= 0.0 {
        1.0
    } else {
        champion_snapshot.hp / champion_snapshot.max_hp
    };
    let nearest_enemy = runtime
        .champions
        .iter()
        .filter(|enemy| {
            enemy.alive
                && normalized_team(&enemy.team) != normalized_team(&champion_snapshot.team)
                && dist(enemy.pos, champion_snapshot.pos) <= 0.10
        })
        .min_by(|a, b| {
            dist(a.pos, champion_snapshot.pos)
                .partial_cmp(&dist(b.pos, champion_snapshot.pos))
                .unwrap_or(Ordering::Equal)
        });

    if self_ratio > 0.28 || nearest_enemy.is_none() {
        return false;
    }

    let base = base_position_for(&champion_snapshot.team);
    let to_base = Vec2 {
        x: base.x - champion_snapshot.pos.x,
        y: base.y - champion_snapshot.pos.y,
    };
    let len = (to_base.x * to_base.x + to_base.y * to_base.y)
        .sqrt()
        .max(1e-6);
    let target = Vec2 {
        x: clamp(
            champion_snapshot.pos.x + (to_base.x / len) * SUMMONER_FLASH_RANGE,
            0.01,
            0.99,
        ),
        y: clamp(
            champion_snapshot.pos.y + (to_base.y / len) * SUMMONER_FLASH_RANGE,
            0.01,
            0.99,
        ),
    };

    runtime.champions[champion_idx].pos = target;
    runtime.champions[champion_idx].target_path.clear();
    runtime.champions[champion_idx].target_path_index = 0;

    if set_spell_cd(
        &mut runtime.champions[champion_idx],
        "Flash",
        now,
        SUMMONER_FLASH_CD_SEC,
    ) {
        log_event(
            runtime,
            &format!("{} flashed", champion_snapshot.name),
            "info",
        );
        return true;
    }
    false
}

fn try_cast_ignite(runtime: &mut RuntimeState, champion_idx: usize, now: f64) -> bool {
    let champion_snapshot = runtime.champions[champion_idx].clone();
    if !champion_has_spell(&champion_snapshot, "Ignite")
        || !spell_ready(&champion_snapshot, "Ignite", now)
    {
        return false;
    }

    let target_idx = runtime
        .champions
        .iter()
        .enumerate()
        .filter(|(_, enemy)| {
            enemy.alive
                && normalized_team(&enemy.team) != normalized_team(&champion_snapshot.team)
                && dist(enemy.pos, champion_snapshot.pos) <= SUMMONER_IGNITE_RANGE
                && enemy.ignite_dot_until <= now
                && enemy.max_hp > 0.0
                && (enemy.hp / enemy.max_hp) <= 0.42
        })
        .min_by(|(idx_a, a), (idx_b, b)| {
            (a.hp / a.max_hp)
                .partial_cmp(&(b.hp / b.max_hp))
                .unwrap_or(Ordering::Equal)
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx);

    let Some(target_idx) = target_idx else {
        return false;
    };

    let target_name = runtime.champions[target_idx].name.clone();
    runtime.champions[target_idx].ignite_dot_until = now + SUMMONER_IGNITE_DURATION_SEC;
    runtime.champions[target_idx].ignite_source_id = Some(champion_snapshot.id.clone());
    runtime.champions[target_idx].last_damaged_by_champion_id = Some(champion_snapshot.id.clone());
    runtime.champions[target_idx].last_damaged_by_champion_at = now;
    runtime.champions[target_idx].last_damaged_at = now;

    if set_spell_cd(
        &mut runtime.champions[champion_idx],
        "Ignite",
        now,
        SUMMONER_IGNITE_CD_SEC,
    ) {
        log_event(
            runtime,
            &format!("{} ignited {}", champion_snapshot.name, target_name),
            "info",
        );
        return true;
    }
    false
}

fn try_cast_smite(
    runtime: &mut RuntimeState,
    neutral_timers: &mut NeutralTimersRuntime,
    champion_idx: usize,
    now: f64,
) -> bool {
    let champion_snapshot = runtime.champions[champion_idx].clone();
    if !champion_has_spell(&champion_snapshot, "Smite")
        || !spell_ready(&champion_snapshot, "Smite", now)
    {
        return false;
    }
    if champion_snapshot.role != "JGL" {
        return false;
    }

    let neutral_key = nearest_attackable_neutral_key(
        &champion_snapshot,
        neutral_timers,
        SUMMONER_SMITE_RANGE,
        SUMMONER_SMITE_RANGE,
    );
    let Some(neutral_key) = neutral_key else {
        return false;
    };

    let Some(timer) = neutral_timers.entities.get(&neutral_key) else {
        return false;
    };
    if !timer.alive || timer.hp > SUMMONER_SMITE_DAMAGE {
        return false;
    }

    if let Some(timer_mut) = neutral_timers.entities.get_mut(&neutral_key) {
        timer_mut.hp = 0.0;
    }
    mark_neutral_taken(runtime, neutral_timers, &neutral_key, Some(champion_idx));

    if set_spell_cd(
        &mut runtime.champions[champion_idx],
        "Smite",
        now,
        SUMMONER_SMITE_CD_SEC,
    ) {
        log_event(
            runtime,
            &format!("{} cast Smite", champion_snapshot.name),
            "info",
        );
        return true;
    }
    false
}

fn try_cast_teleport(runtime: &mut RuntimeState, champion_idx: usize, now: f64) -> bool {
    let champion_snapshot = runtime.champions[champion_idx].clone();
    if !champion_has_spell(&champion_snapshot, "Teleport")
        || !spell_ready(&champion_snapshot, "Teleport", now)
    {
        return false;
    }
    if now < SUMMONER_TP_UNLOCK_AT_SEC {
        return false;
    }

    let base = base_position_for(&champion_snapshot.team);
    let at_base = dist(champion_snapshot.pos, base) <= 0.22;
    if !at_base {
        return false;
    }

    let nearby_enemy = runtime.champions.iter().any(|enemy| {
        enemy.alive
            && normalized_team(&enemy.team) != normalized_team(&champion_snapshot.team)
            && dist(enemy.pos, champion_snapshot.pos) <= 0.14
    });
    if nearby_enemy {
        return false;
    }

    let Some(target) =
        best_lane_tp_target(&champion_snapshot, &runtime.structures, &runtime.minions)
    else {
        return false;
    };

    runtime.champions[champion_idx].pos = target;
    runtime.champions[champion_idx].target_path.clear();
    runtime.champions[champion_idx].target_path_index = 0;
    runtime.champions[champion_idx].next_decision_at = now;

    if set_spell_cd(
        &mut runtime.champions[champion_idx],
        "Teleport",
        now,
        SUMMONER_TP_CD_SEC,
    ) {
        log_event(
            runtime,
            &format!("{} cast Teleport", champion_snapshot.name),
            "recall",
        );
        return true;
    }
    false
}

fn neutral_timers_default_runtime_state() -> NeutralTimersRuntime {
    serde_json::from_value(build_neutral_timers_state()).unwrap_or(NeutralTimersRuntime {
        dragon_soul_unlocked: false,
        elder_unlocked: false,
        entities: HashMap::new(),
        extra: HashMap::new(),
    })
}

fn decode_neutral_timers_state(value: &Value) -> Option<NeutralTimersRuntime> {
    serde_json::from_value(value.clone()).ok()
}

fn nearest_enemy_champion_contesting_objective(
    champions: &[ChampionRuntime],
    attacker: &ChampionRuntime,
    objective_pos: Vec2,
    range: f64,
) -> Option<usize> {
    champions
        .iter()
        .enumerate()
        .filter(|(_, enemy)| {
            enemy.alive
                && enemy.id != attacker.id
                && normalized_team(&enemy.team) != normalized_team(&attacker.team)
                && dist(enemy.pos, objective_pos) <= OBJECTIVE_ASSIST_RADIUS
                && dist(enemy.pos, attacker.pos) <= range
        })
        .min_by(|(idx_a, a), (idx_b, b)| {
            dist(a.pos, attacker.pos)
                .partial_cmp(&dist(b.pos, attacker.pos))
                .unwrap_or(Ordering::Equal)
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx)
}

fn nearest_attackable_neutral_key(
    champion: &ChampionRuntime,
    neutral_timers: &NeutralTimersRuntime,
    camp_radius: f64,
    objective_radius: f64,
) -> Option<String> {
    let mut candidates: Vec<&NeutralTimerRuntime> = neutral_timers
        .entities
        .values()
        .filter(|timer| timer.alive && timer.unlocked)
        .filter(|timer| {
            neutral_is_attackable_by_champion(champion, timer, camp_radius, objective_radius)
        })
        .collect();

    candidates.sort_by(|a, b| {
        dist(champion.pos, a.pos)
            .partial_cmp(&dist(champion.pos, b.pos))
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.key.cmp(&b.key))
    });

    candidates.first().map(|timer| timer.key.clone())
}

fn neutral_is_attackable_by_champion(
    champion: &ChampionRuntime,
    timer: &NeutralTimerRuntime,
    camp_radius: f64,
    objective_radius: f64,
) -> bool {
    let is_objective = is_objective_neutral_key(&timer.key);
    let max_range = if is_objective {
        objective_radius
    } else {
        camp_radius
    };
    if dist(champion.pos, timer.pos) > max_range {
        return false;
    }
    is_objective || nav_grid().has_line_of_sight(champion.pos, timer.pos)
}

fn mark_neutral_taken(
    runtime: &mut RuntimeState,
    neutral_timers: &mut NeutralTimersRuntime,
    key: &str,
    killer_idx: Option<usize>,
) {
    let timer_label = {
        let Some(timer) = neutral_timers.entities.get_mut(key) else {
            return;
        };
        if !timer.alive {
            return;
        }

        timer.alive = false;
        timer.hp = 0.0;
        timer.last_taken_at = Some(runtime.time_sec);
        timer.times_taken += 1;
        timer.next_spawn_at = if timer.one_shot || timer.respawn_delay_sec.is_none() {
            None
        } else {
            Some(runtime.time_sec + timer.respawn_delay_sec.unwrap_or(0.0))
        };
        timer.label.clone()
    };

    let Some(champion_idx) = killer_idx else {
        return;
    };
    if champion_idx >= runtime.champions.len() {
        return;
    }

    let killer_id = runtime.champions[champion_idx].id.clone();
    let killer_name = runtime.champions[champion_idx].name.clone();
    let killer_team = runtime.champions[champion_idx].team.clone();
    let killer_role = runtime.champions[champion_idx].role.clone();

    if is_jungle_camp_key(key) {
        if killer_role == "SUP" && runtime.time_sec >= SUPPORT_OPEN_ROAM_AT_SEC {
            log_event(
                runtime,
                &format!("{} skipped {}", killer_name, timer_label),
                "info",
            );
            return;
        }
        if let Some((gold, xp)) = jungle_camp_reward(key) {
            let (award_gold, award_xp) = if killer_role == "JGL" {
                (
                    ((gold as f64) * JGL_JUNGLE_GOLD_MULTIPLIER).round() as i64,
                    ((xp as f64) * JGL_JUNGLE_XP_MULTIPLIER).round() as i64,
                )
            } else {
                (
                    ((gold as f64) * OFFROLE_JUNGLE_REWARD_MULTIPLIER).round() as i64,
                    ((xp as f64) * OFFROLE_JUNGLE_REWARD_MULTIPLIER).round() as i64,
                )
            };
            add_gold_xp_to_champion(runtime, &killer_id, award_gold, award_xp);
        }
        if let Some(base_cs) = jungle_camp_cs_reward(key) {
            let award_cs = if killer_role == "JGL" {
                base_cs
            } else {
                ((base_cs as f64) * OFFROLE_JUNGLE_REWARD_MULTIPLIER).round() as i64
            }
            .max(1);
            add_cs_to_champion(runtime, &killer_id, award_cs);
        }
        log_event(
            runtime,
            &format!("{} cleared {}", killer_name, timer_label),
            "info",
        );
        return;
    }

    if let Some(decision) = resolve_neutral_capture_decision(key) {
        match decision.kind {
            NeutralCaptureKind::Dragon => {
                team_stats_mut(&mut runtime.stats, &killer_team).dragons += 1;
                add_gold_xp_to_champion(runtime, &killer_id, DRAGON_SECURE_GOLD, DRAGON_SECURE_XP);
                let dragon_kind = process_dragon_capture(runtime, neutral_timers, &killer_team);
                log_event(
                    runtime,
                    &format!(
                        "{} secured {} dragon",
                        normalized_team(&killer_team).to_uppercase(),
                        dragon_kind.to_uppercase()
                    ),
                    decision.event_type,
                );
            }
            NeutralCaptureKind::Baron => {
                team_stats_mut(&mut runtime.stats, &killer_team).barons += 1;
                add_gold_xp_to_champion(runtime, &killer_id, BARON_SECURE_GOLD, BARON_SECURE_XP);
                let mut buffs = runtime_buffs_from_extra(runtime.extra.get("teamBuffs"));
                team_buffs_mut(&mut buffs, &killer_team).baron_until =
                    runtime.time_sec + BARON_BUFF_DURATION_SEC;
                set_runtime_buffs(runtime, &buffs);
                log_event(
                    runtime,
                    &format!(
                        "{} secured baron",
                        normalized_team(&killer_team).to_uppercase()
                    ),
                    decision.event_type,
                );
            }
            NeutralCaptureKind::Elder => {
                add_gold_xp_to_champion(
                    runtime,
                    &killer_id,
                    OBJECTIVE_SECURE_GOLD + 35,
                    OBJECTIVE_SECURE_XP + 55,
                );
                let mut buffs = runtime_buffs_from_extra(runtime.extra.get("teamBuffs"));
                team_buffs_mut(&mut buffs, &killer_team).elder_until =
                    runtime.time_sec + ELDER_BUFF_DURATION_SEC;
                set_runtime_buffs(runtime, &buffs);
                log_event(
                    runtime,
                    &format!(
                        "{} secured elder",
                        normalized_team(&killer_team).to_uppercase()
                    ),
                    decision.event_type,
                );
            }
            NeutralCaptureKind::Herald => {
                add_gold_xp_to_champion(
                    runtime,
                    &killer_id,
                    OBJECTIVE_SECURE_GOLD + 20,
                    OBJECTIVE_SECURE_XP + 30,
                );
                set_rift_herald_charge(runtime, &killer_team, &killer_id);
                log_event(
                    runtime,
                    &format!(
                        "{} secured rift herald",
                        normalized_team(&killer_team).to_uppercase()
                    ),
                    decision.event_type,
                );
            }
            NeutralCaptureKind::Voidgrubs => {
                add_gold_xp_to_champion(
                    runtime,
                    &killer_id,
                    OBJECTIVE_SECURE_GOLD,
                    OBJECTIVE_SECURE_XP,
                );
                log_event(
                    runtime,
                    &format!(
                        "{} cleared voidgrub camp",
                        normalized_team(&killer_team).to_uppercase()
                    ),
                    decision.event_type,
                );
            }
            NeutralCaptureKind::OtherObjective => {
                add_gold_xp_to_champion(
                    runtime,
                    &killer_id,
                    OBJECTIVE_SECURE_GOLD,
                    OBJECTIVE_SECURE_XP,
                );
                log_event(
                    runtime,
                    &format!(
                        "{} secured {}",
                        normalized_team(&killer_team).to_uppercase(),
                        timer_label
                    ),
                    decision.event_type,
                );
            }
        }
    }
}

fn attack_neutral_if_in_range(
    runtime: &mut RuntimeState,
    neutral_timers: &mut NeutralTimersRuntime,
    champion_idx: usize,
    key: &str,
) -> bool {
    let Some(timer) = neutral_timers.entities.get(key) else {
        return false;
    };
    if !timer.alive {
        return false;
    }
    if champion_idx >= runtime.champions.len() || !runtime.champions[champion_idx].alive {
        return false;
    }
    if runtime.champions[champion_idx].role == "SUP"
        && runtime.time_sec >= SUPPORT_OPEN_ROAM_AT_SEC
        && is_jungle_camp_key(key)
    {
        return false;
    }

    let distance = dist(runtime.champions[champion_idx].pos, timer.pos);
    let is_objective = is_objective_neutral_key(key);
    let max_range = if is_objective {
        OBJECTIVE_ATTEMPT_RADIUS
    } else {
        JUNGLE_CAMP_ATTACK_RADIUS
    };
    if distance > max_range
        || (!is_objective
            && !nav_grid().has_line_of_sight(runtime.champions[champion_idx].pos, timer.pos))
    {
        return false;
    }

    let damage = runtime.champions[champion_idx].attack_damage * 1.08;
    runtime.champions[champion_idx].attack_cd_until = runtime.time_sec + 0.78;

    let mut killed = false;
    let mut voidgrub_segments_gained: i64 = 0;
    if let Some(timer_mut) = neutral_timers.entities.get_mut(key) {
        let prev_hp = timer_mut.hp;
        timer_mut.hp -= damage;
        killed = timer_mut.hp <= 0.0;

        if key == "voidgrubs" {
            let prev_ratio = (prev_hp / timer_mut.max_hp).clamp(0.0, 1.0);
            let next_ratio = (timer_mut.hp.max(0.0) / timer_mut.max_hp).clamp(0.0, 1.0);
            let prev_segments_cleared = ((1.0 - prev_ratio) * 3.0).floor() as i64;
            let next_segments_cleared = ((1.0 - next_ratio) * 3.0).floor() as i64;
            voidgrub_segments_gained = (next_segments_cleared - prev_segments_cleared).max(0);
        }
    }

    if key == "voidgrubs" && voidgrub_segments_gained > 0 {
        let killer_team = runtime.champions[champion_idx].team.clone();
        let mut buffs = runtime_buffs_from_extra(runtime.extra.get("teamBuffs"));
        {
            let team_buffs = team_buffs_mut(&mut buffs, &killer_team);
            team_buffs.voidgrub_stacks =
                (team_buffs.voidgrub_stacks + voidgrub_segments_gained).clamp(0, 3);
        }
        set_runtime_buffs(runtime, &buffs);

        let killer_id = runtime.champions[champion_idx].id.clone();
        add_gold_xp_to_champion(
            runtime,
            &killer_id,
            (OBJECTIVE_SECURE_GOLD / 3) * voidgrub_segments_gained,
            (OBJECTIVE_SECURE_XP / 3) * voidgrub_segments_gained,
        );

        for _ in 0..voidgrub_segments_gained {
            log_event(
                runtime,
                &format!(
                    "{} secured voidgrub",
                    normalized_team(&killer_team).to_uppercase()
                ),
                "info",
            );
        }
    }
    if killed {
        mark_neutral_taken(runtime, neutral_timers, key, Some(champion_idx));
    }

    true
}

fn should_engage_enemy_champion(
    runtime: &RuntimeState,
    attacker_idx: usize,
    target_idx: usize,
) -> bool {
    if attacker_idx >= runtime.champions.len() || target_idx >= runtime.champions.len() {
        return false;
    }

    let attacker = &runtime.champions[attacker_idx];
    let target = &runtime.champions[target_idx];
    if !attacker.alive
        || !target.alive
        || normalized_team(&attacker.team) == normalized_team(&target.team)
    {
        return false;
    }

    let hp_ratio = if attacker.max_hp <= 0.0 {
        1.0
    } else {
        attacker.hp / attacker.max_hp
    };
    let enemy_hp_ratio = if target.max_hp <= 0.0 {
        1.0
    } else {
        target.hp / target.max_hp
    };

    let team_tactics = team_tactics_for_runtime(runtime.extra.get("teamTactics"), &attacker.team);
    let fight_plan = team_tactics.fight_plan.as_str();
    let risk_tolerance = stat_delta(attacker.competitive_score).clamp(-1.0, 1.0);
    let retaliating_recent_attacker = recent_attacker_target_idx(
        runtime,
        attacker_idx,
        LANE_CHAMPION_TRADE_RADIUS,
        ALLY_HELP_DAMAGE_RECENT_SEC,
    ) == Some(target_idx);
    let dynamic_retreat_hp_ratio =
        (runtime.policy.trade_retreat_hp_ratio - risk_tolerance * 0.05).clamp(0.24, 0.60);

    let ally_nearby = runtime
        .champions
        .iter()
        .filter(|champion| {
            champion.alive
                && normalized_team(&champion.team) == normalized_team(&attacker.team)
                && dist(champion.pos, target.pos) <= 0.12
        })
        .count();
    let enemy_nearby = runtime
        .champions
        .iter()
        .filter(|champion| {
            champion.alive
                && normalized_team(&champion.team) == normalized_team(&target.team)
                && dist(champion.pos, target.pos) <= 0.12
        })
        .count();

    if attacker.role == "MID" {
        let target_hp_ratio = if target.max_hp <= 0.0 {
            1.0
        } else {
            target.hp / target.max_hp
        };
        if hp_ratio < 0.68 {
            return false;
        }
        if target_hp_ratio > 0.42 {
            return false;
        }
        if ally_nearby <= enemy_nearby {
            return false;
        }
    }

    if attacker.role == "JGL" {
        if hp_ratio <= 0.35 {
            return false;
        }
        if enemy_nearby > ally_nearby && hp_ratio < 0.75 {
            return false;
        }
        if retaliating_recent_attacker {
            return true;
        }
    }

    let attacker_is_backline = attacker.attack_range >= 0.05;
    let attacker_is_frontline = !attacker_is_backline;

    if fight_plan == "FrontToBack" && attacker_is_backline && ally_nearby < enemy_nearby {
        return false;
    }

    if fight_plan == "Siege"
        && attacker.role != "JGL"
        && (enemy_hp_ratio > 0.45 || enemy_nearby > ally_nearby)
    {
        return false;
    }

    if attacker.role == "JGL" {
        if attacker.state != "objective" {
            return false;
        }

        let own_suffix = if normalized_team(&attacker.team) == "blue" {
            "-blue"
        } else {
            "-red"
        };
        let own_camps_alive = decode_neutral_timers_state(&runtime.neutral_timers)
            .map(|timers| {
                timers.entities.values().any(|timer| {
                    timer.alive && is_jungle_camp_key(&timer.key) && timer.key.ends_with(own_suffix)
                })
            })
            .unwrap_or(false);
        if own_camps_alive {
            return false;
        }
    }

    if attacker.state == "objective"
        && ally_nearby >= 2
        && enemy_nearby >= 1
        && enemy_hp_ratio <= 0.65
    {
        return true;
    }

    let target_under_defending_tower = runtime.structures.iter().any(|structure| {
        structure.alive
            && structure.kind == "tower"
            && normalized_team(&structure.team) == normalized_team(&target.team)
            && dist(structure.pos, target.pos) <= TOWER_AGGRO_VICTIM_RADIUS
            && dist(structure.pos, attacker.pos) <= TOWER_AGGRO_ATTACKER_RADIUS
    });

    if target_under_defending_tower && fight_plan != "Dive" {
        let secure_under_tower = enemy_hp_ratio <= 0.25
            || (ally_nearby >= enemy_nearby + 1 && hp_ratio >= enemy_hp_ratio + 0.15);
        if !secure_under_tower {
            return false;
        }
    }

    let pick_force_open = fight_plan == "Pick"
        && (attacker.role == "MID" || attacker.role == "JGL" || attacker.role == "SUP")
        && enemy_nearby <= 1
        && hp_ratio + 0.06 >= dynamic_retreat_hp_ratio;
    let dive_force_open = fight_plan == "Dive"
        && attacker_is_frontline
        && target_under_defending_tower
        && enemy_hp_ratio <= 0.55
        && hp_ratio + 0.05 >= dynamic_retreat_hp_ratio;

    if hp_ratio <= dynamic_retreat_hp_ratio {
        return false;
    }

    if !pick_force_open
        && !dive_force_open
        && !can_open_trade_window(
            attacker,
            target,
            runtime.time_sec,
            &runtime.champions,
            &runtime.minions,
            &runtime.structures,
            &runtime.lane_combat_state_by_champion,
            runtime.ai_mode,
            &runtime.policy,
        )
    {
        return false;
    }

    if !pick_force_open
        && !dive_force_open
        && should_disengage_champion_trade(
            attacker,
            target,
            runtime.time_sec,
            &runtime.champions,
            &runtime.minions,
            &runtime.structures,
            runtime.ai_mode,
            &runtime.policy,
        )
    {
        return false;
    }

    can_champion_tower_dive(runtime, attacker, target)
}

fn can_champion_tower_dive(
    runtime: &RuntimeState,
    attacker: &ChampionRuntime,
    target: &ChampionRuntime,
) -> bool {
    let defending_tower = runtime.structures.iter().find(|structure| {
        structure.alive
            && structure.kind == "tower"
            && normalized_team(&structure.team) == normalized_team(&target.team)
            && dist(structure.pos, target.pos) <= TOWER_AGGRO_VICTIM_RADIUS
            && dist(structure.pos, attacker.pos) <= TOWER_AGGRO_ATTACKER_RADIUS
    });

    let Some(tower) = defending_tower else {
        return true;
    };

    let attacker_hp_ratio = if attacker.max_hp <= 0.0 {
        1.0
    } else {
        attacker.hp / attacker.max_hp
    };
    if attacker_hp_ratio < 0.60 {
        return false;
    }
    let attacker_is_backline = attacker.attack_range >= 0.05;
    let team_tactics = team_tactics_for_runtime(runtime.extra.get("teamTactics"), &attacker.team);
    let dive_plan = team_tactics.fight_plan == "Dive";
    let front_to_back_plan = team_tactics.fight_plan == "FrontToBack";
    let no_dive_hp_min = (runtime.policy.no_dive_hp_min
        + if dive_plan {
            -0.08
        } else if front_to_back_plan {
            0.04
        } else {
            0.0
        })
    .clamp(0.2, 0.95);
    let no_dive_hp_min = if attacker_is_backline {
        (no_dive_hp_min + 0.05).clamp(0.2, 0.95)
    } else {
        no_dive_hp_min
    };

    if attacker_hp_ratio < no_dive_hp_min {
        return false;
    }

    let allied_minions_near_tower = runtime
        .minions
        .iter()
        .filter(|minion| {
            minion.alive
                && normalized_team(&minion.team) == normalized_team(&attacker.team)
                && dist(minion.pos, tower.pos) <= 0.085
        })
        .count();

    let ally_nearby = runtime
        .champions
        .iter()
        .filter(|champion| {
            champion.alive
                && normalized_team(&champion.team) == normalized_team(&attacker.team)
                && dist(champion.pos, target.pos) <= 0.12
        })
        .count();
    let frontline_ally_nearby = runtime
        .champions
        .iter()
        .filter(|champion| {
            champion.alive
                && champion.id != attacker.id
                && normalized_team(&champion.team) == normalized_team(&attacker.team)
                && champion.attack_range < 0.05
                && dist(champion.pos, target.pos) <= 0.12
        })
        .count();
    let enemy_nearby = runtime
        .champions
        .iter()
        .filter(|champion| {
            champion.alive
                && normalized_team(&champion.team) == normalized_team(&target.team)
                && dist(champion.pos, target.pos) <= 0.12
        })
        .count();

    if front_to_back_plan && attacker_is_backline && frontline_ally_nearby == 0 {
        return false;
    }

    let min_hp_without_wave = if dive_plan { 0.58 } else { 0.65 };
    if allied_minions_near_tower == 0 && attacker_hp_ratio < min_hp_without_wave {
        return false;
    }

    let mut required_allies = if dive_plan {
        enemy_nearby.saturating_sub(1)
    } else {
        enemy_nearby
    };

    if attacker_is_backline {
        required_allies = required_allies.saturating_add(1);
    }

    ally_nearby >= required_allies
}

fn attack_enemy_champion(runtime: &mut RuntimeState, attacker_idx: usize, target_idx: usize) {
    if attacker_idx == target_idx
        || attacker_idx >= runtime.champions.len()
        || target_idx >= runtime.champions.len()
    {
        return;
    }

    let now = runtime.time_sec;
    let attacker_snapshot = runtime.champions[attacker_idx].clone();
    let target_snapshot = runtime.champions[target_idx].clone();
    mark_tower_aggro_on_champion_attack(runtime, &attacker_snapshot, &target_snapshot, now);

    let attacker_has_elder = team_has_active_elder_buff(runtime, &attacker_snapshot.team);
    let attacker_micro_mult = champion_micro_damage_multiplier(&attacker_snapshot);
    let defender_hp_ratio = if target_snapshot.max_hp <= 0.0 {
        1.0
    } else {
        target_snapshot.hp / target_snapshot.max_hp
    };
    let attack_damage_multiplier =
        team_damage_multiplier(runtime, &attacker_snapshot.team, defender_hp_ratio)
            * team_damage_reduction_multiplier(runtime, &target_snapshot.team);

    let mut kill_happened = false;
    let mut victim_pos = Vec2 { x: 0.5, y: 0.5 };
    let mut victim_name = String::new();
    let mut killer_id = String::new();
    let mut killer_name = String::new();
    let mut killer_team = String::new();

    if attacker_idx < target_idx {
        let (left, right) = runtime.champions.split_at_mut(target_idx);
        let attacker = &mut left[attacker_idx];
        let defender = &mut right[0];

        let outgoing = attacker.attack_damage
            * CHAMPION_DAMAGE_TO_CHAMPION_MULTIPLIER
            * attack_damage_multiplier
            * attacker_micro_mult;
        defender.hp -= outgoing;
        defender.last_damaged_by_champion_id = Some(attacker.id.clone());
        defender.last_damaged_by_champion_at = now;
        defender.last_damaged_at = now;
        cancel_recall(defender, now, &mut runtime.events);
        attacker.attack_cd_until = now + CHAMPION_ATTACK_CADENCE_SEC;

        if attacker_has_elder
            && defender.max_hp > 0.0
            && defender.hp > 0.0
            && (defender.hp / defender.max_hp) <= ELDER_EXECUTE_HP_RATIO
        {
            defender.hp = 0.0;
        }

        if defender.hp <= 0.0 && defender.alive {
            defender.alive = false;
            defender.hp = 0.0;
            defender.deaths += 1;
            defender.respawn_at = now + champion_respawn_seconds(defender.level, now);
            attacker.kills += 1;
            kill_happened = true;
            victim_pos = defender.pos;
            victim_name = defender.name.clone();
            killer_id = attacker.id.clone();
            killer_name = attacker.name.clone();
            killer_team = attacker.team.clone();
        }
    } else {
        let (left, right) = runtime.champions.split_at_mut(attacker_idx);
        let defender = &mut left[target_idx];
        let attacker = &mut right[0];

        let outgoing = attacker.attack_damage
            * CHAMPION_DAMAGE_TO_CHAMPION_MULTIPLIER
            * attack_damage_multiplier
            * attacker_micro_mult;
        defender.hp -= outgoing;
        defender.last_damaged_by_champion_id = Some(attacker.id.clone());
        defender.last_damaged_by_champion_at = now;
        defender.last_damaged_at = now;
        cancel_recall(defender, now, &mut runtime.events);
        attacker.attack_cd_until = now + CHAMPION_ATTACK_CADENCE_SEC;

        if attacker_has_elder
            && defender.max_hp > 0.0
            && defender.hp > 0.0
            && (defender.hp / defender.max_hp) <= ELDER_EXECUTE_HP_RATIO
        {
            defender.hp = 0.0;
        }

        if defender.hp <= 0.0 && defender.alive {
            defender.alive = false;
            defender.hp = 0.0;
            defender.deaths += 1;
            defender.respawn_at = now + champion_respawn_seconds(defender.level, now);
            attacker.kills += 1;
            kill_happened = true;
            victim_pos = defender.pos;
            victim_name = defender.name.clone();
            killer_id = attacker.id.clone();
            killer_name = attacker.name.clone();
            killer_team = attacker.team.clone();
        }
    }

    if attacker_idx < runtime.champions.len() {
        let attacker_after_hit = runtime.champions[attacker_idx].clone();
        mark_lane_trade_hit(
            &attacker_after_hit,
            now,
            &mut runtime.lane_combat_state_by_champion,
        );
    }

    if !kill_happened {
        return;
    }

    let (kill_gold, kill_xp) = champion_kill_rewards(&attacker_snapshot, &target_snapshot);

    let killer_team_stats = team_stats_mut(&mut runtime.stats, &killer_team);
    killer_team_stats.kills += 1;
    add_gold_xp_to_champion(runtime, &killer_id, kill_gold, kill_xp);

    let assisters: Vec<String> = runtime
        .champions
        .iter()
        .filter(|champion| {
            champion.alive
                && normalized_team(&champion.team) == normalized_team(&killer_team)
                && champion.id != killer_id
                && dist(champion.pos, victim_pos) <= ASSIST_RADIUS
        })
        .map(|champion| champion.id.clone())
        .collect();

    if !assisters.is_empty() {
        let shared_gold = CHAMPION_ASSIST_GOLD_TOTAL / assisters.len() as i64;
        let shared_xp = (kill_xp / 2) / assisters.len() as i64;
        for assist_id in assisters {
            if let Some(champion) = runtime
                .champions
                .iter_mut()
                .find(|champion| champion.id == assist_id)
            {
                champion.assists += 1;
            }
            add_gold_xp_to_champion(runtime, &assist_id, shared_gold, shared_xp);
        }
    }

    log_event(
        runtime,
        &format!("{} killed {}", killer_name, victim_name),
        "kill",
    );
}

fn mark_tower_aggro_on_champion_attack(
    runtime: &mut RuntimeState,
    attacker: &ChampionRuntime,
    victim: &ChampionRuntime,
    now: f64,
) {
    for tower in &mut runtime.structures {
        if !tower.alive
            || tower.kind != "tower"
            || normalized_team(&tower.team) != normalized_team(&victim.team)
        {
            continue;
        }
        if dist(tower.pos, victim.pos) > TOWER_AGGRO_VICTIM_RADIUS {
            continue;
        }
        if dist(tower.pos, attacker.pos) > TOWER_AGGRO_ATTACKER_RADIUS {
            continue;
        }

        tower.forced_target_champion_id = Some(attacker.id.clone());
        tower.forced_target_until = now + TOWER_AGGRO_LOCK_SEC;
    }
}

fn award_recent_champion_kill_credit(
    runtime: &mut RuntimeState,
    victim_idx: usize,
    now: f64,
    cause: &str,
) {
    if victim_idx >= runtime.champions.len() {
        return;
    }

    let victim_snapshot = runtime.champions[victim_idx].clone();
    let Some(killer_id) = victim_snapshot.last_damaged_by_champion_id.clone() else {
        return;
    };
    if now - victim_snapshot.last_damaged_by_champion_at > CHAMPION_LAST_DAMAGE_KILL_CREDIT_SEC {
        return;
    }

    let Some(killer_idx) = runtime
        .champions
        .iter()
        .position(|champion| champion.id == killer_id)
    else {
        return;
    };
    if !runtime.champions[killer_idx].alive {
        return;
    }
    if normalized_team(&runtime.champions[killer_idx].team)
        == normalized_team(&victim_snapshot.team)
    {
        return;
    }

    let killer_snapshot = runtime.champions[killer_idx].clone();
    runtime.champions[killer_idx].kills += 1;
    let killer_team = runtime.champions[killer_idx].team.clone();

    let (kill_gold, kill_xp) = champion_kill_rewards(&killer_snapshot, &victim_snapshot);
    team_stats_mut(&mut runtime.stats, &killer_team).kills += 1;
    add_gold_xp_to_champion(runtime, &killer_id, kill_gold, kill_xp);

    log_event(
        runtime,
        &format!(
            "{} killed {} ({})",
            killer_snapshot.name, victim_snapshot.name, cause
        ),
        "kill",
    );
}

fn champion_level_from_xp(xp: i64) -> i64 {
    let mut level = 1_i64;
    for (idx, threshold) in LEVEL_XP_THRESHOLDS.iter().enumerate() {
        if xp >= *threshold {
            level = (idx + 1) as i64;
        } else {
            break;
        }
    }
    level.clamp(1, CHAMPION_MAX_LEVEL)
}

fn apply_level_scaling(champion: &mut ChampionRuntime) {
    let target_level = champion_level_from_xp(champion.xp);
    if target_level <= champion.level {
        return;
    }

    let level_delta = target_level - champion.level;
    champion.max_hp += CHAMPION_LEVEL_UP_HP_GAIN * level_delta as f64;
    champion.attack_damage += CHAMPION_LEVEL_UP_AD_GAIN * level_delta as f64;
    champion.hp =
        (champion.hp + CHAMPION_LEVEL_UP_HP_GAIN * level_delta as f64).min(champion.max_hp);
    champion.level = target_level;
}

fn champion_respawn_seconds(level: i64, now_sec: f64) -> f64 {
    let time_factor = if now_sec >= 30.0 * 60.0 {
        1.25
    } else if now_sec >= 20.0 * 60.0 {
        1.14
    } else {
        1.0
    };
    ((CHAMPION_RESPAWN_BASE_SEC + (level.max(1) - 1) as f64 * CHAMPION_RESPAWN_PER_LEVEL_SEC)
        * time_factor)
        .clamp(14.0, 58.0)
}

fn add_dragon_stack_for_kind(team_buffs: &mut RuntimeTeamBuffState, kind: &str) {
    match kind {
        "infernal" => team_buffs.infernal_stacks += 1,
        "mountain" => team_buffs.mountain_stacks += 1,
        "ocean" => team_buffs.ocean_stacks += 1,
        "cloud" => team_buffs.cloud_stacks += 1,
        "hextech" => team_buffs.hextech_stacks += 1,
        "chemtech" => team_buffs.chemtech_stacks += 1,
        _ => {}
    }
    team_buffs.dragon_stacks += 1;
}

fn tower_damage_multiplier(at_time_sec: f64, structure: &StructureRuntime) -> f64 {
    if structure.kind != "tower" {
        return 1.0;
    }

    let reduction = if at_time_sec < 20.0 * 60.0 {
        TOWER_FORTIFICATION_REDUCTION_AT_14
    } else if at_time_sec < 25.0 * 60.0 {
        TOWER_FORTIFICATION_REDUCTION_AT_20
    } else if at_time_sec < 30.0 * 60.0 {
        TOWER_FORTIFICATION_REDUCTION_AT_25
    } else if at_time_sec < 35.0 * 60.0 {
        TOWER_FORTIFICATION_REDUCTION_AT_30
    } else {
        TOWER_FORTIFICATION_REDUCTION_AT_35
    };

    1.0 - reduction
}

fn add_gold_xp_to_champion(runtime: &mut RuntimeState, champion_id: &str, gold: i64, xp: i64) {
    if let Some(champion) = runtime
        .champions
        .iter_mut()
        .find(|champion| champion.id == champion_id)
    {
        champion.gold += gold;
        champion.xp += xp;
        apply_level_scaling(champion);
        let team_stats = team_stats_mut(&mut runtime.stats, &champion.team);
        team_stats.gold += gold;
    }
}

fn add_cs_to_champion(runtime: &mut RuntimeState, champion_id: &str, cs: i64) {
    if cs <= 0 {
        return;
    }
    if let Some(champion) = runtime
        .champions
        .iter_mut()
        .find(|champion| champion.id == champion_id)
    {
        if champion.role == "SUP" && runtime.time_sec >= SUPPORT_OPEN_ROAM_AT_SEC {
            return;
        }
        champion.cs += cs;
    }
}

fn register_minion_death(runtime: &mut RuntimeState, minion_idx: usize) {
    if !runtime.minions[minion_idx].alive {
        return;
    }

    runtime.minions[minion_idx].alive = false;
    if runtime.minions[minion_idx].kind == "summon" {
        return;
    }
    let last_hit = runtime.minions[minion_idx].last_hit_by_champion_id.clone();
    let minion_team = runtime.minions[minion_idx].team.clone();
    let minion_lane = runtime.minions[minion_idx].lane.clone();
    let minion_pos = runtime.minions[minion_idx].pos;
    let gold = if runtime.minions[minion_idx].kind == "ranged" {
        44
    } else {
        60
    };
    let xp = if runtime.minions[minion_idx].kind == "ranged" {
        40
    } else {
        70
    };

    // XP soak: allies near the dying minion receive shared XP even without last-hit.
    let xp_recipients: Vec<usize> = runtime
        .champions
        .iter()
        .enumerate()
        .filter(|(_, champion)| {
            champion.alive
                && normalized_team(&champion.team) != normalized_team(&minion_team)
                && normalized_lane(&champion.lane) == normalized_lane(&minion_lane)
                && dist(champion.pos, minion_pos) <= MINION_XP_SHARE_RADIUS
        })
        .map(|(idx, _)| idx)
        .collect();

    if !xp_recipients.is_empty() {
        let shared_xp = (xp / xp_recipients.len() as i64).max(1);
        for idx in xp_recipients {
            if let Some(champion) = runtime.champions.get_mut(idx) {
                champion.xp += shared_xp;
                apply_level_scaling(champion);
            }
        }
    }

    if let Some(champion_id) = last_hit {
        let now = runtime.time_sec;
        if let Some(champion) = runtime
            .champions
            .iter_mut()
            .find(|champion| champion.id == champion_id)
        {
            let support_cs_blocked = champion.role == "SUP"
                && (now - champion.last_support_cs_at) < SUPPORT_CS_MIN_INTERVAL_SEC;

            if !support_cs_blocked {
                champion.gold += gold;
                champion.cs += 1;
                if champion.role == "SUP" {
                    champion.last_support_cs_at = now;
                }
                let team_stats = team_stats_mut(&mut runtime.stats, &champion.team);
                team_stats.gold += gold;
            }

            // Last-hit bonus XP on top of soak (keeps last-hit meaningful without breaking pacing).
            champion.xp += (xp as f64 * 0.35).round() as i64;
            apply_level_scaling(champion);
        }
    }
}

fn destroy_structure(runtime: &mut RuntimeState, structure_idx: usize, attacker_team: &str) {
    if !runtime.structures[structure_idx].alive {
        return;
    }

    runtime.structures[structure_idx].alive = false;
    runtime.structures[structure_idx].hp = 0.0;

    if runtime.structures[structure_idx].kind == "tower" {
        let team_stats = team_stats_mut(&mut runtime.stats, attacker_team);
        team_stats.towers += 1;
    }

    let event_type = if runtime.structures[structure_idx].kind == "nexus" {
        runtime.winner = Some(normalized_team(attacker_team).to_string());
        runtime.running = false;
        "nexus"
    } else {
        "tower"
    };

    log_event(
        runtime,
        &format!(
            "{} destroyed {}",
            normalized_team(attacker_team).to_uppercase(),
            runtime.structures[structure_idx].id
        ),
        event_type,
    );
}

fn nearest_enemy_champion_for_minion(
    champions: &[ChampionRuntime],
    attacker_team: &str,
    attacker_lane: &str,
    attacker_kind: &str,
    from: Vec2,
    range: f64,
) -> Option<usize> {
    champions
        .iter()
        .enumerate()
        .filter(|(_, enemy)| {
            enemy.alive
                && !champion_is_banished(enemy)
                && normalized_team(&enemy.team) != normalized_team(attacker_team)
                && (attacker_kind == "summon"
                    || normalized_lane(&enemy.lane) == normalized_lane(attacker_lane))
                && dist(enemy.pos, from) <= range
        })
        .min_by(|(idx_a, a), (idx_b, b)| {
            a.hp.partial_cmp(&b.hp)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    dist(a.pos, from)
                        .partial_cmp(&dist(b.pos, from))
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx)
}

fn nearest_enemy_minion_index(
    minions: &[MinionRuntime],
    source_idx: usize,
    range: f64,
) -> Option<usize> {
    let source = &minions[source_idx];
    minions
        .iter()
        .enumerate()
        .filter(|(idx, candidate)| {
            *idx != source_idx
                && candidate.alive
                && normalized_team(&candidate.team) != normalized_team(&source.team)
                && normalized_lane(&candidate.lane) == normalized_lane(&source.lane)
                && dist(candidate.pos, source.pos) <= range
        })
        .min_by(|(idx_a, a), (idx_b, b)| {
            dist(a.pos, source.pos)
                .partial_cmp(&dist(b.pos, source.pos))
                .unwrap_or(Ordering::Equal)
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx)
}

fn nearest_enemy_structure_index(
    structures: &[StructureRuntime],
    team: &str,
    lane: &str,
    from: Vec2,
    range: f64,
) -> Option<usize> {
    structures
        .iter()
        .enumerate()
        .filter(|(_, structure)| {
            structure.alive
                && normalized_team(&structure.team) != normalized_team(team)
                && (normalized_lane(&structure.lane) == normalized_lane(lane)
                    || structure.lane == "base")
                && is_structure_targetable(structures, team, structure)
                && dist(structure.pos, from) <= range
        })
        .min_by(|(idx_a, a), (idx_b, b)| {
            dist(a.pos, from)
                .partial_cmp(&dist(b.pos, from))
                .unwrap_or(Ordering::Equal)
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx)
}

fn nearest_enemy_structure_blocker_index(
    structures: &[StructureRuntime],
    team: &str,
    from: Vec2,
    range: f64,
) -> Option<usize> {
    structures
        .iter()
        .enumerate()
        .filter(|(_, structure)| {
            structure.alive
                && structure.kind != "nexus"
                && normalized_team(&structure.team) != normalized_team(team)
                && is_structure_targetable(structures, team, structure)
                && dist(structure.pos, from) <= range
        })
        .min_by(|(idx_a, a), (idx_b, b)| {
            let priority_a = if a.kind == "tower" { 0.0 } else { 0.035 };
            let priority_b = if b.kind == "tower" { 0.0 } else { 0.035 };
            (dist(a.pos, from) + priority_a)
                .partial_cmp(&(dist(b.pos, from) + priority_b))
                .unwrap_or(Ordering::Equal)
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx)
}

fn normalize_champion_key(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn category_plan(category: ItemBuildCategory) -> &'static [ItemTemplate; 6] {
    match category {
        ItemBuildCategory::Tank => &TANK_ITEM_PLAN,
        ItemBuildCategory::Bruiser => &BRUISER_ITEM_PLAN,
        ItemBuildCategory::Colossus => &COLOSSUS_ITEM_PLAN,
        ItemBuildCategory::AssassinAd => &ASSASSIN_AD_ITEM_PLAN,
        ItemBuildCategory::AssassinAp => &ASSASSIN_AP_ITEM_PLAN,
        ItemBuildCategory::ControlMage => &CONTROL_MAGE_ITEM_PLAN,
        ItemBuildCategory::BattleMage => &BATTLE_MAGE_ITEM_PLAN,
        ItemBuildCategory::AdcCrit => &ADC_CRIT_ITEM_PLAN,
        ItemBuildCategory::AdcAttackSpeed => &ADC_ATTACK_SPEED_ITEM_PLAN,
        ItemBuildCategory::LethalityMarksman => &LETHALITY_MARKSMAN_ITEM_PLAN,
        ItemBuildCategory::SupportEngage => &SUPPORT_ENGAGE_ITEM_PLAN,
        ItemBuildCategory::SupportEnchanter => &SUPPORT_ENCHANTER_ITEM_PLAN,
        ItemBuildCategory::SupportDamage => &SUPPORT_DAMAGE_ITEM_PLAN,
    }
}

fn champion_can_afford_next_item(champion: &ChampionRuntime) -> bool {
    if champion.items.len() >= 6 || !champion.has_left_base_once {
        return false;
    }

    let plan = champion_item_plan(&champion.role, &champion.champion_id);
    let has_boots = champion.items.iter().any(|item| is_boots_item_key(item));
    let next_item = if !has_boots {
        plan.iter()
            .find(|candidate| is_boots_item_key(candidate.key))
            .or_else(|| {
                plan.iter()
                    .find(|candidate| !champion.items.iter().any(|owned| owned == candidate.key))
            })
    } else {
        plan.iter()
            .find(|candidate| !champion.items.iter().any(|owned| owned == candidate.key))
    };

    let Some(next_item) = next_item else {
        return false;
    };

    champion.gold >= effective_item_cost(next_item.cost)
}

fn classify_item_build(role: &str, champion_id: &str) -> ItemBuildCategory {
    let champion = normalize_champion_key(champion_id);
    let c = champion.as_str();

    if role == "SUP" {
        if matches!(c, "brand" | "velkoz" | "zyra" | "xerath" | "lux") {
            return ItemBuildCategory::SupportDamage;
        }
        if matches!(
            c,
            "bard"
                | "ivern"
                | "janna"
                | "karma"
                | "lulu"
                | "milio"
                | "morgana"
                | "nami"
                | "renataglasc"
                | "seraphine"
                | "sona"
                | "soraka"
                | "yuumi"
        ) {
            return ItemBuildCategory::SupportEnchanter;
        }
        if matches!(
            c,
            "alistar"
                | "blitzcrank"
                | "braum"
                | "leona"
                | "nautilus"
                | "pyke"
                | "rakan"
                | "rell"
                | "thresh"
        ) {
            return ItemBuildCategory::SupportEngage;
        }
    }

    if role == "ADC" {
        if matches!(
            c,
            "kaisa" | "kalista" | "kogmaw" | "masteryi" | "twitch" | "varus" | "vayne" | "yunara"
        ) {
            return ItemBuildCategory::AdcAttackSpeed;
        }
        if matches!(
            c,
            "graves" | "jhin" | "kindred" | "missfortune" | "quinn" | "senna" | "smolder"
        ) {
            return ItemBuildCategory::LethalityMarksman;
        }
    }

    if matches!(
        c,
        "alistar"
            | "amumu"
            | "braum"
            | "chogath"
            | "galio"
            | "ksante"
            | "leona"
            | "malphite"
            | "maokai"
            | "nautilus"
            | "ornn"
            | "poppy"
            | "rammus"
            | "rell"
            | "sejuani"
            | "shen"
            | "sion"
            | "tahmkench"
            | "taric"
            | "zac"
    ) {
        return ItemBuildCategory::Tank;
    }

    if matches!(
        c,
        "darius"
            | "drmundo"
            | "garen"
            | "illaoi"
            | "mordekaiser"
            | "nasus"
            | "sett"
            | "shyvana"
            | "trundle"
            | "udyr"
            | "urgot"
            | "yorick"
    ) {
        return ItemBuildCategory::Colossus;
    }

    if matches!(
        c,
        "akshan"
            | "khazix"
            | "naafiri"
            | "nocturne"
            | "pyke"
            | "qiyana"
            | "rengar"
            | "shaco"
            | "talon"
            | "zed"
            | "kayn"
    ) {
        return ItemBuildCategory::AssassinAd;
    }

    if matches!(
        c,
        "akali" | "ekko" | "evelynn" | "fizz" | "kassadin" | "katarina" | "leblanc" | "nidalee"
    ) {
        return ItemBuildCategory::AssassinAp;
    }

    if matches!(
        c,
        "anivia"
            | "aurelionsol"
            | "azir"
            | "heimerdinger"
            | "hwei"
            | "lissandra"
            | "lux"
            | "malzahar"
            | "mel"
            | "neeko"
            | "orianna"
            | "ryze"
            | "syndra"
            | "taliyah"
            | "vex"
            | "viktor"
            | "xerath"
            | "ziggs"
            | "zoe"
    ) {
        return ItemBuildCategory::ControlMage;
    }

    if matches!(
        c,
        "cassiopeia"
            | "karthus"
            | "vladimir"
            | "swain"
            | "rumble"
            | "singed"
            | "sylas"
            | "gwen"
            | "lillia"
            | "morgana"
    ) {
        return ItemBuildCategory::BattleMage;
    }

    if matches!(
        c,
        "aatrox"
            | "ambessa"
            | "briar"
            | "camille"
            | "diana"
            | "ekko"
            | "elise"
            | "fiora"
            | "gnar"
            | "hecarim"
            | "irelia"
            | "jarvaniv"
            | "jax"
            | "kled"
            | "leesin"
            | "olaf"
            | "pantheon"
            | "reksai"
            | "renekton"
            | "riven"
            | "skarner"
            | "vi"
            | "volibear"
            | "warwick"
            | "wukong"
            | "xinzhao"
            | "yasuo"
            | "yone"
            | "belveth"
            | "zaahen"
    ) {
        return ItemBuildCategory::Bruiser;
    }

    if matches!(
        c,
        "aphelios"
            | "ashe"
            | "caitlyn"
            | "draven"
            | "jinx"
            | "lucian"
            | "nilah"
            | "samira"
            | "sivir"
            | "tristana"
            | "xayah"
            | "tryndamere"
    ) {
        return ItemBuildCategory::AdcCrit;
    }

    if matches!(
        c,
        "graves" | "jhin" | "kindred" | "missfortune" | "quinn" | "senna" | "smolder"
    ) {
        return ItemBuildCategory::LethalityMarksman;
    }

    match role {
        "TOP" | "JGL" => ItemBuildCategory::Bruiser,
        "MID" => ItemBuildCategory::ControlMage,
        "ADC" => ItemBuildCategory::AdcCrit,
        "SUP" => ItemBuildCategory::SupportEnchanter,
        _ => ItemBuildCategory::Bruiser,
    }
}

fn champion_item_plan(role: &str, champion_id: &str) -> &'static [ItemTemplate; 6] {
    category_plan(classify_item_build(role, champion_id))
}

fn effective_item_cost(base_cost: i64) -> i64 {
    ((base_cost as f64) * ITEM_COST_MULTIPLIER)
        .round()
        .max(ITEM_COST_MIN as f64) as i64
}

fn is_boots_item_key(key: &str) -> bool {
    matches!(
        key,
        "plated_steelcaps"
            | "mercurys_treads"
            | "boots_of_swiftness"
            | "sorcerers_shoes"
            | "berserkers_greaves"
            | "ionian_boots_of_lucidity"
            | "mobility_boots"
    )
}

fn try_auto_buy_items(runtime: &mut RuntimeState) {
    for idx in 0..runtime.champions.len() {
        {
            let champion = &mut runtime.champions[idx];
            let base_pos = base_position_for(&champion.team);
            if dist(champion.pos, base_pos) > 0.12 {
                champion.has_left_base_once = true;
            }
        }

        let (
            alive,
            role,
            champion_id,
            at_base,
            item_count,
            gold,
            name,
            owned_items,
            has_left_base_once,
        ) = {
            let champion = &runtime.champions[idx];
            (
                champion.alive,
                champion.role.clone(),
                champion.champion_id.clone(),
                dist(champion.pos, base_position_for(&champion.team)) <= 0.075,
                champion.items.len(),
                champion.gold,
                champion.name.clone(),
                champion.items.clone(),
                champion.has_left_base_once,
            )
        };

        if !alive || !at_base || item_count >= 6 || !has_left_base_once {
            continue;
        }

        let plan = champion_item_plan(&role, &champion_id);
        let has_boots = owned_items.iter().any(|item| is_boots_item_key(item));

        let next_item = if !has_boots {
            plan.iter()
                .find(|candidate| is_boots_item_key(candidate.key))
        } else {
            plan.iter()
                .find(|candidate| !owned_items.iter().any(|owned| owned == candidate.key))
        };

        let Some(next_item) = next_item else {
            continue;
        };

        let buy_cost = effective_item_cost(next_item.cost);

        if gold < buy_cost {
            continue;
        }

        let champion = &mut runtime.champions[idx];
        champion.gold -= buy_cost;
        champion.spent_gold += buy_cost;
        champion.items.push(next_item.key.to_string());
        champion.attack_damage += next_item.attack_damage;
        if next_item.max_hp > 0.0 {
            champion.max_hp += next_item.max_hp;
            champion.hp = (champion.hp + next_item.max_hp).min(champion.max_hp);
        }

        log_event(
            runtime,
            &format!("{} bought {}", name, next_item.key),
            "info",
        );
    }
}

fn team_stats_mut<'a>(stats: &'a mut RuntimeStats, team: &str) -> &'a mut RuntimeTeamStats {
    if normalized_team(team) == "red" {
        &mut stats.red
    } else {
        &mut stats.blue
    }
}

fn team_has_active_baron_buff(runtime: &RuntimeState, team: &str) -> bool {
    let buffs = team_buffs_for_runtime(runtime.extra.get("teamBuffs"), team);
    buffs.baron_until > runtime.time_sec
}

fn team_has_active_elder_buff(runtime: &RuntimeState, team: &str) -> bool {
    let buffs = team_buffs_for_runtime(runtime.extra.get("teamBuffs"), team);
    buffs.elder_until > runtime.time_sec
}

fn team_damage_multiplier(runtime: &RuntimeState, team: &str, target_hp_ratio: f64) -> f64 {
    let buffs = team_buffs_for_runtime(runtime.extra.get("teamBuffs"), team);
    let mut mult = 1.0 + buffs.infernal_stacks as f64 * 0.014;
    mult += buffs.hextech_stacks as f64 * 0.008;
    if target_hp_ratio <= 0.5 {
        mult += buffs.chemtech_stacks as f64 * 0.008;
    }
    if let Some(soul) = buffs.soul_kind.as_deref() {
        match soul {
            "infernal" => mult += 0.05,
            "hextech" => mult += 0.03,
            "chemtech" if target_hp_ratio <= 0.5 => mult += 0.04,
            _ => {}
        }
    }
    mult
}

fn team_damage_reduction_multiplier(runtime: &RuntimeState, team: &str) -> f64 {
    let buffs = team_buffs_for_runtime(runtime.extra.get("teamBuffs"), team);
    let mut reduction = (buffs.mountain_stacks as f64 * 0.02).min(0.12);
    if buffs.soul_kind.as_deref() == Some("mountain") {
        reduction += 0.08;
    }
    (1.0 - reduction).clamp(0.72, 1.0)
}

fn minion_is_baron_empowered(runtime: &RuntimeState, minion: &MinionRuntime) -> bool {
    if !team_has_active_baron_buff(runtime, &minion.team) {
        return false;
    }
    runtime.champions.iter().any(|champion| {
        champion.alive
            && normalized_team(&champion.team) == normalized_team(&minion.team)
            && dist(champion.pos, minion.pos) <= BARON_MINION_AURA_RADIUS
    })
}

fn minion_move_speed(runtime: &RuntimeState, minion: &MinionRuntime) -> f64 {
    if minion_is_baron_empowered(runtime, minion) {
        minion.move_speed * 1.12
    } else {
        minion.move_speed
    }
}

fn cleanup_tick(runtime: &mut RuntimeState) {
    runtime
        .minions
        .retain(|minion| minion.alive && minion.path_index < minion.path.len());
    runtime
        .wards
        .retain(|ward| ward.expires_at > runtime.time_sec);

    try_auto_buy_items(runtime);

    if runtime.events.len() > EVENT_CAP {
        let drain = runtime.events.len() - EVENT_CAP;
        runtime.events.drain(0..drain);
    }
}

fn move_entity(pos: &mut Vec2, target: Vec2, speed: f64, dt: f64) {
    let dd = dist(*pos, target);
    if dd <= 1e-6 {
        return;
    }
    let step = (speed * dt).min(dd);
    pos.x += ((target.x - pos.x) / dd) * step;
    pos.y += ((target.y - pos.y) / dd) * step;
}

#[cfg(test)]
mod test_helpers;

#[cfg(test)]
mod objectives_tests;

#[cfg(test)]
mod structures_tests;

#[cfg(test)]
mod combat_tests;

#[cfg(test)]
mod vision_tests;

#[cfg(test)]
mod macro_ai_tests;

#[cfg(test)]
mod runtime_tests;
