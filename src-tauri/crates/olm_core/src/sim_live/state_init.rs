use serde_json::{Map, Value, json};
use std::collections::HashMap;

use super::{
    LolChampionCombatProfileInput, LolChampionUltimateInput, RuntimeTeamBuffState,
    RuntimeTeamTactics, SimulatorAiMode, as_mut_object, create_champions, create_structures,
    extract_runtime_team_tactics, neutral_timer_templates,
};

pub(super) fn default_runtime_state() -> Value {
    json!({
        "timeSec": 0.0,
        "running": true,
        "speed": 1.0,
        "aiMode": "hybrid",
        "winner": Value::Null,
        "showWalls": false,
        "champions": [],
        "minions": [],
        "structures": [],
        "objectives": {
            "dragon": { "key": "dragon", "pos": { "x": 0.673828125, "y": 0.703125 }, "alive": false, "nextSpawnAt": 300.0 },
            "baron": { "key": "baron", "pos": { "x": 0.3274739583333333, "y": 0.2981770833333333 }, "alive": false, "nextSpawnAt": 1200.0 }
        },
        "neutralTimers": {
            "dragonSoulUnlocked": false,
            "elderUnlocked": false,
            "entities": {}
        },
        "stats": {
            "blue": { "kills": 0, "towers": 0, "dragons": 0, "barons": 0, "gold": 2500 },
            "red": { "kills": 0, "towers": 0, "dragons": 0, "barons": 0, "gold": 2500 }
        },
        "events": [{ "t": 0.0, "text": "Match started", "type": "info" }],
        "teamTactics": {
            "blue": RuntimeTeamTactics::default(),
            "red": RuntimeTeamTactics::default()
        },
        "teamBuffs": {
            "blue": RuntimeTeamBuffState::default(),
            "red": RuntimeTeamBuffState::default()
        }
    })
}

pub(super) fn ensure_runtime_state_defaults(state: &mut Value) {
    let Ok(root) = as_mut_object(state) else {
        *state = default_runtime_state();
        return;
    };

    if !root.contains_key("timeSec") {
        root.insert("timeSec".to_string(), json!(0.0));
    }
    if !root.contains_key("running") {
        root.insert("running".to_string(), json!(true));
    }
    if !root.contains_key("speed") {
        root.insert("speed".to_string(), json!(1.0));
    }
    if !root.contains_key("aiMode") {
        root.insert("aiMode".to_string(), json!("hybrid"));
    }
    if !root.contains_key("winner") {
        root.insert("winner".to_string(), Value::Null);
    }
    if !root.contains_key("showWalls") {
        root.insert("showWalls".to_string(), json!(false));
    }
    if !root.contains_key("champions") {
        root.insert("champions".to_string(), Value::Array(Vec::new()));
    }
    if !root.contains_key("minions") {
        root.insert("minions".to_string(), Value::Array(Vec::new()));
    }
    if !root.contains_key("structures") {
        root.insert("structures".to_string(), Value::Array(Vec::new()));
    }
    if !root.contains_key("objectives") {
        root.insert(
            "objectives".to_string(),
            json!({
                "dragon": { "key": "dragon", "pos": { "x": 0.673828125, "y": 0.703125 }, "alive": false, "nextSpawnAt": 300.0 },
                "baron": { "key": "baron", "pos": { "x": 0.3274739583333333, "y": 0.2981770833333333 }, "alive": false, "nextSpawnAt": 1200.0 }
            }),
        );
    }
    if !root.contains_key("neutralTimers") {
        root.insert(
            "neutralTimers".to_string(),
            json!({ "dragonSoulUnlocked": false, "elderUnlocked": false, "entities": {} }),
        );
    }
    if !root.contains_key("stats") {
        root.insert(
            "stats".to_string(),
            json!({
                "blue": { "kills": 0, "towers": 0, "dragons": 0, "barons": 0, "gold": 2500 },
                "red": { "kills": 0, "towers": 0, "dragons": 0, "barons": 0, "gold": 2500 }
            }),
        );
    }
    if !root.contains_key("events") {
        root.insert(
            "events".to_string(),
            Value::Array(vec![
                json!({ "t": 0.0, "text": "Match started", "type": "info" }),
            ]),
        );
    }
    if !root.contains_key("teamTactics") {
        root.insert(
            "teamTactics".to_string(),
            json!({
                "blue": RuntimeTeamTactics::default(),
                "red": RuntimeTeamTactics::default(),
            }),
        );
    }
    if !root.contains_key("teamBuffs") {
        root.insert(
            "teamBuffs".to_string(),
            json!({
                "blue": RuntimeTeamBuffState::default(),
                "red": RuntimeTeamBuffState::default(),
            }),
        );
    }
}

pub(super) fn create_initial_state(
    seed: &str,
    snapshot: &Value,
    champion_by_player_id: &HashMap<String, String>,
    champion_profiles_by_id: &HashMap<String, LolChampionCombatProfileInput>,
    champion_ultimates_by_id: &HashMap<String, LolChampionUltimateInput>,
    ai_mode: SimulatorAiMode,
) -> Value {
    let champions = create_champions(
        seed,
        snapshot,
        champion_by_player_id,
        champion_profiles_by_id,
        champion_ultimates_by_id,
    );
    let structures = create_structures();
    let neutral_timers = build_neutral_timers_state();
    let team_tactics = build_team_tactics_state(snapshot);

    json!({
        "timeSec": 0.0,
        "running": true,
        "speed": 1.0,
        "aiMode": ai_mode.as_str(),
        "winner": Value::Null,
        "champions": champions,
        "minions": [],
        "structures": structures,
        "wards": [],
        "objectives": {
            "dragon": { "key": "dragon", "pos": { "x": 0.673828125, "y": 0.703125 }, "alive": false, "nextSpawnAt": 5.0 * 60.0 },
            "baron": { "key": "baron", "pos": { "x": 0.3274739583333333, "y": 0.2981770833333333 }, "alive": false, "nextSpawnAt": 20.0 * 60.0 }
        },
        "neutralTimers": neutral_timers,
        "stats": {
            "blue": { "kills": 0, "towers": 0, "dragons": 0, "barons": 0, "gold": 2500 },
            "red": { "kills": 0, "towers": 0, "dragons": 0, "barons": 0, "gold": 2500 }
        },
        "events": [{ "t": 0.0, "text": "Match started", "type": "info" }],
        "teamTactics": team_tactics,
        "showWalls": false,
    })
}

fn build_team_tactics_state(snapshot: &Value) -> Value {
    let blue = extract_runtime_team_tactics(snapshot, "home", "home_team");
    let red = extract_runtime_team_tactics(snapshot, "away", "away_team");
    json!({ "blue": blue, "red": red })
}

pub(super) fn build_neutral_timers_state() -> Value {
    let mut entities = Map::new();

    for timer in neutral_timer_templates() {
        entities.insert(
            timer.key.to_string(),
            json!({
                "key": timer.key,
                "label": timer.label,
                "alive": false,
                "hp": timer.max_hp,
                "maxHp": timer.max_hp,
                "nextSpawnAt": if timer.unlocked { Value::from(timer.first_spawn_at) } else { Value::Null },
                "firstSpawnAt": timer.first_spawn_at,
                "respawnDelaySec": match timer.respawn_delay_sec {
                    Some(value) => Value::from(value),
                    None => Value::Null,
                },
                "oneShot": timer.one_shot,
                "windowCloseAt": match timer.window_close_at {
                    Some(value) => Value::from(value),
                    None => Value::Null,
                },
                "combatGraceUntil": match timer.combat_grace_until {
                    Some(value) => Value::from(value),
                    None => Value::Null,
                },
                "unlocked": timer.unlocked,
                "lastSpawnAt": Value::Null,
                "lastTakenAt": Value::Null,
                "timesSpawned": 0,
                "timesTaken": 0,
                "pos": { "x": timer.pos.x, "y": timer.pos.y },
            }),
        );
    }

    json!({
        "dragonSoulUnlocked": false,
        "elderUnlocked": false,
        "entities": Value::Object(entities),
    })
}
