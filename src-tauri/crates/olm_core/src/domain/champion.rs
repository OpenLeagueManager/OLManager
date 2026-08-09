use serde::{Deserialize, Serialize};
#[cfg(feature = "typescript")]
use ts_rs::TS;

/// Meta tier for draft priority — how strong a champion is in the current patch.
/// Used by the coach AI to balance meta strength against player comfort/mastery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum MetaTier {
    S = 5,
    A = 4,
    B = 3,
    C = 2,
    D = 1,
}

impl MetaTier {
    pub fn score(&self) -> i32 {
        *self as i32
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_uppercase().as_str() {
            "S" => Some(Self::S),
            "A" => Some(Self::A),
            "B" => Some(Self::B),
            "C" => Some(Self::C),
            "D" => Some(Self::D),
            _ => None,
        }
    }
}

/// Represents a League of Legends champion stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct Champion {
    pub id: i64,
    pub name: String,
    pub champion_key: String,
    pub roles_json: String,
    pub counterpicks_json: Option<String>,
    pub synergies_json: Option<String>,
    pub image_tile_url: Option<String>,
    pub image_splash_url: Option<String>,
    /// Current-patch meta tier (S = must-ban/pick, D = niche).
    /// Populated from the patch champion list; defaults to B if unknown.
    pub meta_tier: MetaTier,
}

/// Input for creating a new champion (without id, which is auto-generated).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct NewChampion {
    pub name: String,
    pub champion_key: String,
    pub roles_json: String,
    pub counterpicks_json: Option<String>,
    pub synergies_json: Option<String>,
    pub image_tile_url: Option<String>,
    pub image_splash_url: Option<String>,
    pub meta_tier: MetaTier,
}
