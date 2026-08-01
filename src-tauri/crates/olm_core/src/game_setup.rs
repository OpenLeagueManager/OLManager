use crate::domain::message::{InboxMessage, MessageCategory, MessageContext, MessagePriority};
use crate::domain::player::{Player, PlayerAttributes};
use crate::domain::team::{
    AcademyLifecycle, AcademyMetadata, ErlAssignment, ErlAssignmentRule, Team, TeamKind,
};
use chrono::{Datelike, NaiveDate};
use log::{info, warn};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::academy;
use crate::game::Game;
use crate::generator::definitions::PlayerDataFile;
use crate::potential;
use crate::state::RESOURCE_DATA_DIR;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const ACADEMY_FALLBACK_PHOTO: &str = "/player-photos/107455908655055017.webp";

// ---------------------------------------------------------------------------
// Age helper
// ---------------------------------------------------------------------------

pub fn calculate_age_on_date(birth_date: chrono::NaiveDate, as_of_date: chrono::NaiveDate) -> i32 {
    let mut age = as_of_date.year() - birth_date.year();
    if (as_of_date.month(), as_of_date.day()) < (birth_date.month(), birth_date.day()) {
        age -= 1;
    }
    age
}

// ---------------------------------------------------------------------------
// Seed types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DraftSeedRoot {
    pub data: DraftSeedData,
}

#[derive(Debug, Deserialize)]
pub struct DraftSeedData {
    #[allow(dead_code)]
    pub rostered_seeds: Vec<DraftPlayerSeed>,
    #[serde(default)]
    pub free_agent_seeds: Vec<DraftPlayerSeed>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DraftPlayerSeed {
    pub ign: String,
    #[serde(default)]
    #[serde(rename = "firstName")]
    pub first_name: Option<String>,
    #[serde(default)]
    #[serde(rename = "lastName")]
    pub last_name: Option<String>,
    #[serde(default)]
    pub dob: Option<String>,
    #[serde(default)]
    pub nationality: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    #[serde(rename = "teamId")]
    pub team_id: Option<String>,
    #[serde(default)]
    pub rating: Option<u8>,
    #[serde(default)]
    pub potential: Option<u8>,
    #[serde(default)]
    pub salary: Option<u32>,
    #[serde(default)]
    #[serde(rename = "contractEnd")]
    pub contract_end: Option<String>,
    #[serde(default)]
    #[serde(rename = "marketValue")]
    pub market_value: Option<u64>,
    #[serde(default)]
    pub reputation: Option<u8>,
    #[serde(default)]
    pub photo: Option<String>,
}

// ---------------------------------------------------------------------------
// Seed helper functions
// ---------------------------------------------------------------------------

pub fn seed_profile_image_url(photo: Option<&str>) -> Option<String> {
    let value = photo?.trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with("/images/") {
        return Some(format!("/data/lec{}", value));
    }
    Some(value.to_string())
}

pub fn normalize_seed_name(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

pub fn seed_is_free_agent(seed: &DraftPlayerSeed) -> bool {
    seed.team_id
        .as_deref()
        .map(normalize_seed_name)
        .map(|team| team == "fa" || team == "freeagent")
        .unwrap_or(true)
}

pub fn clamp_stat(value: i16) -> u8 {
    value.clamp(25, 99) as u8
}

pub fn role_to_lol_role(role: Option<&str>) -> crate::domain::stats::LolRole {
    let key = role.map(normalize_seed_name).unwrap_or_default();
    match key.as_str() {
        "top" => crate::domain::stats::LolRole::Top,
        "jungle" => crate::domain::stats::LolRole::Jungle,
        "mid" | "middle" => crate::domain::stats::LolRole::Mid,
        "bot" | "adc" | "bottom" => crate::domain::stats::LolRole::Adc,
        "support" | "sup" | "utility" => crate::domain::stats::LolRole::Support,
        _ => crate::domain::stats::LolRole::Mid,
    }
}

// ---------------------------------------------------------------------------
// Stat building from seeds
// ---------------------------------------------------------------------------

pub fn build_lol_stats_from_seed(seed: &DraftPlayerSeed) -> [u8; 9] {
    let target = i16::from(seed.rating.unwrap_or(60).clamp(45, 95));
    let role_key = normalize_seed_name(seed.role.as_deref().unwrap_or(""));
    let role_bias: [i16; 9] = match role_key.as_str() {
        "top" => [1, 0, 1, 0, 1, 1, 0, 1, 2],
        "jungle" => [0, 0, 1, 2, 1, 2, 1, 1, 1],
        "mid" | "middle" => [2, 2, 0, 1, 0, 1, 1, 0, 0],
        "bot" | "adc" | "bottom" => [2, 2, 1, 0, 0, 0, 1, 0, 1],
        "support" | "sup" | "utility" => [0, 0, 1, 2, 1, 2, 0, 1, 1],
        _ => [0, 0, 0, 0, 0, 0, 0, 0, 0],
    };

    let base_hash = normalize_seed_name(&seed.ign)
        .chars()
        .fold(0_i16, |acc, ch| acc.wrapping_add(ch as i16));

    let mut values = [target; 9];
    for index in 0..9 {
        let jitter = ((base_hash + (index as i16 * 7)) % 5) - 2;
        values[index] = target + role_bias[index] + jitter;
    }

    let current_avg = values.iter().sum::<i16>() / 9;
    let mut delta = target - current_avg;
    let mut cursor = 0_usize;
    while delta != 0 {
        let direction = if delta > 0 { 1 } else { -1 };
        let candidate = values[cursor] + direction;
        if (25..=99).contains(&candidate) {
            values[cursor] = candidate;
            delta -= direction;
        }
        cursor = (cursor + 1) % 9;
    }

    [
        clamp_stat(values[0]),
        clamp_stat(values[1]),
        clamp_stat(values[2]),
        clamp_stat(values[3]),
        clamp_stat(values[4]),
        clamp_stat(values[5]),
        clamp_stat(values[6]),
        clamp_stat(values[7]),
        clamp_stat(values[8]),
    ]
}

pub fn build_attributes_from_seed(seed: &DraftPlayerSeed) -> PlayerAttributes {
    let [
        mechanics,
        laning,
        teamfighting,
        macro_play,
        consistency,
        shotcalling,
        champion_pool,
        discipline,
        mental_resilience,
    ] = build_lol_stats_from_seed(seed);

    PlayerAttributes {
        mechanics,
        laning,
        teamfighting,
        macro_play,
        consistency,
        shotcalling,
        champion_pool,
        discipline,
        mental_resilience,
    }
}

// ---------------------------------------------------------------------------
// Free agent player building
// ---------------------------------------------------------------------------

pub fn build_free_agent_player(seed: &DraftPlayerSeed, index: usize) -> Option<Player> {
    let ign = seed.ign.trim();
    if ign.is_empty() {
        return None;
    }

    let first_name = seed.first_name.as_deref().unwrap_or(ign).trim().to_string();
    let last_name = seed.last_name.as_deref().unwrap_or("").trim().to_string();
    let full_name = if last_name.is_empty() {
        first_name.clone()
    } else {
        format!("{} {}", first_name, last_name)
    };

    let dob = seed.dob.clone().unwrap_or_else(|| "2002-01-01".to_string());
    let nationality = seed.nationality.clone().unwrap_or_else(|| "KR".to_string());
    let position = role_to_lol_role(seed.role.as_deref());
    let attributes = build_attributes_from_seed(seed);

    let seed_key = normalize_seed_name(ign);
    let id = format!("lec-fa-{}-{}", seed_key, index + 1);

    let mut player = Player::new(
        id,
        ign.to_string(),
        full_name,
        dob,
        nationality,
        position,
        attributes,
    );
    player.team_id = None;
    player.transfer_listed = true;
    player.loan_listed = false;
    let seed_ovr = seed.rating.unwrap_or(70);
    let seed_potential = seed.potential.unwrap_or(seed_ovr).max(seed_ovr);
    player.market_value = seed
        .market_value
        .unwrap_or_else(|| suggested_seed_market_value(seed_ovr, seed_potential, false));
    player.wage = seed.salary.unwrap_or(40_000);
    player.contract_end = seed.contract_end.clone();
    player.condition = 100;
    player.morale = seed.reputation.unwrap_or(68).clamp(35, 95);
    player.potential_base = seed
        .potential
        .unwrap_or(seed.rating.unwrap_or(70))
        .clamp(45, 99);
    player.profile_image_url = seed_profile_image_url(seed.photo.as_deref());

    Some(player)
}

// ---------------------------------------------------------------------------
// Seed loading — reads from RESOURCE_DATA_DIR (set by caller)
// ---------------------------------------------------------------------------

pub fn load_draft_seed_root() -> DraftSeedRoot {
    // Runtime read from assets/draft/players.json for world editor compatibility.
    // Returns empty if file doesn't exist — Flow C provides players from modular data.
    let Some(content) = RESOURCE_DATA_DIR
        .get()
        .map(|dir| {
            dir.parent()
                .unwrap_or(dir)
                .join("assets")
                .join("draft")
                .join("players.json")
        })
        .filter(|p| p.exists())
        .or_else(|| {
            std::env::current_dir().ok().and_then(|cwd| {
                let mut path = cwd.clone();
                path.push("assets");
                path.push("draft");
                path.push("players.json");
                if path.exists() {
                    return Some(path);
                }
                path = cwd;
                path.push("..");
                path.push("assets");
                path.push("draft");
                path.push("players.json");
                if path.exists() {
                    return Some(path);
                }
                None
            })
        })
        .and_then(|p| std::fs::read_to_string(p).ok())
    else {
        return DraftSeedRoot {
            data: DraftSeedData {
                rostered_seeds: vec![],
                free_agent_seeds: vec![],
            },
        };
    };

    let mut merged = serde_json::from_str::<DraftSeedRoot>(&content).unwrap_or(DraftSeedRoot {
        data: DraftSeedData {
            rostered_seeds: vec![],
            free_agent_seeds: vec![],
        },
    });

    if let Some(external) = load_external_more_fa_seed() {
        let mut seen = HashSet::new();
        let mut combined = merged.data.free_agent_seeds;
        combined.extend(external.data.free_agent_seeds);
        combined.retain(|seed| {
            let key = normalize_seed_name(&seed.ign);
            if key.is_empty() || seen.contains(&key) {
                return false;
            }
            seen.insert(key)
        });
        merged.data.free_agent_seeds = combined;
    }

    merged
}

pub fn load_external_more_fa_seed() -> Option<DraftSeedRoot> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        candidates.push(
            std::path::PathBuf::from(user_profile)
                .join("Downloads")
                .join("MoreFA_Players.json"),
        );
    }

    if let Ok(home) = std::env::var("HOME") {
        candidates.push(
            std::path::PathBuf::from(home)
                .join("Downloads")
                .join("MoreFA_Players.json"),
        );
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("MoreFA_Players.json"));
    }

    for path in candidates {
        if !path.exists() {
            continue;
        }

        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };

        if let Ok(parsed) = serde_json::from_str::<DraftSeedRoot>(&raw) {
            return Some(parsed);
        }
    }

    None
}

/// Load and cache free agent players from `data/players/free_agents.json`.
/// Uses OnceLock for lazy init — file is read once per process lifetime.
pub fn load_free_agent_players() -> &'static Vec<Player> {
    static FREE_AGENTS: OnceLock<Vec<Player>> = OnceLock::new();
    FREE_AGENTS.get_or_init(|| {
        let resource = RESOURCE_DATA_DIR
            .get()
            .map(|p| p.join("players").join("free_agents.json"));
        let cwd = std::env::current_dir().ok();
        let candidates = [
            resource,
            cwd.as_ref()
                .map(|p| p.join("data").join("players").join("free_agents.json")),
            cwd.as_ref().map(|p| {
                p.join("..")
                    .join("data")
                    .join("players")
                    .join("free_agents.json")
            }),
            cwd.as_ref().map(|p| {
                p.join("src-tauri")
                    .join("data")
                    .join("players")
                    .join("free_agents.json")
            }),
        ];

        for path in candidates.iter().flatten() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(data) = serde_json::from_str::<PlayerDataFile>(&content) {
                    info!(
                        "[game] loaded {} free agent players from {:?}",
                        data.players.len(),
                        path
                    );
                    return data.players;
                }
            }
        }

        warn!("[game] data/players/free_agents.json not found — using empty pool");
        Vec::new()
    })
}

/// Inject players from `data/players/free_agents.json` into the player list.
/// Deduplicates by player ID to avoid collisions with competition-loaded players.
pub fn inject_json_free_agents(players: &mut Vec<Player>) {
    let mut existing_ids: HashSet<String> = players.iter().map(|p| p.id.clone()).collect();
    for fa in load_free_agent_players() {
        if !existing_ids.contains(&fa.id) {
            players.push(fa.clone());
            existing_ids.insert(fa.id.clone());
        }
    }
}

// ---------------------------------------------------------------------------
// Draft seed caching
// ---------------------------------------------------------------------------

pub fn draft_seed_root() -> &'static DraftSeedRoot {
    static ROOT: OnceLock<DraftSeedRoot> = OnceLock::new();
    ROOT.get_or_init(load_draft_seed_root)
}

#[allow(dead_code)]
pub fn draft_potential_map() -> &'static HashMap<String, u8> {
    static POTENTIALS: OnceLock<HashMap<String, u8>> = OnceLock::new();
    POTENTIALS.get_or_init(|| {
        let parsed = draft_seed_root();

        let mut all_seeds = parsed.data.rostered_seeds.clone();
        all_seeds.extend(parsed.data.free_agent_seeds.clone());

        all_seeds
            .into_iter()
            .filter_map(|seed| {
                let key = normalize_seed_name(&seed.ign);
                if key.is_empty() {
                    return None;
                }
                Some((key, seed.potential.unwrap_or(99)))
            })
            .collect()
    })
}

pub fn potential_seed_for_player(match_name: &str) -> Option<u8> {
    let key = normalize_seed_name(match_name);
    draft_potential_map().get(&key).copied().or_else(
        || {
            if key == "kyeahoo" { Some(89) } else { None }
        },
    )
}

pub fn apply_seed_potential_defaults(players: &mut [Player]) {
    for player in players.iter_mut() {
        let Some(seed_potential) = potential_seed_for_player(&player.match_name) else {
            continue;
        };
        let current_ovr = potential::calculate_lol_ovr(player);
        player.potential_base = seed_potential.max(current_ovr).min(99);
        player.potential_revealed = None;
        player.potential_research_started_on = None;
        player.potential_research_eta_days = None;
    }
}

// ---------------------------------------------------------------------------
// Contract helpers
// ---------------------------------------------------------------------------

pub fn default_initial_contract_end_for_start_year(start_year: i32) -> String {
    format!("{}-11-30", start_year + 1)
}

pub fn apply_default_initial_contract_end(players: &mut [Player]) {
    let default_initial_contract_end = default_initial_contract_end_for_start_year(2026);
    let min_start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

    for player in players.iter_mut() {
        let current = player
            .contract_end
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok());
        if current.is_none() || current.is_some_and(|d| d < min_start) {
            player.contract_end = Some(default_initial_contract_end.clone());
        }
    }
}

pub fn apply_default_market_values(players: &mut [Player]) {
    for player in players.iter_mut() {
        if player.market_value > 0 {
            continue;
        }
        let ovr = potential::calculate_lol_ovr(player);
        let potential = player.potential_base.max(ovr);
        player.market_value = suggested_seed_market_value(ovr, potential, false);
    }
}

// ---------------------------------------------------------------------------
// Free agent injection from seeds
// ---------------------------------------------------------------------------

pub fn inject_seed_free_agents(players: &mut Vec<Player>) {
    let existing_ids: HashSet<String> = players
        .iter()
        .map(|player| normalize_seed_name(&player.match_name))
        .collect();
    let mut existing_ids = existing_ids;

    for (index, seed) in draft_seed_root().data.free_agent_seeds.iter().enumerate() {
        if !seed_is_free_agent(seed) {
            continue;
        }

        let seed_key = normalize_seed_name(&seed.ign);
        if seed_key.is_empty() || existing_ids.contains(&seed_key) {
            continue;
        }

        if let Some(player) = build_free_agent_player(seed, index) {
            players.push(player);
            existing_ids.insert(seed_key);
        }
    }
}

pub fn remove_free_agents_shadowed_by_academy(players: &mut Vec<Player>, teams: &[Team]) {
    let academy_team_ids: HashSet<&str> = teams
        .iter()
        .filter(|team| team.team_kind == TeamKind::Academy)
        .map(|team| team.id.as_str())
        .collect();

    if academy_team_ids.is_empty() {
        return;
    }

    let academy_names: HashSet<String> = players
        .iter()
        .filter(|player| {
            player
                .team_id
                .as_deref()
                .map(|team_id| academy_team_ids.contains(team_id))
                .unwrap_or(false)
        })
        .map(|player| normalize_seed_name(&player.match_name))
        .filter(|key| !key.is_empty())
        .collect();

    if academy_names.is_empty() {
        return;
    }

    players.retain(|player| {
        player.team_id.is_some()
            || !academy_names.contains(&normalize_seed_name(&player.match_name))
    });
}

// ---------------------------------------------------------------------------
// Academy helpers
// ---------------------------------------------------------------------------

pub fn generate_fallback_dob(seed: &str) -> String {
    let hash = seed.bytes().fold(0_u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    let year = 2004 + (hash % 4);
    let month = 1 + ((hash / 7) % 12);
    let day = 1 + ((hash / 13) % 28);
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn academy_team_alias_for_parent(parent_name: &str) -> Option<&'static str> {
    match academy::normalize_key(parent_name).as_str() {
        "movistarkoi" => Some("movistarkoifnix"),
        "g2esports" => Some("g2nord"),
        "giantx" => Some("giantxitero"),
        "karminecorp" => Some("karminecorpblue"),
        "teamvitality" => Some("teamvitalitybee"),
        "teamheretics" | "heretics" => Some("teamheretics"),
        _ => None,
    }
}

pub fn generated_academy_ovr(player_name: &str) -> u8 {
    let hash = player_name.bytes().fold(0_u32, |acc, byte| {
        acc.wrapping_mul(37).wrapping_add(u32::from(byte))
    });
    (60 + (hash % 11)) as u8
}

pub fn generated_academy_potential(player_name: &str, ovr: u8) -> u8 {
    let hash = player_name.bytes().fold(0_u32, |acc, byte| {
        acc.wrapping_mul(41).wrapping_add(u32::from(byte))
    });
    let elite = (hash % 100) < 15;
    let mut potential = if elite {
        84 + ((hash / 7) % 7) as u8
    } else {
        75 + ((hash / 5) % 9) as u8
    };
    let minimum = ovr.saturating_add(4).min(90);
    if potential < minimum {
        potential = minimum;
    }
    potential.min(90)
}

pub fn round_seed_market_value(value: u64) -> u64 {
    value.max(50_000).div_ceil(10_000) * 10_000
}

pub fn suggested_seed_market_value(ovr: u8, potential: u8, is_academy: bool) -> u64 {
    let skill_gap = u64::from(ovr.saturating_sub(60));
    let potential_gap = u64::from(potential.saturating_sub(ovr));
    let skill_value = 50_000 + skill_gap * skill_gap * 300;
    let potential_value = potential_gap * if is_academy { 3_000 } else { 6_000 };
    let raw_value = skill_value + potential_value;
    let adjusted_value = if is_academy {
        raw_value * 70 / 100
    } else {
        raw_value
    };

    round_seed_market_value(adjusted_value)
}

pub fn academy_overview_message(
    parent_team: &Team,
    academy_team: &Team,
    academy_players: usize,
    date: &str,
) -> InboxMessage {
    InboxMessage::new(
        format!("academy-overview-{}", parent_team.id),
        format!("Academia activa: {}", academy_team.name),
        format!(
            "Tu club ya cuenta con academia vinculada. {} tiene {} prospectos cargados y listos para seguimiento/promocion.",
            academy_team.name, academy_players
        ),
        "Director Deportivo".to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::Welcome)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Director Deportivo")
    .with_context(MessageContext {
        team_id: Some(parent_team.id.clone()),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Academy bootstrap
// ---------------------------------------------------------------------------

pub fn bootstrap_example_academy_pool_from_example(
    teams: &mut Vec<Team>,
    players: &mut Vec<Player>,
    current_date_iso: &str,
) {
    let seed_catalog = academy::academy_seed_catalog();
    if seed_catalog.is_empty() {
        return;
    }

    let mut existing_team_ids: HashSet<String> = teams.iter().map(|team| team.id.clone()).collect();

    for seed_team in seed_catalog.iter() {
        let academy_id = academy::seed_team_id(&seed_team.league_id, &seed_team.team_name);
        if existing_team_ids.contains(&academy_id) {
            continue;
        }

        let mut academy_team = Team::new(
            academy_id.clone(),
            seed_team.team_name.clone(),
            seed_team.short_name.clone(),
            seed_team.country_code.clone(),
            seed_team.league_name.clone(),
            format!("{} Academy Arena", seed_team.short_name),
            2_500,
        );
        academy_team.team_kind = TeamKind::Academy;
        academy_team.parent_team_id = None;
        academy_team.manager_id = None;
        academy_team.reputation = 6_000;
        academy_team.finance = 0;
        academy_team.wage_budget = 0;
        academy_team.transfer_budget = 0;
        academy_team.academy = Some(AcademyMetadata {
            lifecycle: AcademyLifecycle::Planned,
            erl_assignment: ErlAssignment {
                erl_league_id: seed_team.league_id.clone(),
                country_rule: ErlAssignmentRule::Domestic,
                fallback_reason: Some(format!(
                    "Seeded from {} academy roster",
                    seed_team.league_name
                )),
                reputation: 60,
                acquisition_cost: 0,
                acquired_at: String::new(),
                creation_cost: 0,
                created_at: current_date_iso.to_string(),
            },
            source_team_id: academy_id.clone(),
            original_name: seed_team.team_name.clone(),
            original_short_name: seed_team.short_name.clone(),
            original_logo_url: seed_team.logo_url.clone(),
            current_logo_url: seed_team.logo_url.clone(),
            acquisition_cost: 0,
            acquired_at: String::new(),
        });

        for (player_index, seed_player) in seed_team.players.iter().enumerate() {
            let ovr = generated_academy_ovr(&seed_player.nickname);
            let potential = generated_academy_potential(&seed_player.nickname, ovr);
            let seed = DraftPlayerSeed {
                ign: seed_player.nickname.clone(),
                first_name: None,
                last_name: None,
                dob: Some(
                    seed_player
                        .dob
                        .clone()
                        .unwrap_or_else(|| generate_fallback_dob(&seed_player.nickname)),
                ),
                nationality: Some(seed_player.nationality.clone()),
                role: Some(academy::role_to_canonical(&seed_player.role)),
                team_id: Some(academy_id.clone()),
                rating: Some(ovr),
                potential: Some(potential),
                salary: Some(8_000),
                contract_end: Some("2028-11-30".to_string()),
                market_value: Some(suggested_seed_market_value(ovr, potential, true)),
                reputation: Some(62),
                photo: seed_profile_image_url(
                    (!seed_player.image_url.is_empty()).then_some(seed_player.image_url.as_str()),
                ),
            };

            let attributes = build_attributes_from_seed(&seed);
            let position = role_to_lol_role(seed.role.as_deref());
            let player_id = format!("{}-player-{}", academy_id, player_index + 1);

            let mut player = Player::new(
                player_id,
                seed_player.nickname.clone(),
                seed_player.full_name.clone(),
                seed.dob
                    .clone()
                    .unwrap_or_else(|| generate_fallback_dob(&seed_player.nickname)),
                seed_player.nationality.clone(),
                position,
                attributes,
            );
            player.team_id = Some(academy_id.clone());
            player.contract_end = seed.contract_end.clone();
            player.wage = seed.salary.unwrap_or(8_000);
            player.market_value = seed.market_value.unwrap_or(240_000);
            player.potential_base = potential;
            player.profile_image_url = seed.photo.clone();
            player.morale = 68;
            player.condition = 100;

            if seed_player.image_url.is_empty() {
                let _ = ACADEMY_FALLBACK_PHOTO;
            }

            players.push(player);
        }

        existing_team_ids.insert(academy_id.clone());
        teams.push(academy_team);
    }

    let parent_snapshots: Vec<(usize, String, String)> = teams
        .iter()
        .enumerate()
        .filter(|(_, team)| team.team_kind == TeamKind::Main)
        .map(|(index, team)| (index, team.id.clone(), team.name.clone()))
        .collect();

    for (parent_index, parent_id, parent_name) in parent_snapshots {
        let Some(academy_alias) = academy_team_alias_for_parent(&parent_name) else {
            continue;
        };

        let Some(parent) = teams.get(parent_index) else {
            continue;
        };
        if parent.academy_team_id.is_some() {
            continue;
        }

        let Some(seed_team) = seed_catalog
            .iter()
            .find(|candidate| academy::normalize_key(&candidate.team_name) == academy_alias)
        else {
            continue;
        };

        let academy_id = academy::seed_team_id(&seed_team.league_id, &seed_team.team_name);
        let Some(academy_index) = teams
            .iter()
            .position(|team| team.id == academy_id && team.team_kind == TeamKind::Academy)
        else {
            continue;
        };

        if let Some(parent_mut) = teams.iter_mut().find(|team| team.id == parent_id) {
            parent_mut.academy_team_id = Some(academy_id.clone());
        }

        if let Some(academy_team) = teams.get_mut(academy_index) {
            academy_team.parent_team_id = Some(parent_id.clone());
            academy_team.manager_id = None;
            academy_team.reputation = 6_200;
            academy_team.academy = Some(AcademyMetadata {
                lifecycle: AcademyLifecycle::Active,
                erl_assignment: ErlAssignment {
                    erl_league_id: seed_team.league_id.clone(),
                    country_rule: ErlAssignmentRule::Domestic,
                    fallback_reason: Some(format!(
                        "Seeded from {} academy roster",
                        seed_team.league_name
                    )),
                    reputation: 62,
                    acquisition_cost: 0,
                    acquired_at: current_date_iso.to_string(),
                    creation_cost: 0,
                    created_at: current_date_iso.to_string(),
                },
                source_team_id: academy_id,
                original_name: seed_team.team_name.clone(),
                original_short_name: seed_team.short_name.clone(),
                original_logo_url: seed_team.logo_url.clone(),
                current_logo_url: seed_team.logo_url.clone(),
                acquisition_cost: 0,
                acquired_at: current_date_iso.to_string(),
            });
        }
    }
}

pub fn ensure_example_academy_pool(game: &mut Game) {
    let bootstrap_date = game.clock.current_date.format("%Y-%m-%d").to_string();
    bootstrap_example_academy_pool_from_example(
        &mut game.teams,
        &mut game.players,
        &bootstrap_date,
    );
    remove_free_agents_shadowed_by_academy(&mut game.players, &game.teams);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        apply_default_initial_contract_end, calculate_age_on_date,
        default_initial_contract_end_for_start_year,
    };
    use crate::domain::player::{Player, PlayerAttributes};
    use crate::domain::stats::LolRole;

    fn default_attrs() -> PlayerAttributes {
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

    fn player_with_contract(id: &str, contract_end: Option<&str>) -> Player {
        let mut player = Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            "2000-01-01".to_string(),
            "ES".to_string(),
            LolRole::Jungle,
            default_attrs(),
        );
        player.contract_end = contract_end.map(str::to_string);
        player
    }

    #[test]
    fn default_initial_contract_end_survives_first_next_season_friendlies() {
        assert_eq!(
            default_initial_contract_end_for_start_year(2025),
            "2026-11-30"
        );
        assert_eq!(
            default_initial_contract_end_for_start_year(2026),
            "2027-11-30"
        );
    }

    #[test]
    fn apply_default_initial_contract_end_only_fills_missing_contracts() {
        let mut players = vec![
            player_with_contract("missing", None),
            player_with_contract("existing", Some("2028-11-30")),
        ];

        apply_default_initial_contract_end(&mut players);

        assert_eq!(players[0].contract_end.as_deref(), Some("2027-11-30"));
        assert_eq!(players[1].contract_end.as_deref(), Some("2028-11-30"));
    }

    #[test]
    fn calculates_age_against_game_date_not_system_date() {
        let birth_date = chrono::NaiveDate::from_ymd_opt(2000, 1, 2).unwrap();
        let game_date = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();

        assert_eq!(calculate_age_on_date(birth_date, game_date), 24);
    }

    #[test]
    fn increments_age_on_birthday() {
        let birth_date = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let game_date = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();

        assert_eq!(calculate_age_on_date(birth_date, game_date), 25);
    }
}
