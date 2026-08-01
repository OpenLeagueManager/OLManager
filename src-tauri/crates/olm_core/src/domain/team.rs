use serde::{Deserialize, Serialize};
#[cfg(feature = "typescript")]
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct Team {
    pub id: String,
    pub name: String,
    pub short_name: String,
    pub country: String,
    pub city: String,
    #[serde(default, alias = "arena_name")]
    pub stadium_name: String,
    #[serde(default, alias = "arena_capacity")]
    pub stadium_capacity: u32,

    // Current state
    #[serde(default)]
    pub finance: i64,
    pub manager_id: Option<String>,
    #[serde(default)]
    pub reputation: u32,

    // Academy affiliation metadata. Defaults keep legacy saves and existing teams as main clubs.
    #[serde(default)]
    pub team_kind: TeamKind,
    #[serde(default)]
    pub parent_team_id: Option<String>,
    #[serde(default)]
    pub academy_team_id: Option<String>,
    #[serde(default)]
    pub academy: Option<AcademyMetadata>,
    #[serde(default)]
    pub logo_url: Option<String>,
    #[serde(default)]
    pub competition_id: Option<String>,

    // Financial breakdown
    #[serde(default)]
    pub wage_budget: i64,
    #[serde(default)]
    pub transfer_budget: i64,
    #[serde(default)]
    pub season_income: i64,
    #[serde(default)]
    pub season_expenses: i64,
    #[serde(default)]
    pub financial_ledger: Vec<FinancialTransaction>,
    #[serde(default)]
    pub sponsorship: Option<Sponsorship>,
    #[serde(default)]
    pub facilities: Facilities,

    // Tactical
    #[serde(default, alias = "play_style")]
    pub draft_strategy: DraftStrategy,
    #[serde(default)]
    pub lol_tactics: LolTactics,

    // Training
    #[serde(default)]
    pub training_focus: TrainingFocus,
    #[serde(default)]
    pub training_intensity: TrainingIntensity,
    #[serde(default)]
    pub training_schedule: TrainingSchedule,

    // Club info
    #[serde(default)]
    pub founded_year: u32,
    #[serde(default)]
    pub colors: TeamColors,

    // Training groups: allow per-group focus overrides for subsets of players
    #[serde(default)]
    pub training_groups: Vec<TrainingGroup>,

    // Weekly scrim plan: ordered opponent team IDs.
    #[serde(default)]
    pub weekly_scrim_opponent_ids: Vec<String>,
    // Per-slot fallback plan. Each slot stores ordered opponent IDs: Plan A, Plan B, Plan C.
    #[serde(default)]
    pub weekly_scrim_plan_team_ids: Vec<Vec<String>>,
    // Optional weekly intent. When unset, reports infer focus from observed issues.
    #[serde(default)]
    pub scrim_weekly_objective: Option<ScrimFocus>,
    // 0 means legacy/default capacity; otherwise effective weekly scrim slots.
    #[serde(default)]
    pub scrim_weekly_slots: u8,
    // Optional week key when manager explicitly locks weekly scrim setup.
    #[serde(default)]
    pub scrim_setup_locked_week_key: Option<String>,
    #[serde(default = "default_scrim_reputation")]
    pub scrim_reputation: u8,
    #[serde(default)]
    pub scrim_weekly_cancellations: u8,
    #[serde(default)]
    pub scrim_loss_streak: u8,
    #[serde(default)]
    pub scrim_weekly_played: u8,
    #[serde(default)]
    pub scrim_weekly_wins: u8,
    #[serde(default)]
    pub scrim_weekly_losses: u8,
    #[serde(default)]
    pub scrim_slot_results: Vec<ScrimSlotResult>,
    #[serde(default)]
    pub scrim_reports: Vec<ScrimReport>,

    // Persistent active League of Legends lineup (player IDs). If empty, auto-select by OVR.
    #[serde(default, alias = "starting_xi_ids")]
    pub active_lineup_ids: Vec<String>,

    #[serde(default)]
    pub team_roles: TeamRoles,

    // Recent form: last 5 results as "W", "D", "L" (most recent last)
    #[serde(default)]
    pub form: Vec<String>,

    // History
    #[serde(default)]
    pub history: Vec<TeamSeasonRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum TeamKind {
    #[default]
    Main,
    Academy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct AcademyMetadata {
    pub lifecycle: AcademyLifecycle,
    pub erl_assignment: ErlAssignment,
    #[serde(default)]
    pub source_team_id: String,
    #[serde(default)]
    pub original_name: String,
    #[serde(default)]
    pub original_short_name: String,
    #[serde(default)]
    pub original_logo_url: Option<String>,
    #[serde(default)]
    pub current_logo_url: Option<String>,
    #[serde(default)]
    pub acquisition_cost: i64,
    #[serde(default)]
    pub acquired_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum AcademyLifecycle {
    Planned,
    #[default]
    Active,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct ErlAssignment {
    pub erl_league_id: String,
    pub country_rule: ErlAssignmentRule,
    #[serde(default)]
    pub fallback_reason: Option<String>,
    pub reputation: u8,
    #[serde(default)]
    pub acquisition_cost: i64,
    #[serde(default)]
    pub acquired_at: String,
    #[serde(default)]
    pub creation_cost: i64,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum ErlAssignmentRule {
    Domestic,
    Fallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct LolTactics {
    #[serde(default)]
    pub strong_side: StrongSide,
    #[serde(default)]
    pub game_timing: GameTiming,
    #[serde(default)]
    pub jungle_style: JungleStyle,
    #[serde(default)]
    pub jungle_pathing: JunglePathing,
    #[serde(default)]
    pub fight_plan: FightPlan,
    #[serde(default)]
    pub support_roaming: SupportRoaming,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum StrongSide {
    Top,
    Mid,
    #[default]
    Bot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum GameTiming {
    #[default]
    Unknown,
    Early,
    Mid,
    Late,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum JungleStyle {
    Ganker,
    Invader,
    Farmer,
    Carry,
    #[default]
    Enabler,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum JunglePathing {
    #[default]
    TopToBot,
    BotToTop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum FightPlan {
    #[default]
    FrontToBack,
    Pick,
    Dive,
    Siege,
    Flank,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum SupportRoaming {
    #[default]
    Lane,
    #[serde(alias = "Roam")]
    RoamMid,
    RoamTop,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct TeamRoles {
    pub captain: Option<String>,
    pub shotcaller: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum TrainingFocus {
    #[default]
    #[serde(rename = "Scrims", alias = "Physical", alias = "General")]
    Scrims,
    #[serde(rename = "VODReview", alias = "Defending")]
    VODReview,
    #[serde(rename = "IndividualCoaching", alias = "Attacking")]
    IndividualCoaching,
    #[serde(rename = "ChampionPoolPractice", alias = "Technical")]
    ChampionPoolPractice,
    #[serde(rename = "MacroSystems", alias = "Tactical")]
    MacroSystems,
    #[serde(rename = "MentalResetRecovery", alias = "Recovery")]
    MentalResetRecovery,
}

impl TrainingFocus {
    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "Scrims" | "Physical" | "General" => Some(Self::Scrims),
            "VODReview" | "Defending" => Some(Self::VODReview),
            "IndividualCoaching" | "Attacking" => Some(Self::IndividualCoaching),
            "ChampionPoolPractice" | "Technical" => Some(Self::ChampionPoolPractice),
            "MacroSystems" | "Tactical" => Some(Self::MacroSystems),
            "MentalResetRecovery" | "Recovery" => Some(Self::MentalResetRecovery),
            _ => None,
        }
    }

    pub fn as_id(&self) -> &'static str {
        match self {
            Self::Scrims => "Scrims",
            Self::VODReview => "VODReview",
            Self::IndividualCoaching => "IndividualCoaching",
            Self::ChampionPoolPractice => "ChampionPoolPractice",
            Self::MacroSystems => "MacroSystems",
            Self::MentalResetRecovery => "MentalResetRecovery",
        }
    }

    pub fn is_recovery_plan(&self) -> bool {
        matches!(self, Self::MentalResetRecovery)
    }
}

fn default_scrim_reputation() -> u8 {
    50
}

#[cfg(test)]
mod training_focus_tests {
    use super::TrainingFocus;

    #[test]
    fn maps_legacy_focus_ids_to_new_training_plans() {
        assert_eq!(
            TrainingFocus::from_id("Physical"),
            Some(TrainingFocus::Scrims)
        );
        assert_eq!(
            TrainingFocus::from_id("Technical"),
            Some(TrainingFocus::ChampionPoolPractice)
        );
        assert_eq!(
            TrainingFocus::from_id("Tactical"),
            Some(TrainingFocus::MacroSystems)
        );
        assert_eq!(
            TrainingFocus::from_id("Defending"),
            Some(TrainingFocus::VODReview)
        );
        assert_eq!(
            TrainingFocus::from_id("Attacking"),
            Some(TrainingFocus::IndividualCoaching)
        );
        assert_eq!(
            TrainingFocus::from_id("Recovery"),
            Some(TrainingFocus::MentalResetRecovery)
        );
    }

    #[test]
    fn serde_aliases_support_old_save_values() {
        let focus: TrainingFocus = serde_json::from_str("\"Technical\"").unwrap();
        assert_eq!(focus, TrainingFocus::ChampionPoolPractice);
    }
}

#[cfg(test)]
mod academy_team_metadata_tests {
    use super::{
        AcademyLifecycle, AcademyMetadata, ErlAssignment, ErlAssignmentRule, Team, TeamKind,
    };

    #[test]
    fn new_teams_default_to_main_without_academy_links() {
        let team = Team::new(
            "g2".to_string(),
            "G2 Esports".to_string(),
            "G2".to_string(),
            "DE".to_string(),
            "Berlin".to_string(),
            "G2 Arena".to_string(),
            10_000,
        );

        assert_eq!(team.team_kind, TeamKind::Main);
        assert!(team.is_main());
        assert!(!team.is_academy());
        assert_eq!(team.parent_team_id, None);
        assert_eq!(team.academy_team_id, None);
        assert_eq!(team.academy, None);
    }

    #[test]
    fn old_save_without_academy_fields_deserializes_as_main_team() {
        let team: Team = serde_json::from_value(serde_json::json!({
            "id": "fnc",
            "name": "Fnatic",
            "short_name": "FNC",
            "country": "GB",
            "city": "London",
            "stadium_name": "Fnatic HQ",
            "stadium_capacity": 5000,
            "finance": 1000000,
            "manager_id": null,
            "reputation": 500,
            "wage_budget": 200000,
            "transfer_budget": 500000,
            "season_income": 0,
            "season_expenses": 0,
            "play_style": "Balanced",
            "founded_year": 1900,
            "colors": { "primary": "#000000", "secondary": "#ffffff" },
            "history": []
        }))
        .unwrap();

        assert_eq!(team.team_kind, TeamKind::Main);
        assert_eq!(team.parent_team_id, None);
        assert_eq!(team.academy_team_id, None);
        assert_eq!(team.academy, None);
    }

    #[test]
    fn legacy_starting_xi_ids_deserializes_as_active_lineup() {
        let team: Team = serde_json::from_value(serde_json::json!({
            "id": "g2",
            "name": "G2 Esports",
            "short_name": "G2",
            "country": "DE",
            "city": "Berlin",
            "stadium_name": "G2 Arena",
            "stadium_capacity": 10000,
            "finance": 1000000,
            "manager_id": null,
            "reputation": 500,
            "wage_budget": 200000,
            "transfer_budget": 500000,
            "season_income": 0,
            "season_expenses": 0,
            "play_style": "Balanced",
            "founded_year": 1900,
            "colors": { "primary": "#000000", "secondary": "#ffffff" },
            "starting_xi_ids": ["top", "jungle", "mid", "adc", "support"],
            "history": []
        }))
        .unwrap();

        assert_eq!(
            team.active_lineup_ids,
            vec!["top", "jungle", "mid", "adc", "support"]
        );
    }

    #[test]
    fn active_lineup_ids_serializes_as_preferred_field() {
        let mut team = Team::new(
            "g2".to_string(),
            "G2 Esports".to_string(),
            "G2".to_string(),
            "DE".to_string(),
            "Berlin".to_string(),
            "G2 Arena".to_string(),
            10_000,
        );
        team.active_lineup_ids = vec!["top".to_string(), "jungle".to_string()];

        let json = serde_json::to_value(&team).unwrap();

        assert_eq!(
            json["active_lineup_ids"],
            serde_json::json!(["top", "jungle"])
        );
        assert!(json.get("starting_xi_ids").is_none());
    }

    #[test]
    fn academy_team_metadata_carries_parent_link_and_erl_assignment() {
        let assignment = ErlAssignment {
            erl_league_id: "lfl".to_string(),
            country_rule: ErlAssignmentRule::Domestic,
            fallback_reason: None,
            reputation: 5,
            acquisition_cost: 300_000,
            acquired_at: "2026-04-26".to_string(),
            creation_cost: 300_000,
            created_at: "2026-04-26".to_string(),
        };

        let metadata = AcademyMetadata {
            lifecycle: AcademyLifecycle::Active,
            erl_assignment: assignment.clone(),
            source_team_id: "karmine-corp-blue".to_string(),
            original_name: "Karmine Corp Blue".to_string(),
            original_short_name: "KCB".to_string(),
            original_logo_url: Some("logos/kcb.svg".to_string()),
            current_logo_url: None,
            acquisition_cost: 300_000,
            acquired_at: "2026-04-26".to_string(),
        };

        assert_eq!(metadata.lifecycle, AcademyLifecycle::Active);
        assert_eq!(metadata.erl_assignment, assignment);
        assert_eq!(metadata.source_team_id, "karmine-corp-blue");
        assert_eq!(metadata.original_name, "Karmine Corp Blue");
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum TrainingIntensity {
    Low,
    #[default]
    Medium,
    High,
}

/// Weekly training schedule controlling how many days per week are training vs rest.
/// Rest days give full condition recovery with no training cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum TrainingSchedule {
    /// 6 training days, 1 rest (Sunday). Max growth, minimal recovery.
    Intense,
    /// 4 training days (Mon, Tue, Thu, Fri), 3 rest (Wed, Sat, Sun). Good balance.
    #[default]
    Balanced,
    /// 2 training days (Tue, Thu), 5 rest. Minimal growth, excellent recovery.
    Light,
}

impl TrainingSchedule {
    /// Returns true if the given weekday (chrono::Weekday) is a training day.
    /// Mon=0, Tue=1, Wed=2, Thu=3, Fri=4, Sat=5, Sun=6
    pub fn is_training_day(&self, weekday_num: u32) -> bool {
        match self {
            // Intense: rest only on Sunday (6)
            TrainingSchedule::Intense => weekday_num != 6,
            // Balanced: train Mon(0), Tue(1), Thu(3), Fri(4); rest Wed(2), Sat(5), Sun(6)
            TrainingSchedule::Balanced => matches!(weekday_num, 0 | 1 | 3 | 4),
            // Light: train Tue(1), Thu(3) only
            TrainingSchedule::Light => matches!(weekday_num, 1 | 3),
        }
    }

    /// Human-readable description of training days per week.
    pub fn training_days_per_week(&self) -> u8 {
        match self {
            TrainingSchedule::Intense => 6,
            TrainingSchedule::Balanced => 4,
            TrainingSchedule::Light => 2,
        }
    }
}

/// A named training group with its own focus. Players in a group train
/// with the group's focus instead of the team-wide default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct TrainingGroup {
    pub id: String,
    pub name: String,
    pub focus: TrainingFocus,
    pub player_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct ScrimSlotResult {
    pub week_key: String,
    pub slot_index: u8,
    pub weekday: u8,
    pub opponent_team_id: String,
    pub won: bool,
    pub simulated_on: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum ScrimStatus {
    Pending,
    Accepted,
    Rejected,
    Cancelled,
    Played,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum ScrimFocus {
    DraftPrep,
    ChampionPool,
    EarlyGame,
    Teamfighting,
    Macro,
    Mental,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum ScrimIssue {
    DraftGap,
    LanePressure,
    ObjectiveSetup,
    TeamfightExecution,
    ChampionComfort,
    Tilt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum PostScrimDecision {
    ContinuePlan,
    VodReview,
    MentalReset,
    TargetedDrills,
    PushThrough,
    DayOff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct ScrimChampionPick {
    pub player_id: String,
    pub champion_id: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct ScrimReport {
    pub date: String,
    pub week_key: String,
    pub slot_index: u8,
    pub weekday: u8,
    pub team_id: String,
    pub opponent_team_id: String,
    pub status: ScrimStatus,
    pub won: Option<bool>,
    pub focus: ScrimFocus,
    pub issue: Option<ScrimIssue>,
    pub severity: u8,
    pub quality: u8,
    pub player_champion_picks: Vec<ScrimChampionPick>,
    pub post_decision: Option<PostScrimDecision>,
    pub created_on: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct TeamColors {
    #[serde(
        default,
        deserialize_with = "crate::domain::serde_util::null_to_default"
    )]
    pub primary: String,
    #[serde(
        default,
        deserialize_with = "crate::domain::serde_util::null_to_default"
    )]
    pub secondary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum DraftStrategy {
    #[default]
    Balanced,
    #[serde(rename = "Attacking", alias = "HighPress")]
    Aggressive,
    #[serde(rename = "Defensive")]
    Passive,
    #[serde(rename = "Possession")]
    Scaling,
    #[serde(rename = "Counter")]
    CounterPick,
    PriorityBans,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct TeamSeasonRecord {
    pub season: u32,
    pub league_position: u32,
    pub played: u32,
    pub won: u32,
    pub lost: u32,
    pub kills_for: u32,
    pub kills_against: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum FinancialTransactionKind {
    Salary,
    StaffWage,
    FacilityUpkeep,
    FacilityUpgrade,
    TransferPurchase,
    TransferSale,
    ReleasePenalty,
    AcademyAcquisition,
    Sponsorship,
    MatchdayRevenue,
    PrizeMoney,
    BudgetRefresh,
    Other,
}

impl Serialize for FinancialTransactionKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            FinancialTransactionKind::Salary => "Salary",
            FinancialTransactionKind::StaffWage => "StaffWage",
            FinancialTransactionKind::FacilityUpkeep => "FacilityUpkeep",
            FinancialTransactionKind::FacilityUpgrade => "FacilityUpgrade",
            FinancialTransactionKind::TransferPurchase => "TransferPurchase",
            FinancialTransactionKind::TransferSale => "TransferSale",
            FinancialTransactionKind::ReleasePenalty => "ReleasePenalty",
            FinancialTransactionKind::AcademyAcquisition => "AcademyAcquisition",
            FinancialTransactionKind::Sponsorship => "Sponsorship",
            FinancialTransactionKind::MatchdayRevenue => "MatchdayRevenue",
            FinancialTransactionKind::PrizeMoney => "PrizeMoney",
            FinancialTransactionKind::BudgetRefresh => "BudgetRefresh",
            FinancialTransactionKind::Other => "Other",
        })
    }
}

impl<'de> Deserialize<'de> for FinancialTransactionKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "Salary" => FinancialTransactionKind::Salary,
            "StaffWage" => FinancialTransactionKind::StaffWage,
            "FacilityUpkeep" => FinancialTransactionKind::FacilityUpkeep,
            "FacilityUpgrade" => FinancialTransactionKind::FacilityUpgrade,
            "TransferPurchase" => FinancialTransactionKind::TransferPurchase,
            "TransferSale" => FinancialTransactionKind::TransferSale,
            "ReleasePenalty" => FinancialTransactionKind::ReleasePenalty,
            "AcademyAcquisition" => FinancialTransactionKind::AcademyAcquisition,
            "Sponsorship" => FinancialTransactionKind::Sponsorship,
            "MatchdayRevenue" => FinancialTransactionKind::MatchdayRevenue,
            "PrizeMoney" => FinancialTransactionKind::PrizeMoney,
            "BudgetRefresh" => FinancialTransactionKind::BudgetRefresh,
            "Other" => FinancialTransactionKind::Other,
            _ => FinancialTransactionKind::Other,
        })
    }
}

impl Default for FinancialTransactionKind {
    fn default() -> Self {
        FinancialTransactionKind::Other
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct FinancialTransaction {
    #[serde(default)]
    pub id: String,
    pub date: String,
    pub description: String,
    pub amount: i64,
    #[serde(default)]
    pub kind: FinancialTransactionKind,
    #[serde(default)]
    pub balance_before: i64,
    #[serde(default)]
    pub balance_after: i64,
    #[serde(default = "default_financial_transaction_source")]
    pub source: String,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

fn default_financial_transaction_source() -> String {
    "legacy".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum SponsorshipBonusCriterion {
    LeaguePosition {
        max_position: u32,
        bonus_amount: i64,
    },
    UnbeatenRun {
        required_matches: usize,
        bonus_amount: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct Sponsorship {
    pub sponsor_name: String,
    /// Total annual value (paid monthly as base_value / 12)
    pub base_value: i64,
    pub remaining_months: u32,
    pub bonus_criteria: Vec<SponsorshipBonusCriterion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum FacilityType {
    Training,
    Medical,
    Scouting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct Facilities {
    #[serde(default = "default_main_hub_level")]
    pub main_hub_level: u8,
    pub training: u8,
    pub medical: u8,
    pub scouting: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrims_room_level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_room_level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootcamp_area_level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_suite_level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_studio_level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scouting_lab_level: Option<u8>,
}

fn default_main_hub_level() -> u8 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum MainFacilityModuleKind {
    ScrimsRoom,
    AnalysisRoom,
    BootcampArea,
    RecoverySuite,
    ContentStudio,
    ScoutingLab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum MainFacilityModuleLevelSource {
    Training,
    Medical,
    Hub,
    Scouting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct MainFacilityModuleDefinition {
    pub kind: MainFacilityModuleKind,
    pub level_source: MainFacilityModuleLevelSource,
}

impl MainFacilityModuleDefinition {
    pub fn level_for(&self, facilities: &Facilities) -> u8 {
        match self.kind {
            MainFacilityModuleKind::ScrimsRoom => {
                facilities.scrims_room_level.unwrap_or(facilities.training)
            }
            MainFacilityModuleKind::AnalysisRoom => facilities
                .analysis_room_level
                .unwrap_or(facilities.training),
            MainFacilityModuleKind::BootcampArea => {
                facilities.bootcamp_area_level.unwrap_or(facilities.medical)
            }
            MainFacilityModuleKind::RecoverySuite => facilities
                .recovery_suite_level
                .unwrap_or(facilities.medical),
            MainFacilityModuleKind::ContentStudio => {
                facilities.content_studio_level.unwrap_or_else(|| {
                    facilities
                        .main_hub_level
                        .max(facilities.training)
                        .max(facilities.medical)
                        .max(facilities.scouting)
                        .max(facilities.scrims_room_level.unwrap_or(0))
                        .max(facilities.analysis_room_level.unwrap_or(0))
                        .max(facilities.bootcamp_area_level.unwrap_or(0))
                        .max(facilities.recovery_suite_level.unwrap_or(0))
                        .max(facilities.scouting_lab_level.unwrap_or(0))
                })
            }
            MainFacilityModuleKind::ScoutingLab => {
                facilities.scouting_lab_level.unwrap_or(facilities.scouting)
            }
        }
    }
}

const MAIN_FACILITY_MODULE_CATALOG: [MainFacilityModuleDefinition; 6] = [
    MainFacilityModuleDefinition {
        kind: MainFacilityModuleKind::ScrimsRoom,
        level_source: MainFacilityModuleLevelSource::Training,
    },
    MainFacilityModuleDefinition {
        kind: MainFacilityModuleKind::AnalysisRoom,
        level_source: MainFacilityModuleLevelSource::Training,
    },
    MainFacilityModuleDefinition {
        kind: MainFacilityModuleKind::BootcampArea,
        level_source: MainFacilityModuleLevelSource::Medical,
    },
    MainFacilityModuleDefinition {
        kind: MainFacilityModuleKind::RecoverySuite,
        level_source: MainFacilityModuleLevelSource::Medical,
    },
    MainFacilityModuleDefinition {
        kind: MainFacilityModuleKind::ContentStudio,
        level_source: MainFacilityModuleLevelSource::Hub,
    },
    MainFacilityModuleDefinition {
        kind: MainFacilityModuleKind::ScoutingLab,
        level_source: MainFacilityModuleLevelSource::Scouting,
    },
];

pub fn main_facility_module_catalog() -> &'static [MainFacilityModuleDefinition] {
    &MAIN_FACILITY_MODULE_CATALOG
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct MainFacilityModuleView {
    pub kind: MainFacilityModuleKind,
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct MainFacilityHubView {
    pub level: u8,
    pub modules: Vec<MainFacilityModuleView>,
}

impl Default for Facilities {
    fn default() -> Self {
        Self {
            main_hub_level: default_main_hub_level(),
            training: 1,
            medical: 1,
            scouting: 1,
            scrims_room_level: None,
            analysis_room_level: None,
            bootcamp_area_level: None,
            recovery_suite_level: None,
            content_studio_level: None,
            scouting_lab_level: None,
        }
    }
}

impl Facilities {
    fn effective_main_hub_level(&self) -> u8 {
        let module_peak = main_facility_module_catalog()
            .iter()
            .map(|definition| definition.level_for(self))
            .max()
            .unwrap_or(default_main_hub_level());

        self.main_hub_level
            .max(self.training)
            .max(self.medical)
            .max(self.scouting)
            .max(module_peak)
    }

    pub fn from_persisted_json(value: &str) -> Self {
        serde_json::from_str::<Self>(value).unwrap_or_default()
    }

    pub fn to_persisted_json_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    pub fn to_persisted_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn as_main_facility_hub(&self) -> MainFacilityHubView {
        let level = self.effective_main_hub_level();

        MainFacilityHubView {
            level,
            modules: main_facility_module_catalog()
                .iter()
                .map(|definition| MainFacilityModuleView {
                    kind: definition.kind,
                    level: definition.level_for(self),
                })
                .collect(),
        }
    }

    pub fn module_level(&self, module: MainFacilityModuleKind) -> u8 {
        main_facility_module_catalog()
            .iter()
            .find(|definition| definition.kind == module)
            .map(|definition| definition.level_for(self))
            .unwrap_or(default_main_hub_level())
    }

    pub fn can_upgrade_main_facility_module(&self, module: MainFacilityModuleKind) -> bool {
        self.module_level(module).saturating_add(1) <= self.as_main_facility_hub().level
    }

    pub fn recovery_suite_condition_multiplier(&self) -> f64 {
        1.0 + f64::from(
            self.module_level(MainFacilityModuleKind::RecoverySuite)
                .saturating_sub(1),
        ) * 0.1
    }
}

#[cfg(test)]
mod facility_compatibility_tests {
    use super::{Facilities, MainFacilityModuleKind, main_facility_module_catalog};

    #[test]
    fn canonical_module_catalog_is_the_single_source_for_hub_order_and_levels() {
        let facilities = Facilities {
            main_hub_level: 4,
            training: 3,
            medical: 2,
            scouting: 1,
            ..Default::default()
        };

        let catalog = main_facility_module_catalog();
        let hub = facilities.as_main_facility_hub();

        assert_eq!(catalog.len(), 6);
        assert_eq!(hub.modules.len(), catalog.len());
        assert_eq!(catalog[0].kind, MainFacilityModuleKind::ScrimsRoom);
        assert_eq!(catalog[1].kind, MainFacilityModuleKind::AnalysisRoom);
        assert_eq!(catalog[2].kind, MainFacilityModuleKind::BootcampArea);
        assert_eq!(catalog[3].kind, MainFacilityModuleKind::RecoverySuite);
        assert_eq!(catalog[4].kind, MainFacilityModuleKind::ContentStudio);
        assert_eq!(catalog[5].kind, MainFacilityModuleKind::ScoutingLab);
        assert_eq!(
            hub.modules
                .iter()
                .map(|module| (module.kind, module.level))
                .collect::<Vec<_>>(),
            catalog
                .iter()
                .map(|definition| (definition.kind, definition.level_for(&facilities)))
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn legacy_facilities_are_interpreted_as_one_main_facility_with_modules() {
        let facilities = Facilities {
            main_hub_level: 1,
            training: 3,
            medical: 1,
            scouting: 2,
            ..Default::default()
        };

        let hub = facilities.as_main_facility_hub();

        assert_eq!(hub.level, 3);
        assert_eq!(hub.modules.len(), 6);
        assert_eq!(hub.modules[0].kind, MainFacilityModuleKind::ScrimsRoom);
        assert_eq!(hub.modules[0].level, 3);
        assert_eq!(hub.modules[1].kind, MainFacilityModuleKind::AnalysisRoom);
        assert_eq!(hub.modules[1].level, 3);
        assert_eq!(hub.modules[2].kind, MainFacilityModuleKind::BootcampArea);
        assert_eq!(hub.modules[2].level, 1);
        assert_eq!(hub.modules[3].kind, MainFacilityModuleKind::RecoverySuite);
        assert_eq!(hub.modules[3].level, 1);
        assert_eq!(hub.modules[4].kind, MainFacilityModuleKind::ContentStudio);
        assert_eq!(hub.modules[4].level, 3);
        assert_eq!(hub.modules[5].kind, MainFacilityModuleKind::ScoutingLab);
        assert_eq!(hub.modules[5].level, 2);
    }

    #[test]
    fn canonical_module_effect_levels_are_derived_from_legacy_storage() {
        let facilities = Facilities {
            main_hub_level: 4,
            training: 3,
            medical: 2,
            scouting: 1,
            ..Default::default()
        };

        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::ScrimsRoom),
            3
        );
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::AnalysisRoom),
            3
        );
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::BootcampArea),
            2
        );
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::RecoverySuite),
            2
        );
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::ContentStudio),
            4
        );
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::ScoutingLab),
            1
        );
    }

    #[test]
    fn recovery_suite_condition_multiplier_is_a_canonical_gameplay_helper() {
        let default_facilities = Facilities::default();
        let upgraded_facilities = Facilities {
            medical: 4,
            ..Facilities::default()
        };

        assert_eq!(
            default_facilities.recovery_suite_condition_multiplier(),
            1.0
        );
        assert_eq!(
            upgraded_facilities.recovery_suite_condition_multiplier(),
            1.3
        );
    }

    #[test]
    fn partial_legacy_facilities_deserialize_to_safe_default_modules() {
        let facilities: Facilities = serde_json::from_str(r#"{"training":4}"#).unwrap();

        let hub = facilities.as_main_facility_hub();

        assert_eq!(hub.level, 4);
        assert_eq!(hub.modules[0].level, 4);
        assert_eq!(hub.modules[1].level, 4);
        assert_eq!(hub.modules[2].level, 1);
        assert_eq!(hub.modules[3].level, 1);
        assert_eq!(hub.modules[4].level, 4);
        assert_eq!(hub.modules[5].level, 1);
        assert_eq!(
            serde_json::to_value(&facilities).unwrap(),
            serde_json::json!({
                "main_hub_level": 1,
                "training": 4,
                "medical": 1,
                "scouting": 1,
            }),
        );
    }

    #[test]
    fn explicit_hub_expansion_level_controls_next_module_unlocks() {
        let mut facilities = Facilities::default();

        assert_eq!(facilities.as_main_facility_hub().level, 1);
        assert!(
            !facilities.can_upgrade_main_facility_module(MainFacilityModuleKind::RecoverySuite)
        );

        facilities.main_hub_level = 2;
        let hub = facilities.as_main_facility_hub();

        assert_eq!(hub.level, 2);
        assert!(facilities.can_upgrade_main_facility_module(MainFacilityModuleKind::RecoverySuite));
        assert!(facilities.can_upgrade_main_facility_module(MainFacilityModuleKind::ScoutingLab));
    }

    #[test]
    fn legacy_module_levels_still_expand_the_hub_for_old_saves() {
        let facilities: Facilities = serde_json::from_str(r#"{"medical":3,"scouting":1}"#).unwrap();

        assert_eq!(facilities.as_main_facility_hub().level, 3);
        assert!(facilities.can_upgrade_main_facility_module(MainFacilityModuleKind::ScoutingLab));
        assert!(
            !facilities.can_upgrade_main_facility_module(MainFacilityModuleKind::RecoverySuite)
        );
    }

    #[test]
    fn persisted_old_facilities_blob_loads_as_hub_contract() {
        let facilities = Facilities::from_persisted_json(r#"{"training":5,"medical":2}"#);
        let hub = facilities.as_main_facility_hub();

        assert_eq!(facilities.training, 5);
        assert_eq!(facilities.medical, 2);
        assert_eq!(facilities.scouting, 1);
        assert_eq!(hub.level, 5);
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::ScrimsRoom),
            5
        );
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::AnalysisRoom),
            5
        );
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::BootcampArea),
            2
        );
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::RecoverySuite),
            2
        );
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::ContentStudio),
            5
        );
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::ScoutingLab),
            1
        );
    }

    #[test]
    fn persisted_hub_contract_blob_roundtrips_without_losing_legacy_keys() {
        let facilities = Facilities::from_persisted_json(
            r#"{"main_hub_level":4,"training":2,"medical":3,"scouting":1}"#,
        );

        assert_eq!(facilities.as_main_facility_hub().level, 4);
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::ContentStudio),
            4
        );
        assert_eq!(
            facilities.module_level(MainFacilityModuleKind::RecoverySuite),
            3
        );
        assert_eq!(
            facilities.to_persisted_json_value().unwrap(),
            serde_json::json!({
                "main_hub_level": 4,
                "training": 2,
                "medical": 3,
                "scouting": 1,
            }),
        );
    }
}

impl Team {
    pub fn is_main(&self) -> bool {
        self.team_kind == TeamKind::Main
    }

    pub fn is_academy(&self) -> bool {
        self.team_kind == TeamKind::Academy
    }

    pub fn new(
        id: String,
        name: String,
        short_name: String,
        country: String,
        city: String,
        stadium_name: String,
        stadium_capacity: u32,
    ) -> Self {
        Self {
            id,
            name,
            short_name,
            country,
            city,
            stadium_name,
            stadium_capacity,
            finance: 1_000_000,
            manager_id: None,
            reputation: 500,
            team_kind: TeamKind::Main,
            parent_team_id: None,
            academy_team_id: None,
            academy: None,
            logo_url: None,
            competition_id: None,
            wage_budget: 200_000,
            transfer_budget: 500_000,
            season_income: 0,
            season_expenses: 0,
            financial_ledger: Vec::new(),
            sponsorship: None,
            facilities: Facilities::default(),
            draft_strategy: DraftStrategy::Balanced,
            lol_tactics: LolTactics::default(),
            training_focus: TrainingFocus::default(),
            training_intensity: TrainingIntensity::default(),
            training_schedule: TrainingSchedule::default(),
            training_groups: Vec::new(),
            weekly_scrim_opponent_ids: Vec::new(),
            weekly_scrim_plan_team_ids: Vec::new(),
            scrim_weekly_objective: None,
            scrim_weekly_slots: 0,
            scrim_setup_locked_week_key: None,
            scrim_reputation: default_scrim_reputation(),
            scrim_weekly_cancellations: 0,
            scrim_loss_streak: 0,
            scrim_weekly_played: 0,
            scrim_weekly_wins: 0,
            scrim_weekly_losses: 0,
            scrim_slot_results: Vec::new(),
            scrim_reports: Vec::new(),
            founded_year: 1900,
            colors: TeamColors {
                primary: "#10b981".to_string(),
                secondary: "#ffffff".to_string(),
            },
            active_lineup_ids: Vec::new(),
            team_roles: TeamRoles::default(),
            form: Vec::new(),
            history: Vec::new(),
        }
    }
}
