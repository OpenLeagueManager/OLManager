use serde::{Deserialize, Serialize};
#[cfg(feature = "typescript")]
use ts_rs::TS;

pub use crate::domain::stats::LolRole;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct Player {
    pub id: String,
    pub match_name: String,
    pub full_name: String,
    #[serde(
        default,
        deserialize_with = "crate::domain::serde_util::null_to_default"
    )]
    pub date_of_birth: String,
    #[serde(default)]
    pub nationality: String,
    #[serde(default)]
    pub birth_country: Option<String>,
    #[serde(default)]
    pub profile_image_url: Option<String>,

    /// Player's current role in the team (set by formation)
    pub position: LolRole,

    /// The player's natural/preferred role (never changed by formation logic)
    #[serde(default)]
    pub natural_position: LolRole,

    /// Alternate roles this player can also play (with reduced effectiveness)
    #[serde(default)]
    pub alternate_positions: Vec<LolRole>,

    // Core attributes 0-100
    #[serde(default)]
    pub attributes: PlayerAttributes,

    // Dynamic match/season values
    #[serde(default = "default_condition")]
    pub condition: u8, // 0-100 (short-term energy; depletes during matches, recovers daily)
    #[serde(default = "default_morale")]
    pub morale: u8, // 0-100
    /// Long-term physical shape (0–100). Determines how fast condition depletes and
    /// recovers. Changes slowly over weeks.
    #[serde(default = "default_fitness")]
    pub fitness: u8,

    pub team_id: Option<String>,

    // Traits / flairs derived from attributes
    #[serde(default)]
    pub traits: Vec<PlayerTrait>,

    // Contract & value
    pub contract_end: Option<String>,
    #[serde(
        default = "default_wage",
        deserialize_with = "crate::domain::serde_util::lenient_u32"
    )]
    pub wage: u32, // annual wage
    #[serde(
        default = "default_market_value",
        deserialize_with = "crate::domain::serde_util::lenient_u64"
    )]
    pub market_value: u64,

    // Season stats — defaulted because some exported records omit them.
    #[serde(default)]
    pub stats: PlayerSeasonStats,

    // Career history
    #[serde(default)]
    pub career: Vec<CareerEntry>,

    // Individual training focus override (takes priority over group and team default)
    #[serde(default)]
    pub training_focus: Option<crate::domain::team::TrainingFocus>,

    // Transfer status
    #[serde(default)]
    pub transfer_listed: bool,
    #[serde(default)]
    pub loan_listed: bool,
    #[serde(default)]
    pub transfer_offers: Vec<TransferOffer>,
    #[serde(default)]
    pub morale_core: PlayerMoraleCore,
    #[serde(default = "default_potential_base")]
    pub potential_base: u8,
    #[serde(default)]
    pub potential_revealed: Option<u8>,
    #[serde(default)]
    pub potential_research_started_on: Option<String>,
    #[serde(default)]
    pub potential_research_eta_days: Option<u8>,
    #[serde(default)]
    pub champion_training_targets: Vec<String>,
    #[serde(default)]
    pub can_be_transferred_until: Option<String>,
    /// Whether this player was released via `release_player_contract`.
    /// Used to exclude them from end-of-season free agent re-assignment.
    #[serde(default)]
    pub was_released: bool,

    /// Pre-computed LoL OVR (average of 9 visible stats, 1-99).
    /// Computed by olm_core::potential::calculate_lol_ovr and serialized to frontend.
    #[serde(default)]
    pub lol_ovr: u8,
    /// Cached SoloQ LP — populated daily by process_daily_champion_system.
    /// The frontend reads this field directly from gameState.players
    /// instead of issuing a separate IPC invoke for the global ranking.
    #[serde(default)]
    pub soloq_lp: f64,
}

/// Footedness is deprecated - LoL roles are lane-agnostic
/// Kept for backward compatibility with legacy save files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum Footedness {
    Left,
    #[default]
    Right,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct PlayerAttributes {
    // These 9 attributes are used by the engine simulation.
    // Aliases provide backward compat with old save files (football-era names + removed fields).
    /// Mechanical skill — replaces reaction_speed
    #[serde(alias = "dribbling", alias = "reaction_speed")]
    pub mechanics: u8,

    /// Lane phase skill
    #[serde(alias = "shooting")]
    pub laning: u8,

    /// Teamfight performance — replaces coordination
    #[serde(default = "default_attr", alias = "teamwork", alias = "coordination")]
    pub teamfighting: u8,

    /// Macro / map awareness — replaces interception
    #[serde(alias = "vision", alias = "interception")]
    pub macro_play: u8,

    /// Consistency — replaces positioning
    #[serde(alias = "decisions", alias = "positioning")]
    pub consistency: u8,

    /// Leadership / decision-making — replaces aggression
    #[serde(default = "default_attr", alias = "leadership", alias = "aggression")]
    pub shotcalling: u8,

    /// Champion versatility
    #[serde(default = "default_attr", alias = "agility")]
    pub champion_pool: u8,

    /// Discipline / composure — replaces positional_defense
    #[serde(
        default = "default_attr",
        alias = "composure",
        alias = "positional_defense"
    )]
    pub discipline: u8,

    /// Mental resilience / stamina — replaces durability
    #[serde(alias = "stamina", alias = "durability")]
    pub mental_resilience: u8,
}

impl PlayerAttributes {
    pub fn overall(&self) -> u8 {
        ((u32::from(self.mechanics)
            + u32::from(self.laning)
            + u32::from(self.teamfighting)
            + u32::from(self.macro_play)
            + u32::from(self.consistency)
            + u32::from(self.shotcalling)
            + u32::from(self.champion_pool)
            + u32::from(self.discipline)
            + u32::from(self.mental_resilience))
            / 9) as u8
    }
}

fn default_attr() -> u8 {
    50
}

fn default_fitness() -> u8 {
    75
}

fn default_condition() -> u8 {
    100
}

fn default_morale() -> u8 {
    70
}

fn default_wage() -> u32 {
    50_000
}

fn default_market_value() -> u64 {
    750_000
}

fn default_potential_base() -> u8 {
    99
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum PlayerIssueCategory {
    Contract,
    PlayingTime,
    Morale,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct PlayerIssue {
    pub category: PlayerIssueCategory,
    pub severity: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct RecentTreatmentMemory {
    pub action_key: String,
    pub times_recently_used: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum PlayerPromiseKind {
    PlayingTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum RenewalSessionStatus {
    #[default]
    Idle,
    Open,
    Agreed,
    Blocked,
    Stalled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum RenewalSessionOutcome {
    #[default]
    None,
    AcceptedByManager,
    AcceptedByAssistant,
    RejectedByPlayer,
    BlockedByManager,
    Stalled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct ContractRenewalState {
    pub status: RenewalSessionStatus,
    pub manager_blocked_until: Option<String>,
    pub last_attempt_date: Option<String>,
    pub last_assistant_attempt_date: Option<String>,
    pub last_outcome: Option<RenewalSessionOutcome>,
    pub conversation_round: u8,
}

impl Default for ContractRenewalState {
    fn default() -> Self {
        Self {
            status: RenewalSessionStatus::Idle,
            manager_blocked_until: None,
            last_attempt_date: None,
            last_assistant_attempt_date: None,
            last_outcome: None,
            conversation_round: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct PlayerPromise {
    pub kind: PlayerPromiseKind,
    pub matches_remaining: u8,
}

impl Default for PlayerPromise {
    fn default() -> Self {
        Self {
            kind: PlayerPromiseKind::PlayingTime,
            matches_remaining: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct PlayerMoraleCore {
    pub manager_trust: u8,
    pub unresolved_issue: Option<PlayerIssue>,
    pub recent_treatment: Option<RecentTreatmentMemory>,
    pub pending_promise: Option<PlayerPromise>,
    pub talk_cooldown_until: Option<String>,
    pub renewal_state: Option<ContractRenewalState>,
}

impl Default for PlayerMoraleCore {
    fn default() -> Self {
        Self {
            manager_trust: 50,
            unresolved_issue: None,
            recent_treatment: None,
            pending_promise: None,
            talk_cooldown_until: None,
            renewal_state: None,
        }
    }
}

fn default_transfer_offer_status() -> TransferOfferStatus {
    TransferOfferStatus::Pending
}

fn default_transfer_offer_date() -> String {
    String::new()
}

fn default_transfer_offer_round() -> u8 {
    0
}

fn default_transfer_offer_destination_team_id() -> Option<String> {
    None
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct PlayerSeasonStats {
    #[serde(default)]
    pub appearances: u32,
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub assists: u32,
    #[serde(default)]
    pub avg_rating: f32,
    #[serde(default)]
    pub minutes_played: u32,
    #[serde(default)]
    pub shots: u32,
    #[serde(default)]
    pub shots_on_target: u32,
    #[serde(default)]
    pub passes_completed: u32,
    #[serde(default)]
    pub passes_attempted: u32,
    #[serde(default)]
    pub tackles_won: u32,
    #[serde(default)]
    pub interceptions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct CareerEntry {
    pub season: u32,
    pub team_id: Option<String>,
    pub team_name: String,
    #[serde(default)]
    pub appearances: u32,
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub deaths: u32,
    #[serde(default)]
    pub assists: u32,
    #[serde(default)]
    pub avg_rating: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct TransferOffer {
    pub id: String,
    pub from_team_id: String,
    #[serde(default = "default_transfer_offer_destination_team_id")]
    pub destination_team_id: Option<String>,
    pub fee: u64,
    pub wage_offered: u32,
    #[serde(default)]
    pub last_manager_fee: Option<u64>,
    #[serde(default = "default_transfer_offer_round")]
    pub negotiation_round: u8,
    #[serde(default)]
    pub suggested_counter_fee: Option<u64>,
    #[serde(default)]
    pub players_included: Vec<PlayerOfferItem>,
    #[serde(default = "default_transfer_offer_status")]
    pub status: TransferOfferStatus,
    #[serde(default = "default_transfer_offer_date")]
    pub date: String,
    #[serde(default = "default_wage_neg_status")]
    pub wage_negotiation_status: WageNegotiationStatus,
    #[serde(default)]
    pub contract_years_offered: u8,
    #[serde(default)]
    pub suggested_counter_wage: Option<u32>,
    #[serde(default)]
    pub suggested_counter_years: Option<u8>,
    #[serde(default = "default_wage_neg_round")]
    pub wage_negotiation_round: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct PlayerOfferItem {
    pub player_id: String,
    pub player_name: String,
    pub valuation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(rename_all = "PascalCase")]
pub enum WageNegotiationStatus {
    NotStarted,
    Pending,
    Agreed,
    Rejected,
}

fn default_wage_neg_status() -> WageNegotiationStatus {
    WageNegotiationStatus::NotStarted
}
fn default_wage_neg_round() -> u8 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum TransferOfferStatus {
    Pending,
    Accepted,
    Rejected,
    Withdrawn,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum PlayerTrait {
    // Mechanics
    #[serde(alias = "Speedster")]
    LightningQuick, // mechanics >= 85
    #[serde(alias = "Tank")]
    Immovable, // mental_resilience >= 85
    #[serde(alias = "Agile")]
    NimbleFingers, // champion_pool >= 85
    #[serde(alias = "Tireless")]
    MarathonMan, // mental_resilience >= 90
    // Game Knowledge
    #[serde(alias = "Playmaker")]
    GameManager, // teamfighting >= 80 && macro_play >= 80
    #[serde(alias = "Sharpshooter")]
    Lethal, // laning >= 85
    #[serde(alias = "Dribbler")]
    KiteMaster, // mechanics >= 85
    #[serde(alias = "BallWinner")]
    Interceptor, // macro_play >= 80 && shotcalling >= 70
    #[serde(alias = "Rock")]
    Sentinel, // discipline >= 85 && consistency >= 75
    // Mental
    #[serde(alias = "Leader")]
    ShotCaller, // shotcalling >= 85 && teamfighting >= 75
    #[serde(alias = "CoolHead")]
    IceCold, // discipline >= 85 && consistency >= 80
    #[serde(alias = "Visionary")]
    Visionary, // macro_play >= 85
    #[serde(alias = "HotHead")]
    Intimidator, // shotcalling >= 85 && discipline < 50
    #[serde(alias = "TeamPlayer")]
    TeamPlayer, // teamfighting >= 85
    // Special
    #[serde(alias = "CompleteForward")]
    HyperCarry, // laning >= 75 && mechanics >= 75 && mental_resilience >= 70
    #[serde(alias = "Engine")]
    Workhorse, // mental_resilience >= 85 && mechanics >= 70 && teamfighting >= 75
    #[serde(alias = "SetPieceSpecialist")]
    MacroSpecialist, // teamfighting >= 80 && laning >= 75 && macro_play >= 75
}

/// Derive traits purely from a player's attributes (role-independent).
pub fn compute_traits(attrs: &PlayerAttributes, _role: &LolRole) -> Vec<PlayerTrait> {
    let mut traits = Vec::new();

    // Mechanics
    if attrs.mechanics >= 85 {
        traits.push(PlayerTrait::LightningQuick);
    }
    if attrs.mental_resilience >= 85 {
        traits.push(PlayerTrait::Immovable);
    }
    if attrs.mental_resilience >= 85 {
        traits.push(PlayerTrait::Immovable);
    }
    if attrs.champion_pool >= 85 {
        traits.push(PlayerTrait::NimbleFingers);
    }
    if attrs.mental_resilience >= 90 {
        traits.push(PlayerTrait::MarathonMan);
    }

    // Game Knowledge
    if attrs.teamfighting >= 80 && attrs.macro_play >= 80 {
        traits.push(PlayerTrait::GameManager);
    }
    if attrs.laning >= 85 {
        traits.push(PlayerTrait::Lethal);
    }
    if attrs.mechanics >= 85 {
        traits.push(PlayerTrait::KiteMaster);
    }
    if attrs.macro_play >= 80 && attrs.shotcalling >= 70 {
        traits.push(PlayerTrait::Interceptor);
    }
    if attrs.discipline >= 85 && attrs.consistency >= 75 {
        traits.push(PlayerTrait::Sentinel);
    }

    // Mental
    if attrs.shotcalling >= 85 && attrs.teamfighting >= 75 {
        traits.push(PlayerTrait::ShotCaller);
    }
    if attrs.discipline >= 85 && attrs.consistency >= 80 {
        traits.push(PlayerTrait::IceCold);
    }
    if attrs.macro_play >= 85 {
        traits.push(PlayerTrait::Visionary);
    }
    if attrs.shotcalling >= 85 && attrs.discipline < 50 {
        traits.push(PlayerTrait::Intimidator);
    }
    if attrs.teamfighting >= 85 {
        traits.push(PlayerTrait::TeamPlayer);
    }

    // Special — purely attribute-based
    if attrs.laning >= 75 && attrs.mechanics >= 75 && attrs.mental_resilience >= 70 {
        traits.push(PlayerTrait::HyperCarry);
    }
    if attrs.mental_resilience >= 85 && attrs.mechanics >= 70 && attrs.teamfighting >= 75 {
        traits.push(PlayerTrait::Workhorse);
    }
    if attrs.teamfighting >= 80 && attrs.laning >= 75 && attrs.macro_play >= 75 {
        traits.push(PlayerTrait::MacroSpecialist);
    }

    traits
}

impl Player {
    pub fn new<R: Into<LolRole>>(
        id: String,
        match_name: String,
        full_name: String,
        date_of_birth: String,
        nationality: String,
        role: R,
        attributes: PlayerAttributes,
    ) -> Self {
        let role: LolRole = role.into();
        let traits = compute_traits(&attributes, &role);
        let birth_country = crate::domain::identity::derive_birth_country_code(&nationality);
        Self {
            id,
            match_name,
            full_name,
            date_of_birth,
            nationality,
            birth_country,
            profile_image_url: None,
            natural_position: role,
            position: role,
            alternate_positions: Vec::new(),
            attributes,
            condition: 100,
            morale: 100,
            fitness: 75,
            team_id: None,
            traits,
            contract_end: None,
            wage: 0,
            market_value: 0,
            stats: PlayerSeasonStats::default(),
            career: Vec::new(),
            training_focus: None,
            transfer_listed: false,
            loan_listed: false,
            transfer_offers: Vec::new(),
            morale_core: PlayerMoraleCore::default(),
            potential_base: default_potential_base(),
            potential_revealed: None,
            potential_research_started_on: None,
            potential_research_eta_days: None,
            champion_training_targets: Vec::new(),
            can_be_transferred_until: None,
            was_released: false,
            lol_ovr: 0,
            soloq_lp: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_attributes() -> PlayerAttributes {
        PlayerAttributes {
            mechanics: 69,
            laning: 61,
            teamfighting: 76,
            macro_play: 73,
            consistency: 71,
            shotcalling: 49,
            champion_pool: 68,
            discipline: 66,
            mental_resilience: 72,
        }
    }

    #[test]
    fn legacy_football_position_deserializes_to_lol_role() {
        // Test that legacy Position strings are correctly mapped to LolRole
        // "Midfielder" (legacy) -> LolRole::Jungle (as per spec)
        let player: Player = serde_json::from_value(serde_json::json!({
            "id": "p-legacy",
            "match_name": "J. Legacy",
            "full_name": "John Legacy",
            "date_of_birth": "2000-01-15",
            "nationality": "GB",
            "position": "Midfielder",
            "natural_position": "Midfielder",
            "alternate_positions": [],
            "attributes": sample_attributes(),
            "condition": 100,
            "morale": 100,
            "team_id": null,
            "traits": [],
            "contract_end": null,
            "wage": 0,
            "market_value": 0,
            "stats": {},
            "career": [],
            "transfer_listed": false,
            "loan_listed": false,
            "transfer_offers": [],
            "morale_core": {}
        }))
        .expect("legacy player json should deserialize");

        // "Midfielder" should map to LolRole::Jungle per the spec
        assert_eq!(player.natural_position, LolRole::Jungle);
        assert_eq!(player.potential_base, 99);
        assert_eq!(player.potential_revealed, None);
    }

    #[test]
    fn new_lol_role_string_deserializes_directly() {
        // Test that new LolRole strings deserialize correctly
        let player: Player = serde_json::from_value(serde_json::json!({
            "id": "p-new",
            "match_name": "J. New",
            "full_name": "John New",
            "date_of_birth": "2000-01-15",
            "nationality": "GB",
            "position": "Top",
            "natural_position": "Top",
            "alternate_positions": ["Jungle", "Mid"],
            "attributes": sample_attributes(),
            "condition": 100,
            "morale": 100,
            "team_id": null,
            "traits": [],
            "contract_end": null,
            "wage": 0,
            "market_value": 0,
            "stats": {},
            "career": [],
            "transfer_listed": false,
            "loan_listed": false,
            "transfer_offers": [],
            "morale_core": {}
        }))
        .expect("new player json should deserialize");

        assert_eq!(player.position, LolRole::Top);
        assert_eq!(player.natural_position, LolRole::Top);
        assert_eq!(
            player.alternate_positions,
            vec![LolRole::Jungle, LolRole::Mid]
        );
    }

    #[test]
    fn was_released_flag_serde_roundtrip() {
        // Serialize a player with was_released=true, deserialize, verify it survives
        let mut player: Player = serde_json::from_value(serde_json::json!({
            "id": "p-wr",
            "match_name": "W. Released",
            "full_name": "Will Released",
            "date_of_birth": "2000-01-15",
            "nationality": "GB",
            "position": "Top",
            "natural_position": "Top",
            "alternate_positions": [],
            "attributes": sample_attributes(),
            "condition": 100,
            "morale": 100,
            "team_id": null,
            "traits": [],
            "contract_end": null,
            "wage": 0,
            "market_value": 0,
            "stats": {},
            "career": [],
            "transfer_listed": false,
            "loan_listed": false,
            "transfer_offers": [],
            "morale_core": {},
            "was_released": true
        }))
        .expect("player with was_released=true should deserialize");

        assert!(
            player.was_released,
            "was_released should be true after deserialization"
        );

        // Roundtrip through serde
        let json = serde_json::to_string(&player).expect("should serialize");
        let restored: Player = serde_json::from_str(&json).expect("should deserialize roundtrip");

        assert!(
            restored.was_released,
            "was_released should survive serde roundtrip"
        );

        // Default (missing field) should be false
        let default_player: Player = serde_json::from_value(serde_json::json!({
            "id": "p-dflt",
            "match_name": "D. Fault",
            "full_name": "Default Fault",
            "date_of_birth": "2000-01-15",
            "nationality": "GB",
            "position": "Top",
            "natural_position": "Top",
            "alternate_positions": [],
            "attributes": sample_attributes(),
            "condition": 100,
            "morale": 100,
            "team_id": null,
            "traits": [],
            "contract_end": null,
            "wage": 0,
            "market_value": 0,
            "stats": {},
            "career": [],
            "transfer_listed": false,
            "loan_listed": false,
            "transfer_offers": [],
            "morale_core": {}
        }))
        .expect("player without was_released should deserialize");

        assert!(
            !default_player.was_released,
            "was_released should default to false"
        );
    }
}
