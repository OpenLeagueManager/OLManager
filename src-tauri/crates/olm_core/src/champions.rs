use crate::domain::message::{InboxMessage, MessageCategory, MessagePriority};
use crate::domain::staff::StaffRole;
use crate::game::Game;
use crate::staff_effects::LolStaffEffects;
use chrono::{Datelike, NaiveDate};
use rand::Rng;
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;
#[cfg(feature = "typescript")]
use ts_rs::TS;

fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

const MIN_MASTERY: u8 = 25;
const MASTERY_CAP: u8 = 100;
const PATCH_INTERVAL_DAYS: i64 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum SoloQTier {
    Challenger,
    Grandmaster,
    Master,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum ChampionPatchChange {
    Buff,
    Nerf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct ChampionMasteryEntry {
    pub player_id: String,
    pub champion_id: String,
    pub mastery: u8,
    pub last_active_on: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct ChampionMetaEntry {
    pub champion_id: String,
    pub role: String,
    #[serde(default = "default_meta_tier")]
    pub tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct ChampionPatchNote {
    pub champion_id: String,
    pub role: String,
    pub change: ChampionPatchChange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct ChampionPatchState {
    pub current_patch: u32,
    #[serde(default)]
    pub current_patch_label: String,
    #[serde(default)]
    pub patch_year: u16,
    #[serde(default)]
    pub patch_index_in_year: u16,
    pub last_patch_date: Option<String>,
    #[serde(default)]
    pub hidden_meta: Vec<ChampionMetaEntry>,
    #[serde(default)]
    pub patch_notes: Vec<ChampionPatchNote>,
    #[serde(default)]
    pub discovered_champion_ids: Vec<String>,
    #[serde(default)]
    pub rng_seed: u64,
}

impl Default for ChampionPatchState {
    fn default() -> Self {
        Self {
            current_patch: 0,
            current_patch_label: String::new(),
            patch_year: 0,
            patch_index_in_year: 0,
            last_patch_date: None,
            hidden_meta: Vec::new(),
            patch_notes: Vec::new(),
            discovered_champion_ids: Vec::new(),
            rng_seed: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChampionCatalogRoot {
    data: ChampionCatalogData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChampionCatalogData {
    roles: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
struct WorkingMeta {
    champion_id: String,
    role: String,
    score: i16,
}

static CHAMPION_CATALOG: OnceLock<Vec<(String, String)>> = OnceLock::new();
fn default_meta_tier() -> String {
    "C".to_string()
}

fn today_str(game: &Game) -> String {
    game.clock.current_date.format("%Y-%m-%d").to_string()
}

fn parse_day(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn normalize_key(value: &str) -> String {
    value
        .to_lowercase()
        .replace(|ch: char| !ch.is_ascii_alphanumeric(), "")
}

fn role_for_lineup_index(index: usize) -> Option<crate::domain::player::LolRole> {
    match index {
        0 => Some(crate::domain::player::LolRole::Top),
        1 => Some(crate::domain::player::LolRole::Jungle),
        2 => Some(crate::domain::player::LolRole::Mid),
        3 => Some(crate::domain::player::LolRole::Adc),
        4 => Some(crate::domain::player::LolRole::Support),
        _ => None,
    }
}

fn role_label_for_position(pos: crate::domain::player::LolRole) -> &'static str {
    match pos {
        crate::domain::player::LolRole::Top => "Top",
        crate::domain::player::LolRole::Jungle => "Jungle",
        crate::domain::player::LolRole::Mid => "Mid",
        crate::domain::player::LolRole::Adc => "ADC",
        crate::domain::player::LolRole::Support => "Support",
        crate::domain::player::LolRole::Unknown => "Unknown",
    }
}

fn current_role_for_player(
    game: &Game,
    team_id: &str,
    player_id: &str,
    natural_position: crate::domain::player::LolRole,
) -> crate::domain::player::LolRole {
    game.teams
        .iter()
        .find(|team| team.id == team_id)
        .and_then(|team| {
            team.active_lineup_ids
                .iter()
                .position(|id| id == player_id)
                .and_then(role_for_lineup_index)
        })
        .unwrap_or(natural_position)
}

fn normalize_role(value: &str) -> Option<String> {
    match normalize_key(value).as_str() {
        "top" => Some("Top".to_string()),
        "jungle" => Some("Jungle".to_string()),
        "mid" | "middle" => Some("Mid".to_string()),
        "bot" | "adc" | "bottom" => Some("ADC".to_string()),
        "support" | "utility" | "sup" => Some("Support".to_string()),
        _ => None,
    }
}

fn champion_role_key(champion_id: &str, role: &str) -> String {
    format!("{}:{}", normalize_key(champion_id), normalize_key(role))
}

fn hash_text(value: &str) -> u32 {
    let mut hash = 0u32;
    for ch in value.chars() {
        hash = hash.wrapping_mul(31).wrapping_add(ch as u32);
    }
    hash
}

fn pseudo_random(seed: &str) -> f64 {
    f64::from(hash_text(seed) % 10_000) / 10_000.0
}

fn days_between(start: chrono::DateTime<chrono::Utc>, end: chrono::DateTime<chrono::Utc>) -> i64 {
    ((end - start).num_days()).max(0)
}

fn mastery_signal_for_player(game: &Game, player_id: &str) -> f64 {
    let mut values: Vec<u8> = game
        .champion_masteries
        .iter()
        .filter(|entry| entry.player_id == player_id)
        .map(|entry| entry.mastery)
        .collect();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by_key(|value| std::cmp::Reverse(*value));
    let top = values.into_iter().take(3).collect::<Vec<_>>();
    let avg = top.iter().map(|value| f64::from(*value)).sum::<f64>() / top.len() as f64;
    (avg - 60.0).max(0.0)
}

const SOLOQ_POINTS_BASELINE: f64 = 3000.0;
const SOLOQ_POINTS_MAX: f64 = 7000.0;
const SOLOQ_GRANDMASTER_LP_CUTOFF: f64 = 800.0;
const SOLOQ_CHALLENGER_LP_CUTOFF: f64 = 1300.0;

/// Maximum LP-points swing the training regime can add or subtract on top of the
/// skill baseline — roughly one tier of movement, so smart grinding lifts a
/// player a tier and neglect can cost one, but skill still sets the center.
const SOLOQ_TRAINING_SWING: f64 = 600.0;
/// Days for the training modifier to ramp to full effect: a fresh save starts at
/// the pure skill baseline and the grind compounds over ~2 months.
const SOLOQ_TRAINING_RAMP_DAYS: f64 = 60.0;
/// How far below the skill baseline neglect can drag a player (about half a tier).
const SOLOQ_SKILL_FLOOR_DROP: f64 = 350.0;

fn soloq_focus_multiplier(focus: Option<crate::domain::team::TrainingFocus>) -> f64 {
    use crate::domain::team::TrainingFocus;
    match focus {
        Some(TrainingFocus::ChampionPoolPractice) => 1.25,
        Some(TrainingFocus::IndividualCoaching) => 1.0,
        Some(TrainingFocus::Scrims) => 0.85,
        Some(TrainingFocus::MacroSystems) => 0.75,
        Some(TrainingFocus::VODReview) | Some(TrainingFocus::MentalResetRecovery) => 0.7,
        None => 0.85,
    }
}

fn soloq_intensity_multiplier(intensity: crate::domain::team::TrainingIntensity) -> f64 {
    use crate::domain::team::TrainingIntensity;
    match intensity {
        TrainingIntensity::High => 1.25,
        TrainingIntensity::Medium => 1.0,
        TrainingIntensity::Low => 0.75,
    }
}

fn soloq_schedule_multiplier(schedule: crate::domain::team::TrainingSchedule) -> f64 {
    use crate::domain::team::TrainingSchedule;
    match schedule {
        TrainingSchedule::Intense => 1.3,
        TrainingSchedule::Balanced => 1.0,
        TrainingSchedule::Light => 0.6,
    }
}

/// Resolve the training regime for a player and collapse it into a single grind
/// quality factor centered on 1.0 (Balanced / Medium / IndividualCoaching). The
/// player's personal focus overrides the team default; intensity and schedule
/// come from the team. Mirrors the inputs the Meta/Training tabs surface.
fn soloq_grind_quality(game: &Game, player: &crate::domain::player::Player) -> f64 {
    let team = player
        .team_id
        .as_deref()
        .and_then(|team_id| game.teams.iter().find(|team| team.id == team_id));

    let focus = player
        .training_focus
        .clone()
        .or_else(|| team.map(|team| team.training_focus.clone()));
    let intensity = team
        .map(|team| team.training_intensity.clone())
        .unwrap_or_default();
    let schedule = team
        .map(|team| team.training_schedule.clone())
        .unwrap_or_default();

    soloq_focus_multiplier(focus)
        * soloq_intensity_multiplier(intensity)
        * soloq_schedule_multiplier(schedule)
}

/// Hybrid SoloQ points at a given day index: a skill baseline (OVR + champion
/// mastery) fixes the expected tier, a bounded training modifier ramps in over
/// time, and a small daily drift keeps the number alive. Clamped so neglect
/// can't sink a player far below their skill and the grind can't run away.
fn soloq_points_at(game: &Game, player: &crate::domain::player::Player, day_index: i64) -> f64 {
    let ovr = f64::from(crate::potential::calculate_lol_ovr(player));
    let mastery_signal = mastery_signal_for_player(game, &player.id);
    let id_jitter = f64::from(hash_text(&player.id) % 121) - 60.0;

    let skill_base = 3520.0 + (ovr - 76.0) * 52.0 + mastery_signal * 4.0 + id_jitter;
    let skill_floor = skill_base - SOLOQ_SKILL_FLOOR_DROP;

    let grind_quality = soloq_grind_quality(game, player);
    let ramp = (day_index.max(0) as f64 / SOLOQ_TRAINING_RAMP_DAYS).min(1.0);
    let training_mod = (grind_quality - 1.0) * SOLOQ_TRAINING_SWING * ramp;

    let drift = (pseudo_random(&format!("{}:{}", player.id, day_index)) * 90.0 - 45.0).round();

    (skill_base + training_mod + drift).clamp(skill_floor, SOLOQ_POINTS_MAX)
}

fn soloq_points_for_player(game: &Game, player: &crate::domain::player::Player) -> f64 {
    let day_index = days_between(game.clock.start_date, game.clock.current_date);
    soloq_points_at(game, player, day_index)
}

fn soloq_lp_for_player(game: &Game, player: &crate::domain::player::Player) -> f64 {
    (soloq_points_for_player(game, player) - SOLOQ_POINTS_BASELINE).max(0.0)
}

fn soloq_tier_from_lp(lp: f64) -> SoloQTier {
    if lp >= SOLOQ_CHALLENGER_LP_CUTOFF {
        SoloQTier::Challenger
    } else if lp >= SOLOQ_GRANDMASTER_LP_CUTOFF {
        SoloQTier::Grandmaster
    } else {
        SoloQTier::Master
    }
}

pub fn soloq_tier_for_player(game: &Game, player: &crate::domain::player::Player) -> SoloQTier {
    soloq_tier_from_lp(soloq_lp_for_player(game, player))
}

fn soloq_tier_label(tier: SoloQTier) -> &'static str {
    match tier {
        SoloQTier::Challenger => "Challenger",
        SoloQTier::Grandmaster => "Grandmaster",
        SoloQTier::Master => "Master",
    }
}

fn soloq_multiplier_for_tier(tier: SoloQTier) -> f64 {
    match tier {
        SoloQTier::Challenger => 1.2,
        SoloQTier::Grandmaster => 1.0,
        SoloQTier::Master => 0.8,
    }
}

/// SoloQ standing for one player, ready to serialize to the UI. This is the
/// single source of truth: the Meta and Training tabs render this instead of
/// recomputing the formula (which previously diverged from the simulation).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct SoloQStatus {
    pub player_id: String,
    pub tier: String,
    pub lp: u32,
    pub delta: i32,
    pub multiplier: f64,
}

pub fn soloq_status_for_player(game: &Game, player: &crate::domain::player::Player) -> SoloQStatus {
    let day_index = days_between(game.clock.start_date, game.clock.current_date);
    let lp = (soloq_points_at(game, player, day_index) - SOLOQ_POINTS_BASELINE).max(0.0);
    let tier = soloq_tier_from_lp(lp);

    let delta = if day_index > 0 {
        let lp_yesterday =
            (soloq_points_at(game, player, day_index - 1) - SOLOQ_POINTS_BASELINE).max(0.0);
        (lp - lp_yesterday).round() as i32
    } else {
        0
    };

    SoloQStatus {
        player_id: player.id.clone(),
        tier: soloq_tier_label(tier).to_string(),
        lp: lp.round() as u32,
        delta,
        multiplier: soloq_multiplier_for_tier(tier),
    }
}

pub fn mastery_gain_multiplier_for_player(game: &Game, player_id: &str) -> f64 {
    let Some(player) = game
        .players
        .iter()
        .find(|candidate| candidate.id == player_id)
    else {
        return 1.0;
    };
    soloq_multiplier_for_tier(soloq_tier_for_player(game, player))
}

fn two_digit_year(game: &Game) -> u16 {
    (game.clock.current_date.year().rem_euclid(100)) as u16
}

fn format_patch_label(year: u16, patch_index: u16) -> String {
    format!("{}.{}", year, patch_index)
}

fn tier_to_score(tier: &str) -> i16 {
    match tier {
        "S" => 82,
        "A" => 70,
        "B" => 58,
        "C" => 46,
        "D" => 34,
        _ => 46,
    }
}

fn tier_from_rank(rank_index: usize, total: usize) -> &'static str {
    if total == 0 {
        return "C";
    }
    let pct = (rank_index + 1) as f64 / total as f64;
    if pct <= 0.12 {
        "S"
    } else if pct <= 0.34 {
        "A"
    } else if pct <= 0.64 {
        "B"
    } else if pct <= 0.86 {
        "C"
    } else {
        "D"
    }
}

fn tier_map_from_working(working: &[WorkingMeta]) -> HashMap<String, String> {
    let mut sorted: Vec<usize> = (0..working.len()).collect();
    sorted.sort_by_key(|index| std::cmp::Reverse(working[*index].score));

    let mut tier_map = HashMap::new();
    for (rank, index) in sorted.iter().enumerate() {
        tier_map.insert(
            champion_role_key(&working[*index].champion_id, &working[*index].role),
            tier_from_rank(rank, sorted.len()).to_string(),
        );
    }
    tier_map
}

fn champion_catalog() -> &'static Vec<(String, String)> {
    CHAMPION_CATALOG.get_or_init(|| {
        let raw = include_str!("../../../../assets/simulation/champions.json");
        let parsed: ChampionCatalogRoot =
            serde_json::from_str(raw).unwrap_or(ChampionCatalogRoot {
                data: ChampionCatalogData {
                    roles: HashMap::new(),
                },
            });
        let mut entries: Vec<(String, String)> = Vec::new();
        for (champion_id, roles) in parsed.data.roles {
            let mut normalized_roles: Vec<String> = roles
                .iter()
                .filter_map(|role| normalize_role(role))
                .collect();

            normalized_roles.sort();
            normalized_roles.dedup();

            if normalized_roles.is_empty() {
                entries.push((champion_id.clone(), "Mid".to_string()));
            } else {
                for role in normalized_roles {
                    entries.push((champion_id.clone(), role));
                }
            }
        }

        entries
    })
}

fn base_role_score(role: &str) -> i16 {
    match role {
        "Top" => 56,
        "Jungle" => 57,
        "Mid" => 58,
        "ADC" => 56,
        "Support" => 55,
        _ => 54,
    }
}

fn upsert_mastery(game: &mut Game, player_id: &str, champion_id: &str, value: u8) {
    let today = today_str(game);
    if let Some(existing) = game.champion_masteries.iter_mut().find(|entry| {
        entry.player_id == player_id
            && normalize_key(&entry.champion_id) == normalize_key(champion_id)
    }) {
        existing.mastery = value.clamp(MIN_MASTERY, MASTERY_CAP);
        existing.last_active_on = today;
        return;
    }

    game.champion_masteries.push(ChampionMasteryEntry {
        player_id: player_id.to_string(),
        champion_id: champion_id.to_string(),
        mastery: value.clamp(MIN_MASTERY, MASTERY_CAP),
        last_active_on: today,
    });
}

pub fn bootstrap_seed_masteries(game: &mut Game) {
    // Preserve existing masteries from saves (Flow C).
    if !game.champion_masteries.is_empty() {
        return;
    }

    let catalog = champion_catalog();

    // Group champion IDs by role from the static catalog.
    // The catalog returns (champion_id, role) pairs, one per role per champ.
    let mut by_role: HashMap<&str, Vec<&str>> = HashMap::new();
    for (champion_id, role) in catalog.iter() {
        by_role
            .entry(role.as_str())
            .or_default()
            .push(champion_id.as_str());
    }

    // Collect (player_id, role_str) upfront to avoid borrowing conflicts.
    let player_roles: Vec<(String, String)> = game
        .players
        .iter()
        .filter_map(|p| {
            let role = match p.natural_position {
                crate::domain::player::LolRole::Top => "Top",
                crate::domain::player::LolRole::Jungle => "Jungle",
                crate::domain::player::LolRole::Mid => "Mid",
                crate::domain::player::LolRole::Adc => "ADC",
                crate::domain::player::LolRole::Support => "Support",
                crate::domain::player::LolRole::Unknown => return None,
            };
            Some((p.id.clone(), role.to_string()))
        })
        .collect();

    for (player_id, role_str) in &player_roles {
        let Some(champions) = by_role.get(role_str.as_str()) else {
            continue;
        };
        if champions.is_empty() {
            continue;
        }

        // Deterministic count: 4-6 champions per player.
        let count_seed = format!("{}_{}", player_id, "seed_count");
        let count =
            4 + (hash_text(&count_seed) as usize % 3.min(champions.len().saturating_sub(4)));
        let count = count.min(champions.len());

        // Sort champions deterministically using a hash seeded by player_id.
        let mut sorted: Vec<&&str> = champions.iter().collect();
        sorted.sort_by(|a, b| {
            let ha = hash_text(&format!("{}_{}", player_id, a));
            let hb = hash_text(&format!("{}_{}", player_id, b));
            ha.cmp(&hb)
        });

        for champion_id in sorted.iter().take(count) {
            let mastery_seed = format!("{}_{}_mastery", player_id, champion_id);
            let mastery = 40 + (hash_text(&mastery_seed) as u8 % 41); // 40..80
            upsert_mastery(game, player_id, champion_id, mastery);
        }
    }
}

pub(crate) fn ensure_patch_seed(state: &mut ChampionPatchState) {
    if state.rng_seed == 0 {
        state.rng_seed = rand::rng().random();
    }
}

/// Derive a deterministic but time-varying seed from the persistent base seed
/// and a salt that changes over time (patch number, date, week key).
///
/// The persistent `rng_seed` is fixed for the lifetime of a save, so reseeding
/// an RNG straight from it makes every patch roll, every daily discovery and
/// every weekly mastery tick replay the identical sequence forever. Mixing in a
/// time-varying salt keeps the result reproducible per save while letting the
/// outcome actually move between patches/days/weeks.
pub(crate) fn derived_seed(base: u64, salt: &str) -> u64 {
    // FNV-1a 64-bit hash of the salt, XOR-folded into the base seed.
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in salt.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    base ^ hash
}

fn ensure_multirole_meta_shape(game: &mut Game) {
    if game.champion_patch.hidden_meta.is_empty() {
        return;
    }

    let mut existing_role_keys: HashSet<String> = game
        .champion_patch
        .hidden_meta
        .iter()
        .map(|entry| champion_role_key(&entry.champion_id, &entry.role))
        .collect();

    let mut tier_by_champion: HashMap<String, String> = HashMap::new();
    game.champion_patch.hidden_meta.iter().for_each(|entry| {
        let champion_key = normalize_key(&entry.champion_id);
        let tier = (entry.tier.clone()).to_uppercase();
        let previous = tier_by_champion.get(&champion_key).cloned();
        if let Some(prev) = previous {
            let prev_weight = tier_to_score(&prev);
            let current_weight = tier_to_score(&tier);
            if current_weight > prev_weight {
                tier_by_champion.insert(champion_key, tier);
            }
        } else {
            tier_by_champion.insert(champion_key, tier);
        }
    });

    let mut migrated_entries: Vec<ChampionMetaEntry> = Vec::new();
    for (champion_id, role) in champion_catalog().iter() {
        let role_key = champion_role_key(champion_id, role);
        if existing_role_keys.contains(&role_key) {
            continue;
        }

        let champion_key = normalize_key(champion_id);
        let Some(tier) = tier_by_champion.get(&champion_key) else {
            continue;
        };

        migrated_entries.push(ChampionMetaEntry {
            champion_id: champion_id.clone(),
            role: role.clone(),
            tier: tier.clone(),
        });
        existing_role_keys.insert(role_key);
    }

    if !migrated_entries.is_empty() {
        game.champion_patch.hidden_meta.extend(migrated_entries);
    }
}

fn ensure_initial_patch_state(game: &mut Game) {
    if !game.champion_patch.hidden_meta.is_empty() {
        ensure_multirole_meta_shape(game);
        if game.champion_patch.current_patch_label.is_empty() {
            let year = if game.champion_patch.patch_year == 0 {
                two_digit_year(game)
            } else {
                game.champion_patch.patch_year
            };
            let idx = if game.champion_patch.patch_index_in_year == 0 {
                1
            } else {
                game.champion_patch.patch_index_in_year
            };
            game.champion_patch.current_patch_label = format_patch_label(year, idx);
        }
        return;
    }

    ensure_patch_seed(&mut game.champion_patch);
    let mut rng = StdRng::seed_from_u64(game.champion_patch.rng_seed);

    let initial_working: Vec<WorkingMeta> = champion_catalog()
        .iter()
        .map(|(champion_id, role)| {
            let score = (base_role_score(role) + rng.random_range(-4..=4)).clamp(32, 82);
            WorkingMeta {
                champion_id: champion_id.clone(),
                role: role.clone(),
                score,
            }
        })
        .collect();

    let initial_tier_map = tier_map_from_working(&initial_working);
    let initial_meta: Vec<ChampionMetaEntry> = initial_working
        .iter()
        .map(|entry| ChampionMetaEntry {
            champion_id: entry.champion_id.clone(),
            role: entry.role.clone(),
            tier: initial_tier_map
                .get(&champion_role_key(&entry.champion_id, &entry.role))
                .cloned()
                .unwrap_or_else(|| "C".to_string()),
        })
        .collect();

    let fallback_year = two_digit_year(game);
    let year = if game.champion_patch.patch_year == 0 {
        fallback_year
    } else {
        game.champion_patch.patch_year
    };
    let patch_index = if game.champion_patch.patch_index_in_year == 0 {
        1
    } else {
        game.champion_patch.patch_index_in_year
    };
    game.champion_patch.current_patch = game.champion_patch.current_patch.max(1);
    game.champion_patch.patch_year = year;
    game.champion_patch.patch_index_in_year = patch_index;
    game.champion_patch.current_patch_label = format_patch_label(year, patch_index);
    game.champion_patch.last_patch_date = Some(today_str(game));
    game.champion_patch.hidden_meta = initial_meta;
    game.champion_patch.patch_notes.clear();
}

fn seed_initial_discovery(game: &mut Game) {
    if !game.champion_patch.discovered_champion_ids.is_empty() {
        return;
    }

    let mut base_reveals = 15usize;

    if let Some(manager_team_id) = game.manager.team_id.as_deref() {
        let scouts: Vec<_> = game
            .staff
            .iter()
            .filter(|staff| {
                staff.team_id.as_deref() == Some(manager_team_id) && staff.role == StaffRole::Scout
            })
            .collect();

        if !scouts.is_empty() {
            let avg_scouting = scouts
                .iter()
                .map(|s| s.attributes.judging_ability as f64)
                .sum::<f64>()
                / scouts.len() as f64;
            let avg_potential = scouts
                .iter()
                .map(|s| s.attributes.judging_potential as f64)
                .sum::<f64>()
                / scouts.len() as f64;

            base_reveals += scouts.len() * 3;
            base_reveals += (avg_scouting / 15.0).floor() as usize;
            base_reveals += (avg_potential / 30.0).floor() as usize;
        }
    }

    let mut all_keys: Vec<String> = game
        .champion_patch
        .hidden_meta
        .iter()
        .map(|entry| normalize_key(&entry.champion_id))
        .collect();
    all_keys.sort();
    all_keys.dedup();

    if all_keys.is_empty() {
        return;
    }

    let seed_salt = format!("initial_discovery:{}", game.champion_patch.rng_seed);
    let mut rng = StdRng::seed_from_u64(derived_seed(game.champion_patch.rng_seed, &seed_salt));

    let reveal_count = base_reveals.min(all_keys.len());
    let mut indices: Vec<usize> = (0..all_keys.len()).collect();
    for _ in 0..reveal_count {
        if indices.is_empty() {
            break;
        }
        let pick = rng.random_range(0..indices.len());
        let chosen_idx = indices.swap_remove(pick);
        game.champion_patch
            .discovered_champion_ids
            .push(all_keys[chosen_idx].clone());
    }
}

pub fn bootstrap_champion_state(game: &mut Game) {
    bootstrap_seed_masteries(game);
    ensure_initial_patch_state(game);
    seed_initial_discovery(game);

    // Populate soloq_lp for all players so the frontend reads instant data
    // even on initial game load (not only after the first day advance).
    let day_index = days_between(game.clock.start_date, game.clock.current_date);
    let player_ids: Vec<String> = game.players.iter().map(|p| p.id.clone()).collect();
    for player_id in &player_ids {
        let idx = game
            .players
            .iter()
            .position(|p| &p.id == player_id)
            .unwrap();
        let lp = {
            let player = &game.players[idx];
            (soloq_points_at(game, player, day_index) - SOLOQ_POINTS_BASELINE).max(0.0)
        };
        game.players[idx].soloq_lp = lp;
    }
}

pub fn set_player_training_target(
    game: &mut Game,
    player_id: &str,
    priority_index: usize,
    champion_id: Option<String>,
) -> Result<(), String> {
    let player = game
        .players
        .iter_mut()
        .find(|candidate| candidate.id == player_id)
        .ok_or_else(|| format!("Player not found: {}", player_id))?;

    if priority_index >= 3 {
        return Err("priority_index must be between 0 and 2".to_string());
    }

    if player.champion_training_targets.len() < 3 {
        player.champion_training_targets.resize(3, String::new());
    }

    if let Some(champion) = champion_id.clone() {
        let normalized = normalize_key(&champion);
        // Remove duplicates from other slots before setting the requested priority.
        for (index, slot) in player.champion_training_targets.iter_mut().enumerate() {
            if index != priority_index && normalize_key(slot) == normalized {
                *slot = String::new();
            }
        }
        player.champion_training_targets[priority_index] = champion;
    } else {
        player.champion_training_targets[priority_index] = String::new();
    }

    // Compact empty slots while preserving priority order among non-empty slots.
    let mut compacted: Vec<String> = player
        .champion_training_targets
        .iter()
        .filter(|slot| !slot.trim().is_empty())
        .cloned()
        .collect();
    compacted.truncate(3);
    player.champion_training_targets = compacted;
    player.champion_training_targets.resize(3, String::new());

    if let Some(champion) = champion_id {
        let current = mastery_for_player_champion(game, player_id, &champion);
        upsert_mastery(game, player_id, &champion, current.max(MIN_MASTERY));
    }
    Ok(())
}

pub fn training_targets_for_player(player: &crate::domain::player::Player) -> Vec<String> {
    let mut targets: Vec<String> = player
        .champion_training_targets
        .iter()
        .filter(|slot| !slot.trim().is_empty())
        .cloned()
        .collect();

    targets.truncate(3);
    targets
}

pub fn ensure_training_targets_from_mastery(game: &mut Game, player_id: &str) {
    let has_existing_targets = game
        .players
        .iter()
        .find(|candidate| candidate.id == player_id)
        .is_some_and(|player| !training_targets_for_player(player).is_empty());
    if has_existing_targets {
        return;
    }

    let role = game
        .players
        .iter()
        .find(|candidate| candidate.id == player_id)
        .map(|player| match player.natural_position {
            crate::domain::player::LolRole::Top => "Top",
            crate::domain::player::LolRole::Jungle => "Jungle",
            crate::domain::player::LolRole::Mid => "Mid",
            crate::domain::player::LolRole::Adc => "ADC",
            crate::domain::player::LolRole::Support => "Support",
            crate::domain::player::LolRole::Unknown => "Unknown",
        })
        .unwrap_or("Unknown");

    let discovered: HashSet<String> = game
        .champion_patch
        .discovered_champion_ids
        .iter()
        .map(|id| normalize_key(id))
        .collect();

    let tier_score = |tier: &str| -> i32 {
        match tier.to_uppercase().as_str() {
            "S" => 100,
            "A" => 85,
            "B" => 70,
            "C" => 55,
            "D" => 40,
            _ => 60,
        }
    };

    let mastery_map: HashMap<String, u8> = game
        .champion_masteries
        .iter()
        .filter(|entry| entry.player_id == player_id)
        .map(|entry| (normalize_key(&entry.champion_id), entry.mastery))
        .collect();

    let mut by_meta: Vec<(String, i32)> = game
        .champion_patch
        .hidden_meta
        .iter()
        .filter(|meta| normalize_key(&meta.role) == normalize_key(role))
        .filter(|meta| {
            let key = normalize_key(&meta.champion_id);
            discovered.is_empty() || discovered.contains(&key)
        })
        .map(|meta| {
            let key = normalize_key(&meta.champion_id);
            let mastery = i32::from(*mastery_map.get(&key).unwrap_or(&MIN_MASTERY));
            let mastery_gap = i32::from(MASTERY_CAP) - mastery;
            let role_fit = if normalize_key(&meta.role) == normalize_key(role) {
                10
            } else {
                0
            };
            let score = tier_score(&meta.tier) * 2 + role_fit + mastery_gap;
            (meta.champion_id.clone(), score)
        })
        .collect();
    by_meta.sort_by(|left, right| right.1.cmp(&left.1));

    let mut selected: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (champion_id, _) in by_meta {
        let key = normalize_key(&champion_id);
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);
        selected.push(champion_id);
        if selected.len() >= 3 {
            break;
        }
    }

    if selected.len() < 3 {
        let mut ranked_masteries: Vec<(String, u8)> = game
            .champion_masteries
            .iter()
            .filter(|entry| entry.player_id == player_id)
            .map(|entry| (entry.champion_id.clone(), entry.mastery))
            .collect();
        ranked_masteries.sort_by(|left, right| right.1.cmp(&left.1));
        for (champion_id, _) in ranked_masteries {
            let key = normalize_key(&champion_id);
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);
            selected.push(champion_id);
            if selected.len() >= 3 {
                break;
            }
        }
    }

    if selected.is_empty() {
        return;
    }

    if let Some(player) = game
        .players
        .iter_mut()
        .find(|candidate| candidate.id == player_id)
    {
        player.champion_training_targets.clear();
        player.champion_training_targets.extend(selected);
        player.champion_training_targets.truncate(3);
        player.champion_training_targets.resize(3, String::new());
    }
}

pub fn delegate_champion_training_to_coach(game: &mut Game) -> Result<usize, String> {
    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or("No team assigned to manager".to_string())?;

    let discovered: HashSet<String> = game
        .champion_patch
        .discovered_champion_ids
        .iter()
        .map(|id| normalize_key(id))
        .collect();

    let tier_weight = |tier: &str| -> i32 {
        match tier.to_uppercase().as_str() {
            "S" => 0,
            "A" => 1,
            "B" => 2,
            "C" => 3,
            "D" => 4,
            _ => 99,
        }
    };

    // Collect all meta entries upfront
    let meta_entries: Vec<ChampionMetaEntry> = game.champion_patch.hidden_meta.clone();

    // Collect mastery data upfront and build lookup map
    let mastery_map: HashMap<String, u8> = game
        .champion_masteries
        .iter()
        .map(|e| {
            (
                format!("{}:{}", e.player_id, normalize_key(&e.champion_id)),
                e.mastery,
            )
        })
        .collect();

    let get_mastery = |player_id: &str, champ_id: &str| -> u8 {
        *mastery_map
            .get(&format!("{}:{}", player_id, normalize_key(champ_id)))
            .unwrap_or(&MIN_MASTERY)
    };

    let player_ids: Vec<String> = game
        .players
        .iter()
        .filter(|p| p.team_id == Some(manager_team_id.clone()))
        .map(|p| p.id.clone())
        .collect();

    let mut results: Vec<(String, Vec<String>)> = Vec::new();

    for player_id in player_ids {
        let player = game.players.iter().find(|p| p.id == player_id).unwrap();
        let role = role_label_for_position(current_role_for_player(
            game,
            &manager_team_id,
            &player_id,
            player.natural_position,
        ));

        let role_meta: Vec<&ChampionMetaEntry> = meta_entries
            .iter()
            .filter(|entry| {
                normalize_key(&entry.role) == normalize_key(&role)
                    && discovered.contains(&normalize_key(&entry.champion_id))
                    && tier_weight(&entry.tier) <= 1
            })
            .collect();

        let mut sorted_meta = role_meta.clone();
        sorted_meta.sort_by(|a, b| {
            let tier_cmp = tier_weight(&a.tier).cmp(&tier_weight(&b.tier));
            if tier_cmp != std::cmp::Ordering::Equal {
                return tier_cmp;
            }
            get_mastery(&player_id, &a.champion_id).cmp(&get_mastery(&player_id, &b.champion_id))
        });

        let mut picks: Vec<String> = Vec::new();
        for entry in sorted_meta {
            if picks.len() >= 3 {
                break;
            }
            let normalized = normalize_key(&entry.champion_id);
            let mastery = get_mastery(&player_id, &entry.champion_id);
            if mastery >= MASTERY_CAP {
                continue;
            }
            if picks.iter().any(|p| normalize_key(p) == normalized) {
                continue;
            }
            picks.push(entry.champion_id.clone());
        }

        if picks.len() < 3 {
            let mut all_role_masteries: Vec<(String, u8)> = meta_entries
                .iter()
                .filter(|meta| {
                    let champ_key = normalize_key(&meta.champion_id);
                    normalize_key(&meta.role) == normalize_key(&role)
                        && discovered.contains(&champ_key)
                        && get_mastery(&player_id, &meta.champion_id) < MASTERY_CAP
                })
                .map(|meta| {
                    (
                        meta.champion_id.clone(),
                        get_mastery(&player_id, &meta.champion_id),
                    )
                })
                .collect();

            all_role_masteries.sort_by_key(|(_, m)| *m);

            for (champ_id, mastery) in all_role_masteries {
                if picks.len() >= 3 {
                    break;
                }
                if mastery >= MASTERY_CAP {
                    continue;
                }
                let normalized = normalize_key(&champ_id);
                if picks.iter().any(|p| normalize_key(p) == normalized) {
                    continue;
                }
                picks.push(champ_id);
            }
        }

        picks.resize(3, String::new());
        results.push((player_id, picks));
    }

    let mut updated_count = 0;
    for (player_id, targets) in &results {
        let player = game
            .players
            .iter_mut()
            .find(|p| p.id == *player_id)
            .unwrap();
        let old_targets = player.champion_training_targets.clone();
        player.champion_training_targets = targets.clone();
        player.champion_training_targets.resize(3, String::new());

        if old_targets != player.champion_training_targets {
            updated_count += 1;
        }
    }

    for (player_id, targets) in &results {
        for champion in targets {
            if !champion.trim().is_empty() {
                let current = mastery_for_player_champion(game, player_id, champion);
                upsert_mastery(game, player_id, champion, current.max(MIN_MASTERY));
            }
        }
    }

    Ok(updated_count)
}

pub fn mastery_for_player_champion(game: &Game, player_id: &str, champion_id: &str) -> u8 {
    game.champion_masteries
        .iter()
        .find(|entry| {
            entry.player_id == player_id
                && normalize_key(&entry.champion_id) == normalize_key(champion_id)
        })
        .map(|entry| entry.mastery)
        .unwrap_or(MIN_MASTERY)
}

pub fn apply_training_mastery_progress(
    game: &mut Game,
    player_id: &str,
    champion_id: &str,
    gain_factor: f64,
    rng: &mut impl Rng,
) {
    let current = mastery_for_player_champion(game, player_id, champion_id);
    let Some(player) = game
        .players
        .iter()
        .find(|candidate| candidate.id == player_id)
    else {
        return;
    };

    let mechanics = f64::from(player.attributes.mechanics.min(100)) / 100.0;
    let champion_pool = f64::from(player.attributes.champion_pool.min(100)) / 100.0;
    let stat_push = (mechanics * 0.6) + (champion_pool * 0.6);

    let headroom = f64::from(MASTERY_CAP.saturating_sub(current)) / 75.0;
    let chance = (0.16 + gain_factor * 0.26 + headroom * 0.2 + stat_push * 0.18).clamp(0.14, 0.88);
    let roll: f64 = rng.random_range(0.0..1.0);
    if roll > chance {
        return;
    }

    let stat_bonus = if mechanics >= 0.85 && champion_pool >= 0.85 {
        2
    } else if mechanics >= 0.72 || champion_pool >= 0.72 {
        1
    } else {
        0
    };

    let gain = if current >= 90 {
        2 + stat_bonus
    } else if current >= 75 {
        3 + stat_bonus
    } else {
        4 + stat_bonus
    };
    let next = current.saturating_add(gain).min(MASTERY_CAP);
    upsert_mastery(game, player_id, champion_id, next);
}

pub fn apply_scrim_mastery_progress(
    game: &mut Game,
    player_id: &str,
    champion_id: &str,
    quality: u8,
    won: bool,
    decision: Option<&crate::domain::team::PostScrimDecision>,
) {
    let current = mastery_for_player_champion(game, player_id, champion_id);
    if !game.players.iter().any(|player| player.id == player_id) {
        return;
    }

    let mut gain = if quality >= 82 {
        2
    } else if quality >= 55 {
        1
    } else {
        0
    };

    if won && quality >= 70 {
        gain += 1;
    }

    match decision {
        Some(crate::domain::team::PostScrimDecision::TargetedDrills) => gain += 1,
        Some(crate::domain::team::PostScrimDecision::VodReview) if quality >= 65 => gain += 1,
        Some(crate::domain::team::PostScrimDecision::PushThrough) if quality >= 75 => gain += 1,
        Some(crate::domain::team::PostScrimDecision::MentalReset) | None | Some(_) => {}
    }

    if gain == 0 {
        return;
    }

    let capped_gain = if current >= 90 { 1 } else { gain.min(3) };
    upsert_mastery(
        game,
        player_id,
        champion_id,
        current.saturating_add(capped_gain).min(MASTERY_CAP),
    );
}

pub fn apply_match_mastery_progress(
    game: &mut Game,
    winner_team_id: &str,
    picks: &[(String, String)],
) {
    for (player_id, champion_id) in picks {
        let current = mastery_for_player_champion(game, player_id, champion_id);
        let mut raw_gain = if current >= 90 {
            1.0
        } else if current >= 70 {
            2.0
        } else {
            3.0
        };

        let player_team_id = game
            .players
            .iter()
            .find(|player| player.id == *player_id)
            .and_then(|player| player.team_id.as_deref())
            .map(str::to_string);

        if player_team_id.as_deref() == Some(winner_team_id) {
            raw_gain += 1.0;
        }

        let staff_mult = player_team_id
            .as_deref()
            .map(|team_id| {
                LolStaffEffects::for_team(&game.staff, team_id).match_mastery_multiplier()
            })
            .unwrap_or(1.0);
        let gain = (raw_gain * staff_mult).round().clamp(1.0, 5.0) as u8;

        upsert_mastery(
            game,
            player_id,
            champion_id,
            current.saturating_add(gain).min(MASTERY_CAP),
        );
    }
}

fn apply_mastery_decay(game: &mut Game) {
    let today = game.clock.current_date.date_naive();
    for entry in &mut game.champion_masteries {
        if entry.mastery <= MIN_MASTERY {
            continue;
        }

        let Some(last_active) = parse_day(&entry.last_active_on) else {
            continue;
        };

        let inactive_days = (today - last_active).num_days();
        if inactive_days >= 56 && inactive_days % 28 == 0 {
            entry.mastery = entry.mastery.saturating_sub(1).max(MIN_MASTERY);
        }
    }
}

fn should_roll_patch(game: &Game, state: &ChampionPatchState) -> bool {
    if game.clock.current_date.weekday().num_days_from_monday() != 1 {
        return false;
    }

    let today = game.clock.current_date.date_naive();
    let Some(last_patch) = state.last_patch_date.as_deref().and_then(parse_day) else {
        return true;
    };

    (today - last_patch).num_days() >= PATCH_INTERVAL_DAYS
}

fn pick_unique_indices(rng: &mut impl Rng, candidates: &[usize], count: usize) -> Vec<usize> {
    let mut pool = candidates.to_vec();
    let mut picked = Vec::new();
    let target = count.min(pool.len());

    while picked.len() < target {
        let idx = rng.random_range(0..pool.len());
        picked.push(pool.swap_remove(idx));
    }

    picked
}

fn apply_patch(game: &mut Game) {
    ensure_patch_seed(&mut game.champion_patch);
    // Salt with the (pre-increment) patch number + roll date so each fortnight's
    // drift and buff/nerf picks differ instead of replaying the same sequence.
    let patch_salt = format!(
        "patch:{}:{}",
        game.champion_patch.current_patch,
        today_str(game)
    );
    let mut rng = StdRng::seed_from_u64(derived_seed(game.champion_patch.rng_seed, &patch_salt));
    let catalog = champion_catalog();
    if catalog.is_empty() {
        return;
    }

    let previous_tier_by_role: HashMap<String, String> = game
        .champion_patch
        .hidden_meta
        .iter()
        .map(|entry| {
            (
                champion_role_key(&entry.champion_id, &entry.role),
                entry.tier.clone(),
            )
        })
        .collect();

    let mut working: Vec<WorkingMeta> = catalog
        .iter()
        .map(|(champion_id, role)| {
            let previous_score = previous_tier_by_role
                .get(&champion_role_key(champion_id, role))
                .map(|tier| tier_to_score(tier))
                .unwrap_or(base_role_score(role));
            let mean = base_role_score(role);
            let drift: i16 = rng.random_range(-5..=5);
            let reversion = (mean - previous_score) / 4;
            let score = (previous_score + drift + reversion).clamp(30, 84);
            WorkingMeta {
                champion_id: champion_id.clone(),
                role: role.clone(),
                score,
            }
        })
        .collect();

    let mut sorted: Vec<usize> = (0..working.len()).collect();
    sorted.sort_by_key(|index| std::cmp::Reverse(working[*index].score));

    let top_window = sorted.len().max(8) / 4;
    let low_window = sorted.len().max(10) / 3;

    let nerf_pool: Vec<usize> = sorted.iter().take(top_window.max(1)).copied().collect();
    let buff_pool: Vec<usize> = sorted
        .iter()
        .rev()
        .take(low_window.max(1))
        .copied()
        .collect();

    let nerf_indices = pick_unique_indices(&mut rng, &nerf_pool, 4);
    let buff_indices = pick_unique_indices(&mut rng, &buff_pool, 4);

    let mut notes: Vec<ChampionPatchNote> = Vec::new();

    for index in &buff_indices {
        working[*index].score = (working[*index].score + 9).clamp(30, 88);
        notes.push(ChampionPatchNote {
            champion_id: working[*index].champion_id.clone(),
            role: working[*index].role.clone(),
            change: ChampionPatchChange::Buff,
        });
    }

    for index in &nerf_indices {
        working[*index].score = (working[*index].score - 9).clamp(24, 84);
        notes.push(ChampionPatchNote {
            champion_id: working[*index].champion_id.clone(),
            role: working[*index].role.clone(),
            change: ChampionPatchChange::Nerf,
        });
    }

    let next_tier_map = tier_map_from_working(&working);
    let next_meta: Vec<ChampionMetaEntry> = working
        .iter()
        .map(|entry| ChampionMetaEntry {
            champion_id: entry.champion_id.clone(),
            role: entry.role.clone(),
            tier: next_tier_map
                .get(&champion_role_key(&entry.champion_id, &entry.role))
                .cloned()
                .unwrap_or_else(|| "C".to_string()),
        })
        .collect();

    let changed_set: HashSet<String> = notes
        .iter()
        .map(|note| normalize_key(&note.champion_id))
        .collect();
    let next_meta_set: HashSet<String> = next_meta
        .iter()
        .map(|entry| normalize_key(&entry.champion_id))
        .collect();
    let retained_discovery: Vec<String> = game
        .champion_patch
        .discovered_champion_ids
        .iter()
        .filter(|champion| {
            let key = normalize_key(champion);
            !changed_set.contains(&key) && next_meta_set.contains(&key)
        })
        .cloned()
        .collect();

    let year = two_digit_year(game);
    let next_index =
        if game.champion_patch.patch_year == year && game.champion_patch.patch_index_in_year > 0 {
            game.champion_patch.patch_index_in_year.saturating_add(1)
        } else {
            1
        };

    game.champion_patch.current_patch = game.champion_patch.current_patch.saturating_add(1);
    game.champion_patch.patch_year = year;
    game.champion_patch.patch_index_in_year = next_index;
    game.champion_patch.current_patch_label = format_patch_label(year, next_index);
    game.champion_patch.last_patch_date = Some(today_str(game));
    game.champion_patch.hidden_meta = next_meta;
    game.champion_patch.patch_notes = notes.clone();
    game.champion_patch.discovered_champion_ids = retained_discovery;

    let buffs: Vec<String> = notes
        .iter()
        .filter(|note| note.change == ChampionPatchChange::Buff)
        .map(|note| note.champion_id.clone())
        .collect();
    let nerfs: Vec<String> = notes
        .iter()
        .filter(|note| note.change == ChampionPatchChange::Nerf)
        .map(|note| note.champion_id.clone())
        .collect();

    let label = game.champion_patch.current_patch_label.clone();
    let buffed_list = if buffs.is_empty() {
        "-".to_string()
    } else {
        buffs.join(", ")
    };
    let nerfed_list = if nerfs.is_empty() {
        "-".to_string()
    } else {
        nerfs.join(", ")
    };
    let body = format!(
        "Patch {} deployed.\n\nBuffed: {}\nNerfed: {}\n\nYour staff is already scouting the new tier shifts.",
        label, buffed_list, nerfed_list,
    );

    let msg = InboxMessage::new(
        format!("msg_patch_{}", uuid::Uuid::new_v4()),
        format!("Patch {} Notes", label),
        body,
        "League Operations".to_string(),
        game.clock.current_date.to_rfc3339(),
    )
    .with_category(MessageCategory::System)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Competition Team")
    .with_i18n(
        "be.msg.patchNotes.subject",
        "be.msg.patchNotes.body",
        params(&[
            ("label", &label),
            ("buffed", &buffed_list),
            ("nerfed", &nerfed_list),
        ]),
    )
    .with_sender_i18n("be.sender.leagueOffice", "be.role.competitionSecretary");

    game.messages.push(msg);
}

fn process_meta_discovery(game: &mut Game) {
    if game.champion_patch.hidden_meta.is_empty() {
        return;
    }

    let Some(manager_team_id) = game.manager.team_id.as_deref() else {
        return;
    };

    let scouts: Vec<_> = game
        .staff
        .iter()
        .filter(|staff| {
            staff.team_id.as_deref() == Some(manager_team_id) && staff.role == StaffRole::Scout
        })
        .collect();

    if scouts.is_empty() {
        return;
    }

    let avg_scouting = scouts
        .iter()
        .map(|scout| scout.attributes.judging_ability as f64)
        .sum::<f64>()
        / scouts.len() as f64;
    let avg_potential = scouts
        .iter()
        .map(|scout| scout.attributes.judging_potential as f64)
        .sum::<f64>()
        / scouts.len() as f64;
    let staff_effects = LolStaffEffects::for_team(&game.staff, manager_team_id);

    let mut reveals = 6usize;
    reveals += scouts.len() * 2;
    reveals += (avg_scouting / 25.0).floor() as usize;
    // Ability discovers the current obvious meta; potential adds a small read on
    // sleeper/future-ish picks. The data model only stores discovered champions,
    // so we express that as a conservative extra reveal count instead of inventing
    // speculative hints.
    reveals += (avg_potential / 50.0).floor() as usize;
    reveals = ((reveals as f64) * staff_effects.meta_discovery).round() as usize;

    // Discovery runs every day, so salt with the date to vary the random reveal
    // count and the picks instead of repeating the same draw every single day.
    let discovery_salt = format!("discovery:{}", today_str(game));
    let mut rng =
        StdRng::seed_from_u64(derived_seed(game.champion_patch.rng_seed, &discovery_salt));
    reveals += rng.random_range(0..=4);

    let discovered_set: HashSet<String> = game
        .champion_patch
        .discovered_champion_ids
        .iter()
        .map(|value| normalize_key(value))
        .collect();

    let mut candidate_keys: Vec<String> = game
        .champion_patch
        .hidden_meta
        .iter()
        .map(|entry| normalize_key(&entry.champion_id))
        .filter(|key| !discovered_set.contains(key))
        .collect();
    candidate_keys.sort();
    candidate_keys.dedup();

    if candidate_keys.is_empty() {
        return;
    }

    let reveal_count = reveals.min(candidate_keys.len()).min(20);
    let mut candidate_indices: Vec<usize> = (0..candidate_keys.len()).collect();
    for _ in 0..reveal_count {
        if candidate_indices.is_empty() {
            break;
        }
        let pick = rng.random_range(0..candidate_indices.len());
        let chosen_idx = candidate_indices.swap_remove(pick);
        game.champion_patch
            .discovered_champion_ids
            .push(candidate_keys[chosen_idx].clone());
    }
}

pub fn process_daily_champion_system(game: &mut Game) {
    bootstrap_champion_state(game);
    apply_mastery_decay(game);

    // Cache SoloQ LP in each player so the frontend can read it synchronously
    // from gameState.players instead of issuing a separate IPC invoke.
    let day_index = days_between(game.clock.start_date, game.clock.current_date);
    let player_ids: Vec<String> = game.players.iter().map(|p| p.id.clone()).collect();
    for player_id in &player_ids {
        let idx = game
            .players
            .iter()
            .position(|p| &p.id == player_id)
            .unwrap();
        let lp = {
            let player = &game.players[idx];
            (soloq_points_at(game, player, day_index) - SOLOQ_POINTS_BASELINE).max(0.0)
        };
        game.players[idx].soloq_lp = lp;
    }

    if should_roll_patch(game, &game.champion_patch) {
        apply_patch(game);
    }

    process_meta_discovery(game);
}

// ── Champion catalog (from JSON data file) ─────────────────

#[derive(Deserialize)]
pub struct ChampionListFile {
    pub champions: Vec<ChampionListEntry>,
}

#[derive(Deserialize)]
pub struct ChampionListEntry {
    pub id: String,
    pub name: String,
    pub tags: Vec<String>,
    pub image: String,
}

/// Load the champion catalog from `assets/draft/champion-list.json`.
/// Returns an empty vec if the file cannot be read.
pub fn load_champion_catalog(data_base: &Path) -> Vec<crate::domain::champion::Champion> {
    let path = data_base
        .parent()
        .unwrap_or(data_base)
        .join("assets")
        .join("draft")
        .join("champion-list.json");
    load_champion_catalog_from_path(&path)
}

/// Load the champion catalog from a specific file path.
/// Returns an empty vec if the file cannot be read.
pub fn load_champion_catalog_from_path(path: &Path) -> Vec<crate::domain::champion::Champion> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Failed to read champion catalog at {:?}: {e}", path);
            return vec![];
        }
    };
    let list: ChampionListFile = match serde_json::from_str(&content) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("Failed to parse champion catalog: {e}");
            return vec![];
        }
    };
    list.champions
        .into_iter()
        .enumerate()
        .map(|(i, entry)| {
            let champion_key = entry.id.clone();
            let name = entry.name.clone();
            crate::domain::champion::Champion {
                id: (i + 1) as i64,
                name,
                champion_key,
                roles_json: serde_json::to_string(&entry.tags).unwrap_or_default(),
                counterpicks_json: None,
                synergies_json: None,
                image_tile_url: Some(format!("/champion-tiles/{}.webp", entry.id)),
                image_splash_url: Some(format!("/champion-splash/{}.jpg", entry.id)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::domain::manager::Manager;
    use crate::domain::player::{LolRole, Player, PlayerAttributes};
    use crate::domain::staff::{Staff, StaffAttributes, StaffRole};
    use crate::domain::team::Team;
    use chrono::{TimeZone, Utc};

    fn attrs() -> PlayerAttributes {
        PlayerAttributes {
            mechanics: 60,
            laning: 60,
            teamfighting: 60,
            macro_play: 60,
            consistency: 60,
            shotcalling: 60,
            champion_pool: 60,
            discipline: 60,
            mental_resilience: 60,
        }
    }

    fn game_with_lineup(lineup: Vec<&str>) -> Game {
        let mut manager = Manager::new(
            "manager-1".to_string(),
            "Jane".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "ES".to_string(),
        );
        manager.hire("team-1".to_string());

        let mut team = Team::new(
            "team-1".to_string(),
            "Team One".to_string(),
            "ONE".to_string(),
            "ES".to_string(),
            "Madrid".to_string(),
            "Arena".to_string(),
            10_000,
        );
        team.active_lineup_ids = lineup.into_iter().map(str::to_string).collect();

        Game::new(
            GameClock::new(test_date(2026, 1, 6)),
            manager,
            vec![team],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    fn test_date(year: i32, month: u32, day: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 12, 0, 0)
            .single()
            .unwrap()
    }

    fn game_with_player() -> Game {
        let mut game = game_with_lineup(Vec::new());
        let mut player = Player::new(
            "player-1".to_string(),
            "Player One".to_string(),
            "Player One".to_string(),
            "2000-01-01".to_string(),
            "ES".to_string(),
            LolRole::Mid,
            attrs(),
        );
        player.team_id = Some("team-1".to_string());
        game.players.push(player);
        game
    }

    #[test]
    fn patches_only_roll_on_tuesday_after_the_fortnightly_interval() {
        let mut game = game_with_lineup(Vec::new());
        game.clock.current_date = test_date(2026, 1, 20); // Tuesday
        let state = ChampionPatchState {
            last_patch_date: Some("2026-01-06".to_string()),
            ..Default::default()
        };
        assert!(should_roll_patch(&game, &state));

        game.clock.current_date = test_date(2026, 1, 19); // Monday
        assert!(!should_roll_patch(&game, &state));

        game.clock.current_date = test_date(2026, 1, 13); // Tuesday, one week later
        assert!(!should_roll_patch(&game, &state));
    }

    #[test]
    fn patch_resets_only_discovery_for_its_noted_champions() {
        let mut game = game_with_lineup(Vec::new());
        game.clock.current_date = test_date(2026, 1, 20);
        game.champion_patch.rng_seed = 7;
        game.champion_patch.current_patch = 1;
        game.champion_patch.patch_year = 26;
        game.champion_patch.patch_index_in_year = 1;
        game.champion_patch.hidden_meta = champion_catalog()
            .iter()
            .map(|(champion_id, role)| ChampionMetaEntry {
                champion_id: champion_id.clone(),
                role: role.clone(),
                tier: "C".to_string(),
            })
            .collect();
        game.champion_patch.discovered_champion_ids = champion_catalog()
            .iter()
            .map(|(champion_id, _)| champion_id.clone())
            .collect();

        apply_patch(&mut game);

        let reset: HashSet<String> = game
            .champion_patch
            .patch_notes
            .iter()
            .map(|note| normalize_key(&note.champion_id))
            .collect();
        let retained: HashSet<String> = game
            .champion_patch
            .discovered_champion_ids
            .iter()
            .map(|champion| normalize_key(champion))
            .collect();
        assert!(!reset.is_empty());
        assert!(reset.is_disjoint(&retained));
    }

    #[test]
    fn discovery_adds_unique_champions_and_never_exceeds_available_candidates() {
        let mut game = game_with_lineup(Vec::new());
        let mut scout = Staff::new(
            "scout-1".to_string(),
            "Scout".to_string(),
            "One".to_string(),
            "1990-01-01".to_string(),
            StaffRole::Scout,
            StaffAttributes {
                judging_ability: 100,
                judging_potential: 100,
                ..Default::default()
            },
        );
        scout.team_id = Some("team-1".to_string());
        game.staff.push(scout);
        game.champion_patch.rng_seed = 99;
        game.champion_patch.hidden_meta = vec![
            ChampionMetaEntry {
                champion_id: "Ahri".to_string(),
                role: "Mid".to_string(),
                tier: "S".to_string(),
            },
            ChampionMetaEntry {
                champion_id: "Jinx".to_string(),
                role: "ADC".to_string(),
                tier: "A".to_string(),
            },
            ChampionMetaEntry {
                champion_id: "Garen".to_string(),
                role: "Top".to_string(),
                tier: "B".to_string(),
            },
        ];
        game.champion_patch.discovered_champion_ids = vec!["Ahri".to_string()];

        process_meta_discovery(&mut game);

        let discovered: HashSet<String> = game
            .champion_patch
            .discovered_champion_ids
            .iter()
            .map(|champion| normalize_key(champion))
            .collect();
        assert_eq!(
            discovered.len(),
            game.champion_patch.discovered_champion_ids.len()
        );
        assert_eq!(discovered.len(), 3);
    }

    #[test]
    fn mastery_stays_within_bounds_and_decays_at_inactivity_thresholds() {
        let mut game = game_with_player();
        game.clock.current_date = test_date(2026, 3, 1);
        upsert_mastery(&mut game, "player-1", "Ahri", 255);
        game.champion_masteries[0].last_active_on = "2026-01-04".to_string();
        assert_eq!(
            mastery_for_player_champion(&game, "player-1", "Ahri"),
            MASTERY_CAP
        );

        game.champion_masteries.push(ChampionMasteryEntry {
            player_id: "player-1".to_string(),
            champion_id: "Jinx".to_string(),
            mastery: MIN_MASTERY,
            last_active_on: "2026-01-04".to_string(),
        });
        apply_mastery_decay(&mut game);

        assert_eq!(
            mastery_for_player_champion(&game, "player-1", "Ahri"),
            MASTERY_CAP - 1
        );
        assert_eq!(
            mastery_for_player_champion(&game, "player-1", "Jinx"),
            MIN_MASTERY
        );
    }

    #[test]
    fn training_targets_compact_priorities_and_remove_normalized_duplicates() {
        let mut game = game_with_player();
        set_player_training_target(&mut game, "player-1", 1, Some("Ahri".to_string())).unwrap();
        set_player_training_target(&mut game, "player-1", 2, Some("AHRI".to_string())).unwrap();
        set_player_training_target(&mut game, "player-1", 2, Some("Jinx".to_string())).unwrap();

        let targets = &game.players[0].champion_training_targets;
        assert_eq!(
            targets,
            &vec!["AHRI".to_string(), "Jinx".to_string(), String::new()]
        );
        assert!(
            set_player_training_target(&mut game, "player-1", 3, Some("Orianna".to_string()))
                .is_err()
        );
    }

    #[test]
    fn soloq_status_is_deterministic_bounded_and_uses_its_tier_multiplier() {
        let mut game = game_with_player();
        game.clock.current_date = test_date(2026, 3, 7);
        let player = &game.players[0];

        let first = soloq_status_for_player(&game, player);
        let second = soloq_status_for_player(&game, player);
        assert_eq!(first.lp, second.lp);
        assert_eq!(first.delta, second.delta);
        assert!(first.lp <= (SOLOQ_POINTS_MAX - SOLOQ_POINTS_BASELINE) as u32);
        assert_eq!(
            first.multiplier,
            soloq_multiplier_for_tier(soloq_tier_from_lp(f64::from(first.lp)))
        );
        assert_eq!(soloq_tier_from_lp(799.0), SoloQTier::Master);
        assert_eq!(soloq_tier_from_lp(800.0), SoloQTier::Grandmaster);
        assert_eq!(soloq_tier_from_lp(1300.0), SoloQTier::Challenger);
    }

    #[test]
    fn current_role_for_player_uses_active_lineup_slot_before_natural_role() {
        let mut game = game_with_lineup(vec!["new-top", "jungle", "mid", "adc", "support"]);
        let mut player = Player::new(
            "new-top".to_string(),
            "New Top".to_string(),
            "New Top".to_string(),
            "2000-01-01".to_string(),
            "ES".to_string(),
            LolRole::Support,
            attrs(),
        );
        player.team_id = Some("team-1".to_string());
        game.players.push(player.clone());

        expect_role(&game, &player, LolRole::Top);
    }

    #[test]
    fn current_role_for_player_keeps_bench_player_natural_role() {
        let mut game = game_with_lineup(vec!["top", "jungle", "mid", "adc", "support"]);
        let mut player = Player::new(
            "bench-support".to_string(),
            "Bench Support".to_string(),
            "Bench Support".to_string(),
            "2000-01-01".to_string(),
            "ES".to_string(),
            LolRole::Support,
            attrs(),
        );
        player.team_id = Some("team-1".to_string());
        game.players.push(player.clone());

        expect_role(&game, &player, LolRole::Support);
    }

    fn expect_role(game: &Game, player: &Player, expected: LolRole) {
        let team_id = player.team_id.as_deref().unwrap();
        assert_eq!(
            current_role_for_player(game, team_id, &player.id, player.natural_position),
            expected,
        );
    }
}
