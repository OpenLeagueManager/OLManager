use crate::domain::league::MatchType;
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
#[cfg(feature = "typescript")]
use ts_rs::TS;

/// Stats state container
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct StatsState {
    pub player_matches: Vec<PlayerMatchStatsRecord>,
    pub team_matches: Vec<TeamMatchStatsRecord>,
}

impl StatsState {
    pub fn append(&mut self, other: StatsState) {
        self.player_matches.extend(other.player_matches);
        self.team_matches.extend(other.team_matches);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum MatchOutcome {
    Win,
    #[serde(alias = "Draw")]
    #[default]
    Loss,
}

impl MatchOutcome {
    pub fn from_scores(team_score: u8, opponent_score: u8) -> Self {
        if team_score > opponent_score {
            Self::Win
        } else {
            // LoL no permite empate en el core path; cualquier no-victoria es derrota.
            Self::Loss
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum TeamSide {
    #[serde(alias = "Home")]
    #[default]
    Blue,
    #[serde(alias = "Away")]
    Red,
}

/// LoL role enum - replaces the legacy Position enum from player.rs
/// Custom deserialization handles both new LolRole strings and legacy Position strings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(rename_all = "UPPERCASE")]
pub enum LolRole {
    Top,
    Jungle,
    Mid,
    Adc,
    Support,
    #[default]
    Unknown,
}

/// Custom deserializer that maps legacy football positions to LoL roles:
///
/// Legacy Position → LolRole:
/// - Goalkeeper, DefensiveMidfielder → Support
/// - Defender, RightBack, CenterBack, LeftBack, RightWingBack, LeftWingBack → Top
/// - Midfielder, CentralMidfielder → Jungle
/// - AttackingMidfielder, RightMidfielder, LeftMidfielder → Mid
/// - Forward, RightWinger, LeftWinger, Striker → Adc
impl<'de> Deserialize<'de> for LolRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LolRoleVisitor;

        impl<'de> Visitor<'de> for LolRoleVisitor {
            type Value = LolRole;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a LoL role variant (Top, Jungle, Mid, Adc, Support, Unknown) or legacy position string or variant index")
            }

            fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    0 => Ok(LolRole::Top),
                    1 => Ok(LolRole::Jungle),
                    2 => Ok(LolRole::Mid),
                    3 => Ok(LolRole::Adc),
                    4 => Ok(LolRole::Support),
                    5 => Ok(LolRole::Unknown),
                    // Legacy Position numbers beyond LolRole range → Unknown
                    _ => Ok(LolRole::Unknown),
                }
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(value as u32)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                // First try direct LolRole match (handles PascalCase, UPPERCASE, lowercase)
                match value {
                    "Top" | "TOP" | "top" => Ok(LolRole::Top),
                    "Jungle" | "JUNGLE" | "jungle" => Ok(LolRole::Jungle),
                    "Mid" | "MID" | "mid" => Ok(LolRole::Mid),
                    "Adc" | "ADC" | "adc" => Ok(LolRole::Adc),
                    "Support" | "SUPPORT" | "support" => Ok(LolRole::Support),
                    "Unknown" | "UNKNOWN" | "unknown" => Ok(LolRole::Unknown),
                    _ => {
                        // Fall back to legacy position mapping
                        let role = match value {
                            // Goalkeeper/Defensive → Support
                            "Goalkeeper" | "DefensiveMidfielder" => LolRole::Support,
                            // Defender variants → Top
                            "Defender" | "RightBack" | "CenterBack" | "LeftBack"
                            | "RightWingBack" | "LeftWingBack" => LolRole::Top,
                            // Midfielder variants → Jungle
                            "Midfielder" | "CentralMidfielder" => LolRole::Jungle,
                            // Attacking midfield → Mid
                            "AttackingMidfielder" | "RightMidfielder" | "LeftMidfielder" => {
                                LolRole::Mid
                            }
                            // Forward variants → ADC
                            "Forward" | "RightWinger" | "LeftWinger" | "Striker" => LolRole::Adc,
                            // Unknown legacy position
                            _ => LolRole::Unknown,
                        };
                        Ok(role)
                    }
                }
            }
        }

        deserializer.deserialize_any(LolRoleVisitor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct PlayerMatchStatsRecord {
    pub fixture_id: String,
    pub season: u32,
    pub matchday: u32,
    pub date: String,
    #[serde(alias = "competition")]
    pub match_type: MatchType,
    pub player_id: String,
    pub team_id: String,
    pub opponent_team_id: String,
    pub side: TeamSide,
    pub result: MatchOutcome,
    pub role: LolRole,
    #[serde(alias = "champion_id")]
    pub champion: Option<String>,
    pub duration_seconds: u32,
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    pub creep_score: u16,
    pub gold_earned: u32,
    pub damage_dealt: u32,
    pub vision_score: u16,
    pub wards_placed: u16,
    #[serde(default)]
    pub bans_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[serde(default)]
pub struct TeamMatchStatsRecord {
    pub fixture_id: String,
    pub season: u32,
    pub matchday: u32,
    pub date: String,
    #[serde(alias = "competition")]
    pub match_type: MatchType,
    pub team_id: String,
    pub opponent_team_id: String,
    pub side: TeamSide,
    pub result: MatchOutcome,
    pub duration_seconds: u32,
    pub kills: u16,
    pub deaths: u16,
    pub gold_earned: u32,
    pub damage_dealt: u32,
    pub objectives: u16,
}
