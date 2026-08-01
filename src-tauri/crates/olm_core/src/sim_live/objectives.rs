use serde_json::Value;

use super::{
    NeutralTimerRuntime, NeutralTimersRuntime, OBJECTIVE_NEXT_SPAWN_FALLBACK, RuntimeState,
    add_dragon_stack_for_kind, decode_neutral_timers_state, log_event,
    neutral_timers_default_runtime_state, runtime_buffs_from_extra, set_runtime_buffs,
    team_buffs_mut, team_buffs_ref,
};

pub(super) struct NeutralTimerTickResult {
    pub(super) spawn_text: Option<String>,
    pub(super) despawn_text: Option<String>,
    pub(super) voidgrubs_expired_with_remaining_hp: bool,
}

pub(super) struct VoidgrubExpirationInput {
    pub(super) blue_stacks: i64,
    pub(super) red_stacks: i64,
}

pub(super) struct VoidgrubExpirationEffect {
    pub(super) winner_team: &'static str,
    pub(super) stacks_to_award: i64,
}

pub(super) enum NeutralCaptureKind {
    Dragon,
    Baron,
    Elder,
    Herald,
    Voidgrubs,
    OtherObjective,
}

pub(super) struct NeutralCaptureDecision {
    pub(super) kind: NeutralCaptureKind,
    pub(super) event_type: &'static str,
}

pub(super) fn resolve_neutral_capture_decision(key: &str) -> Option<NeutralCaptureDecision> {
    let kind = match key {
        "dragon" => NeutralCaptureKind::Dragon,
        "baron" => NeutralCaptureKind::Baron,
        "elder" => NeutralCaptureKind::Elder,
        "herald" => NeutralCaptureKind::Herald,
        "voidgrubs" => NeutralCaptureKind::Voidgrubs,
        "scuttle-top" | "scuttle-bot" => NeutralCaptureKind::OtherObjective,
        _ => return None,
    };

    let event_type = match kind {
        NeutralCaptureKind::Dragon | NeutralCaptureKind::Elder => "dragon",
        NeutralCaptureKind::Baron => "baron",
        _ => "info",
    };

    Some(NeutralCaptureDecision { kind, event_type })
}

pub(super) fn resolve_voidgrub_expiration_effect(
    expired_with_remaining_hp: bool,
    stacks: VoidgrubExpirationInput,
) -> Option<VoidgrubExpirationEffect> {
    if !expired_with_remaining_hp {
        return None;
    }

    let total = (stacks.blue_stacks + stacks.red_stacks).clamp(0, 3);
    let remaining = (3 - total).max(0);
    if remaining <= 0 {
        return None;
    }

    let winner_team = if stacks.red_stacks > stacks.blue_stacks {
        "red"
    } else {
        "blue"
    };

    Some(VoidgrubExpirationEffect {
        winner_team,
        stacks_to_award: remaining,
    })
}

pub(super) fn current_dragon_kind(neutral_timers: &NeutralTimersRuntime) -> String {
    let raw = neutral_timers
        .extra
        .get("dragonCurrentKind")
        .and_then(Value::as_str)
        .unwrap_or("infernal")
        .trim()
        .to_lowercase();

    match raw.as_str() {
        "infernal" | "ocean" | "mountain" | "cloud" | "hextech" | "chemtech" => raw,
        _ => "infernal".to_string(),
    }
}

pub(super) fn set_current_dragon_kind(neutral_timers: &mut NeutralTimersRuntime, kind: &str) {
    neutral_timers
        .extra
        .insert("dragonCurrentKind".to_string(), Value::from(kind));
}

pub(super) fn choose_different_dragon_kind(base_kind: &str, seed: i64) -> &'static str {
    const KINDS: [&str; 6] = [
        "infernal", "ocean", "mountain", "cloud", "hextech", "chemtech",
    ];
    let mut options: Vec<&str> = KINDS
        .into_iter()
        .filter(|kind| *kind != base_kind)
        .collect();
    if options.is_empty() {
        return "infernal";
    }
    let idx = (seed.unsigned_abs() as usize) % options.len();
    options.swap_remove(idx)
}

pub(super) fn choose_dragon_kind_excluding(excluded: &[&str], seed: i64) -> &'static str {
    const KINDS: [&str; 6] = [
        "infernal", "ocean", "mountain", "cloud", "hextech", "chemtech",
    ];
    let mut options: Vec<&str> = KINDS
        .into_iter()
        .filter(|kind| !excluded.iter().any(|excluded_kind| excluded_kind == kind))
        .collect();
    if options.is_empty() {
        return "infernal";
    }
    let idx = (seed.unsigned_abs() as usize) % options.len();
    options.swap_remove(idx)
}

pub(super) fn ensure_dragon_cycle_defaults(
    champion_ids: impl Iterator<Item = String>,
    neutral_timers: &mut NeutralTimersRuntime,
) {
    if neutral_timers.extra.get("dragonCurrentKind").is_some() {
        return;
    }
    let seed = champion_ids.fold(0_i64, |acc, id| {
        acc + id.bytes().fold(0_i64, |s, b| s + b as i64)
    });
    let first = choose_different_dragon_kind("", seed);
    set_current_dragon_kind(neutral_timers, first);
    neutral_timers
        .extra
        .insert("dragonFirstKind".to_string(), Value::from(""));
    neutral_timers
        .extra
        .insert("dragonSecondKind".to_string(), Value::from(""));
    neutral_timers
        .extra
        .insert("dragonSoulRiftKind".to_string(), Value::from(""));
}

pub(super) fn sync_objectives_from_neutral_timers(
    runtime: &mut RuntimeState,
    neutral_timers: &NeutralTimersRuntime,
) {
    let Some(objectives) = runtime.objectives.as_object_mut() else {
        return;
    };

    let buffs = runtime_buffs_from_extra(runtime.extra.get("teamBuffs"));

    if let Some(dragon_timer) = neutral_timers.entities.get("dragon") {
        sync_dragon_objective(
            objectives,
            neutral_timers,
            dragon_timer,
            buffs.blue.dragon_stacks,
            buffs.red.dragon_stacks,
            buffs.blue.soul_kind.is_some(),
            buffs.red.soul_kind.is_some(),
        );
    }

    if let Some(baron_timer) = neutral_timers.entities.get("baron") {
        sync_baron_objective(objectives, baron_timer);
    }
}

pub(super) fn sync_dragon_timer_kind(neutral_timers: &mut NeutralTimersRuntime) {
    let dragon_kind = current_dragon_kind(neutral_timers);
    if let Some(dragon_timer) = neutral_timers.entities.get_mut("dragon") {
        dragon_timer
            .extra
            .insert("dragonCurrentKind".to_string(), Value::from(dragon_kind));
    }
}

pub(super) fn unlock_elder_if_needed(neutral_timers: &mut NeutralTimersRuntime, now: f64) {
    if !neutral_timers.elder_unlocked {
        return;
    }

    if let Some(elder) = neutral_timers.entities.get_mut("elder") {
        if !elder.unlocked {
            elder.unlocked = true;
            elder.next_spawn_at = Some(now + 6.0 * 60.0);
        }
    }
}

pub(super) fn tick_neutral_entity_timer(
    neutral_timers: &mut NeutralTimersRuntime,
    key: &str,
    now: f64,
) -> NeutralTimerTickResult {
    let mut spawn_text: Option<String> = None;
    let mut despawn_text: Option<String> = None;
    let mut voidgrubs_expired_with_remaining_hp = false;

    if let Some(timer) = neutral_timers.entities.get_mut(key) {
        let can_spawn = timer.unlocked
            && !timer.alive
            && timer.next_spawn_at.is_some()
            && now >= timer.next_spawn_at.unwrap_or(f64::INFINITY);
        if can_spawn {
            timer.alive = true;
            timer.hp = timer.max_hp;
            timer.last_spawn_at = timer.next_spawn_at;
            timer.times_spawned += 1;
            spawn_text = Some(format!("{} spawned", timer.label));
        }

        if timer.alive {
            if let Some(grace_until) = timer.combat_grace_until {
                if now >= grace_until {
                    let had_remaining_hp = timer.hp > 0.0;
                    timer.alive = false;
                    timer.hp = 0.0;
                    timer.next_spawn_at = None;
                    despawn_text = Some(format!("{} despawned", timer.label));

                    if key == "voidgrubs" && had_remaining_hp {
                        voidgrubs_expired_with_remaining_hp = true;
                    }
                }
            }
        }
    }

    NeutralTimerTickResult {
        spawn_text,
        despawn_text,
        voidgrubs_expired_with_remaining_hp,
    }
}

pub(super) fn tick_neutral_timers(runtime: &mut RuntimeState) {
    let mut neutral_timers = decode_neutral_timers_state(&runtime.neutral_timers)
        .unwrap_or_else(neutral_timers_default_runtime_state);
    let now = runtime.time_sec;

    ensure_dragon_cycle_defaults(
        runtime.champions.iter().map(|champion| champion.id.clone()),
        &mut neutral_timers,
    );

    sync_dragon_timer_kind(&mut neutral_timers);
    unlock_elder_if_needed(&mut neutral_timers, now);

    let mut keys: Vec<String> = neutral_timers.entities.keys().cloned().collect();
    keys.sort();

    for key in keys {
        let timer_tick = tick_neutral_entity_timer(&mut neutral_timers, &key, now);

        let mut buffs = runtime_buffs_from_extra(runtime.extra.get("teamBuffs"));
        if let Some(effect) = resolve_voidgrub_expiration_effect(
            timer_tick.voidgrubs_expired_with_remaining_hp,
            VoidgrubExpirationInput {
                blue_stacks: buffs.blue.voidgrub_stacks,
                red_stacks: buffs.red.voidgrub_stacks,
            },
        ) {
            let target = team_buffs_mut(&mut buffs, effect.winner_team);
            target.voidgrub_stacks = (target.voidgrub_stacks + effect.stacks_to_award).clamp(0, 3);
            set_runtime_buffs(runtime, &buffs);
        }

        if let Some(text) = timer_tick.spawn_text {
            log_event(runtime, &text, "spawn");
        }
        if let Some(text) = timer_tick.despawn_text {
            log_event(runtime, &text, "info");
        }
    }

    sync_objectives_from_neutral_timers(runtime, &neutral_timers);
    if let Ok(value) = serde_json::to_value(&neutral_timers) {
        runtime.neutral_timers = value;
    }
}

pub(super) fn process_dragon_capture(
    runtime: &mut RuntimeState,
    neutral_timers: &mut NeutralTimersRuntime,
    killer_team: &str,
) -> String {
    ensure_dragon_cycle_defaults(
        runtime.champions.iter().map(|champion| champion.id.clone()),
        neutral_timers,
    );
    let dragon_kind = current_dragon_kind(neutral_timers);

    let mut buffs = runtime_buffs_from_extra(runtime.extra.get("teamBuffs"));
    {
        let team_buffs = team_buffs_mut(&mut buffs, killer_team);
        add_dragon_stack_for_kind(team_buffs, &dragon_kind);
        if team_buffs.dragon_history.len() >= 8 {
            team_buffs.dragon_history.remove(0);
        }
        team_buffs.dragon_history.push(dragon_kind.clone());
    }

    let total_dragons = buffs.blue.dragon_stacks + buffs.red.dragon_stacks;

    if total_dragons == 1 {
        neutral_timers.extra.insert(
            "dragonFirstKind".to_string(),
            Value::from(dragon_kind.as_str()),
        );
        let second_kind = choose_different_dragon_kind(
            &dragon_kind,
            runtime.time_sec as i64 + runtime.events.len() as i64,
        );
        set_current_dragon_kind(neutral_timers, second_kind);
    } else if total_dragons == 2 {
        let first_kind = neutral_timers
            .extra
            .get("dragonFirstKind")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("")
            .to_string();
        neutral_timers.extra.insert(
            "dragonSecondKind".to_string(),
            Value::from(dragon_kind.as_str()),
        );
        let rift_kind = choose_dragon_kind_excluding(
            &[first_kind.as_str(), dragon_kind.as_str()],
            runtime.time_sec as i64 + runtime.events.len() as i64 + 37,
        );
        neutral_timers
            .extra
            .insert("dragonSoulRiftKind".to_string(), Value::from(rift_kind));
        set_current_dragon_kind(neutral_timers, rift_kind);
    }

    let soul_rift_kind = neutral_timers
        .extra
        .get("dragonSoulRiftKind")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(dragon_kind.as_str())
        .to_string();

    let team_dragons = team_buffs_ref(&buffs, killer_team).dragon_stacks;
    let soul_missing = team_buffs_ref(&buffs, killer_team).soul_kind.is_none();

    if team_dragons >= 4 && soul_missing {
        team_buffs_mut(&mut buffs, killer_team).soul_kind = Some(soul_rift_kind.clone());
        neutral_timers.dragon_soul_unlocked = true;
        neutral_timers.elder_unlocked = true;

        if let Some(dragon) = neutral_timers.entities.get_mut("dragon") {
            dragon.alive = false;
            dragon.hp = 0.0;
            dragon.unlocked = false;
            dragon.next_spawn_at = None;
        }
        if let Some(elder) = neutral_timers.entities.get_mut("elder") {
            elder.unlocked = true;
            elder.next_spawn_at = Some(runtime.time_sec + 6.0 * 60.0);
        }
    } else if total_dragons != 1 {
        set_current_dragon_kind(neutral_timers, &soul_rift_kind);
    }

    set_runtime_buffs(runtime, &buffs);
    dragon_kind
}

fn sync_dragon_objective(
    objectives: &mut serde_json::Map<String, Value>,
    neutral_timers: &NeutralTimersRuntime,
    dragon_timer: &NeutralTimerRuntime,
    blue_dragon_stacks: i64,
    red_dragon_stacks: i64,
    blue_has_soul: bool,
    red_has_soul: bool,
) {
    let Some(dragon_obj) = objectives.get_mut("dragon").and_then(Value::as_object_mut) else {
        return;
    };

    dragon_obj.insert("alive".to_string(), Value::from(dragon_timer.alive));
    dragon_obj.insert(
        "nextSpawnAt".to_string(),
        Value::from(next_spawn_or_fallback(dragon_timer)),
    );
    dragon_obj.insert(
        "currentKind".to_string(),
        Value::from(current_dragon_kind(neutral_timers)),
    );
    dragon_obj.insert(
        "firstKind".to_string(),
        neutral_timers
            .extra
            .get("dragonFirstKind")
            .cloned()
            .unwrap_or(Value::from("")),
    );
    dragon_obj.insert(
        "secondKind".to_string(),
        neutral_timers
            .extra
            .get("dragonSecondKind")
            .cloned()
            .unwrap_or(Value::from("")),
    );
    dragon_obj.insert(
        "soulRiftKind".to_string(),
        neutral_timers
            .extra
            .get("dragonSoulRiftKind")
            .cloned()
            .unwrap_or(Value::from("")),
    );
    dragon_obj.insert("homeStacks".to_string(), Value::from(blue_dragon_stacks));
    dragon_obj.insert("awayStacks".to_string(), Value::from(red_dragon_stacks));
    dragon_obj.insert(
        "soulClaimedBy".to_string(),
        if blue_has_soul {
            Value::from("Home")
        } else if red_has_soul {
            Value::from("Away")
        } else {
            Value::Null
        },
    );
}

fn sync_baron_objective(
    objectives: &mut serde_json::Map<String, Value>,
    baron_timer: &NeutralTimerRuntime,
) {
    let Some(baron_obj) = objectives.get_mut("baron").and_then(Value::as_object_mut) else {
        return;
    };
    baron_obj.insert("alive".to_string(), Value::from(baron_timer.alive));
    baron_obj.insert(
        "nextSpawnAt".to_string(),
        Value::from(next_spawn_or_fallback(baron_timer)),
    );
}

fn next_spawn_or_fallback(timer: &NeutralTimerRuntime) -> f64 {
    timer.next_spawn_at.unwrap_or(OBJECTIVE_NEXT_SPAWN_FALLBACK)
}
