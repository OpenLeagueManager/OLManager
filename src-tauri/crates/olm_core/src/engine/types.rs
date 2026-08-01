use serde::{Deserialize, Serialize};

// Re-export LolRole from live_match module for use in this crate
pub use crate::engine::sim_background::LolRole;

// ---------------------------------------------------------------------------
// DraftStrategy — mirrors crate::domain::team::DraftStrategy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
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

// ---------------------------------------------------------------------------
// PlayerData — a snapshot of a player for engine consumption
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerData {
    pub id: String,
    pub name: String,
    /// Optional explicit player portrait URL carried from the domain roster.
    #[serde(default)]
    pub profile_image_url: Option<String>,
    /// Player's LoL role (Top, Jungle, Mid, Adc, Support)
    pub role: LolRole,
    pub condition: u8, // 0-100
    /// Long-term physical shape (0-100). Multiplies stamina depletion rate in-match.
    #[serde(default = "default_fitness")]
    pub fitness: u8,

    // LoL Attributes
    pub mechanics: u8,
    pub laning: u8,
    pub teamfighting: u8,
    pub macro_play: u8,
    pub consistency: u8,
    pub shotcalling: u8,
    pub champion_pool: u8,
    pub discipline: u8,
    pub mental_resilience: u8,

    // Traits (string names matching crate::domain::player::PlayerTrait variants)
    #[serde(default)]
    pub traits: Vec<String>,
}

fn default_fitness() -> u8 {
    75
}

impl PlayerData {
    /// Overall rating (simple mean of 9 visible LoL stats, matching calculate_lol_ovr).
    pub fn overall(&self) -> f64 {
        (self.mechanics as f64
            + self.laning as f64
            + self.teamfighting as f64
            + self.macro_play as f64
            + self.consistency as f64
            + self.shotcalling as f64
            + self.champion_pool as f64
            + self.discipline as f64
            + self.mental_resilience as f64)
            / 9.0
    }
}

// ---------------------------------------------------------------------------
// TeamData — everything the engine needs to know about one side
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamData {
    pub id: String,
    pub name: String,
    pub draft_strategy: DraftStrategy,
    pub players: Vec<PlayerData>,
}

impl TeamData {
    /// Count players by role.
    pub fn count_role(&self, role: LolRole) -> usize {
        self.players.iter().filter(|p| p.role == role).count()
    }

    /// Average of a specific attribute among players in the given role.
    pub fn role_attr_avg(&self, role: LolRole, attr_fn: fn(&PlayerData) -> u8) -> f64 {
        let players: Vec<_> = self.players.iter().filter(|p| p.role == role).collect();
        if players.is_empty() {
            return 40.0; // fallback
        }
        players.iter().map(|p| attr_fn(p) as f64).sum::<f64>() / players.len() as f64
    }

    /// Composite defense rating (from Top + Support).
    pub fn defense_rating(&self) -> f64 {
        let top_avg = self.role_attr_avg(LolRole::Top, |p| {
            ((p.consistency as u16 + p.discipline as u16 + p.mental_resilience as u16) / 3) as u8
        });
        let support_avg = self.role_attr_avg(LolRole::Support, |p| {
            ((p.macro_play as u16 + p.teamfighting as u16 + p.discipline as u16) / 3) as u8
        });
        top_avg * 0.7 + support_avg * 0.3
    }

    /// Composite attack rating (from ADC + Mid).
    pub fn attack_rating(&self) -> f64 {
        let adc_avg = self.role_attr_avg(LolRole::Adc, |p| {
            ((p.mechanics as u16 + p.laning as u16 + p.teamfighting as u16) / 3) as u8
        });
        let mid_contrib = self.role_attr_avg(LolRole::Mid, |p| {
            ((p.mechanics as u16 + p.teamfighting as u16 + p.consistency as u16) / 3) as u8
        });
        adc_avg * 0.75 + mid_contrib * 0.25
    }

    /// Support contribution rating (Vision + Teamwork).
    pub fn support_rating(&self) -> f64 {
        self.role_attr_avg(LolRole::Support, |p| p.champion_pool)
    }

    pub fn midfield_rating(&self) -> f64 {
        let mid_avg = self.role_attr_avg(LolRole::Mid, |p| {
            ((p.macro_play as u16 + p.shotcalling as u16 + p.laning as u16) / 3) as u8
        });
        let jg_avg = self.role_attr_avg(LolRole::Jungle, |p| {
            ((p.macro_play as u16 + p.shotcalling as u16 + p.mental_resilience as u16) / 3) as u8
        });
        mid_avg * 0.6 + jg_avg * 0.4
    }
}

// ---------------------------------------------------------------------------
// MatchConfig — tuneable simulation parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchConfig {
    /// Multiplier applied to the home team's ratings (e.g. 1.08 = 8% boost).
    pub home_advantage: f64,
    /// Base probability that a shot from the box is on target (0.0–1.0).
    pub shot_accuracy_base: f64,
    /// Random swing applied in objective control comparisons.
    #[serde(default = "default_objective_swing_min")]
    pub objective_swing_min: f64,
    #[serde(default = "default_objective_swing_max")]
    pub objective_swing_max: f64,
    /// Per-tick structure damage random range.
    #[serde(default = "default_structure_damage_min")]
    pub structure_damage_min: f64,
    #[serde(default = "default_structure_damage_max")]
    pub structure_damage_max: f64,
    /// Late-game combat scaling cap.
    #[serde(default = "default_late_game_damage_scale")]
    pub late_game_damage_scale: f64,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            home_advantage: 1.03,
            shot_accuracy_base: 0.45,
            objective_swing_min: default_objective_swing_min(),
            objective_swing_max: default_objective_swing_max(),
            structure_damage_min: default_structure_damage_min(),
            structure_damage_max: default_structure_damage_max(),
            late_game_damage_scale: default_late_game_damage_scale(),
        }
    }
}

fn default_objective_swing_min() -> f64 {
    0.97
}

fn default_objective_swing_max() -> f64 {
    1.06
}

fn default_structure_damage_min() -> f64 {
    9.0
}

fn default_structure_damage_max() -> f64 {
    15.0
}

fn default_late_game_damage_scale() -> f64 {
    1.50
}

// ---------------------------------------------------------------------------
// Side — which side of the match
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Home,
    Away,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::Home => Side::Away,
            Side::Away => Side::Home,
        }
    }
}

// ---------------------------------------------------------------------------
// Zone — regions of the rift from the perspective of the match (not a team)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Zone {
    HomeBox,
    HomeDefense,
    Midfield,
    AwayDefense,
    AwayBox,
}

impl Zone {
    /// The attacking zone for a given side (where they score).
    pub fn attacking_box(side: Side) -> Zone {
        match side {
            Side::Home => Zone::AwayBox,
            Side::Away => Zone::HomeBox,
        }
    }

    /// The attacking third for a given side.
    pub fn attacking_third(side: Side) -> Zone {
        match side {
            Side::Home => Zone::AwayDefense,
            Side::Away => Zone::HomeDefense,
        }
    }

    /// The defensive third for a given side.
    pub fn defensive_third(side: Side) -> Zone {
        match side {
            Side::Home => Zone::HomeDefense,
            Side::Away => Zone::AwayDefense,
        }
    }

    /// Advance the ball one zone towards the given side's goal.
    pub fn advance_towards(self, attacking_side: Side) -> Zone {
        match attacking_side {
            Side::Home => match self {
                Zone::HomeBox => Zone::HomeDefense,
                Zone::HomeDefense => Zone::Midfield,
                Zone::Midfield => Zone::AwayDefense,
                Zone::AwayDefense => Zone::AwayBox,
                Zone::AwayBox => Zone::AwayBox,
            },
            Side::Away => match self {
                Zone::AwayBox => Zone::AwayDefense,
                Zone::AwayDefense => Zone::Midfield,
                Zone::Midfield => Zone::HomeDefense,
                Zone::HomeDefense => Zone::HomeBox,
                Zone::HomeBox => Zone::HomeBox,
            },
        }
    }

    /// Is this zone the attacking box for the given side?
    pub fn is_box_for(self, attacking_side: Side) -> bool {
        self == Zone::attacking_box(attacking_side)
    }
}
