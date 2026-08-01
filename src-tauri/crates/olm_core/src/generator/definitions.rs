use crate::domain::team::TeamColors;
use serde::{Deserialize, Serialize};

/// Serialisable world database — can be saved to / loaded from JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldData {
    pub name: String,
    pub description: String,
    pub teams: Vec<crate::domain::team::Team>,
    pub players: Vec<crate::domain::player::Player>,
    pub staff: Vec<crate::domain::staff::Staff>,
}

// ---------------------------------------------------------------------------
// Competition / multi-league definition types
// ---------------------------------------------------------------------------

/// Top-level manifest for a single competition (league / tournament).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetitionManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub full_name: Option<String>,
    pub region: String,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub tier: Option<u8>,
    #[serde(default)]
    pub logo: Option<String>,
    pub schedule: ScheduleConfig,
    #[serde(default = "default_teams_file")]
    pub teams_file: String,
    #[serde(default = "default_players_file")]
    pub players_file: String,
    #[serde(default)]
    pub staff_file: Option<String>,
    #[serde(default)]
    pub championships_file: Option<String>,
    #[serde(default)]
    pub erls: Vec<String>,
    /// ERL reputation (used for academy cost calculation).
    #[serde(default)]
    pub reputation: Option<u8>,
    /// Nearby country codes for cross-border ERL eligibility.
    #[serde(default)]
    pub nearby_country_codes: Vec<String>,
    /// True for legacy/template competitions that should not be loaded into the game.
    /// Legacy competitions are hidden from selection and skipped during world assembly.
    #[serde(default)]
    pub legacy: bool,
    /// True for competitions that should appear in the league selection and in-game tabs.
    #[serde(default)]
    pub active: bool,
}

fn default_teams_file() -> String {
    "teams.json".to_string()
}

fn default_players_file() -> String {
    "players.json".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    pub format: String,
    pub team_count: u32,
    pub splits: Vec<SplitConfig>,
    #[serde(default = "default_preseason_friendlies")]
    pub preseason_friendlies: u32,
}

fn default_preseason_friendlies() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitConfig {
    pub name: String,
    pub season_start: SeasonStart,
    #[serde(default)]
    pub superweek_offsets: Vec<i64>,
    #[serde(default = "default_best_of")]
    pub best_of: u32,
    #[serde(default)]
    pub playoffs: Option<PlayoffConfig>,
}

fn default_best_of() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonStart {
    pub month: u32,
    pub day: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayoffConfig {
    pub format: String,
    pub teams: u32,
}

// ---------------------------------------------------------------------------
// Team / player / staff data file types (Flow C — per-competition JSON)
// ---------------------------------------------------------------------------

/// Wrapper for `data/teams/*.json` files.
/// Supports both the full `crate::domain::team::Team` format (with `name`/`description`)
/// and the simplified format (with `competition_id`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamDataFile {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub competition_id: Option<String>,
    pub teams: Vec<crate::domain::team::Team>,
}

/// Wrapper for `data/players/*.json` files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerDataFile {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub competition_id: Option<String>,
    pub players: Vec<crate::domain::player::Player>,
}

/// Wrapper for `data/staffs/free_agents.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaffDataFile {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub staff: Vec<crate::domain::staff::Staff>,
}

// ---------------------------------------------------------------------------
// League selection screen types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LeagueSelectionData {
    pub competitions: Vec<CompetitionSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompetitionSummary {
    pub id: String,
    pub name: String,
    pub region: String,
    #[serde(default)]
    pub logo: Option<String>,
    pub tier: u8,
    #[serde(default)]
    pub legacy: bool,
    #[serde(default)]
    pub active: bool,
    pub team_count: u32,
    pub teams: Vec<TeamSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamSummary {
    pub id: String,
    pub name: String,
    pub short_name: String,
    #[serde(default)]
    pub logo_url: Option<String>,
    pub country: String,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub finance: Option<i64>,
    #[serde(default)]
    pub reputation: Option<u32>,
    #[serde(default)]
    pub colors: Option<TeamColors>,
    #[serde(default)]
    pub ovr: Option<u8>,
    #[serde(default)]
    pub player_count: Option<usize>,
}

// ---------------------------------------------------------------------------
// World database types
// ---------------------------------------------------------------------------

/// Lightweight metadata shown in the UI when listing available databases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldDatabaseInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub team_count: usize,
    pub player_count: usize,
    /// "builtin" | "user"
    pub source: String,
    /// Filesystem path (empty for built-in random)
    pub path: String,
}
