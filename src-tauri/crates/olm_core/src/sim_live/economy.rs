use super::{
    CHAMPION_KILL_GOLD, CHAMPION_KILL_GOLD_MAX, CHAMPION_KILL_GOLD_MIN, CHAMPION_KILL_XP,
    CHAMPION_KILL_XP_MAX, CHAMPION_KILL_XP_MIN, ChampionRuntime,
};

pub(super) fn jungle_camp_reward(key: &str) -> Option<(i64, i64)> {
    match key {
        "blue-buff-blue" | "blue-buff-red" => Some((120, 180)),
        "red-buff-blue" | "red-buff-red" => Some((120, 185)),
        "wolves-blue" | "wolves-red" => Some((90, 135)),
        "raptors-blue" | "raptors-red" => Some((92, 140)),
        "gromp-blue" | "gromp-red" => Some((105, 156)),
        "krugs-blue" | "krugs-red" => Some((110, 162)),
        "scuttle-top" | "scuttle-bot" => Some((88, 132)),
        _ => None,
    }
}

pub(super) fn jungle_camp_cs_reward(key: &str) -> Option<i64> {
    match key {
        "blue-buff-blue" | "blue-buff-red" => Some(2),
        "red-buff-blue" | "red-buff-red" => Some(2),
        "wolves-blue" | "wolves-red" => Some(3),
        "raptors-blue" | "raptors-red" => Some(6),
        "gromp-blue" | "gromp-red" => Some(1),
        "krugs-blue" | "krugs-red" => Some(10),
        "scuttle-top" | "scuttle-bot" => Some(1),
        _ => None,
    }
}

pub(super) fn champion_kill_rewards(
    killer: &ChampionRuntime,
    victim: &ChampionRuntime,
) -> (i64, i64) {
    let level_gap = victim.level - killer.level;
    let victim_streak = (victim.kills as i64 - victim.deaths as i64).max(0);
    let killer_ahead = (killer.kills as i64 - killer.deaths as i64).max(0);
    let killer_kills = killer.kills.max(0) as i64;

    let mut gold = CHAMPION_KILL_GOLD + level_gap * 14 + victim_streak * 25;
    if killer_ahead >= 2 {
        gold -= ((killer_ahead - 1) * 24).min(160);
    }
    if killer_kills >= 6 {
        gold -= ((killer_kills - 5) * 10).min(70);
    }

    let mut xp = CHAMPION_KILL_XP + level_gap * 12 + victim_streak * 10;
    if killer_ahead >= 2 {
        xp -= ((killer_ahead - 1) * 12).min(96);
    }
    if killer_kills >= 6 {
        xp -= ((killer_kills - 5) * 6).min(42);
    }

    (
        gold.clamp(CHAMPION_KILL_GOLD_MIN, CHAMPION_KILL_GOLD_MAX),
        xp.clamp(CHAMPION_KILL_XP_MIN, CHAMPION_KILL_XP_MAX),
    )
}
