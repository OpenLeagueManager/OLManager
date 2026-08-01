use super::*;

pub(super) fn spawn_waves_if_due(runtime: &mut RuntimeState, session: &mut SimLiveSession) {
    while runtime.time_sec >= session.wave_spawn_at {
        spawn_wave(runtime, session);
        session.wave_spawn_at += wave_interval_sec(session.wave_spawn_at);
    }
}

pub(super) fn spawn_wave(runtime: &mut RuntimeState, session: &mut SimLiveSession) {
    for lane in ["top", "mid", "bot"] {
        for i in 0..3 {
            runtime
                .minions
                .push(build_minion(session, "blue", lane, "melee", i));
            runtime
                .minions
                .push(build_minion(session, "red", lane, "melee", i));
        }
        for i in 0..3 {
            runtime
                .minions
                .push(build_minion(session, "blue", lane, "ranged", i));
            runtime
                .minions
                .push(build_minion(session, "red", lane, "ranged", i));
        }
    }

    log_event(runtime, "Minion wave spawned", "spawn");
}

pub(super) fn build_minion(
    session: &mut SimLiveSession,
    team: &str,
    lane: &str,
    kind: &str,
    slot: i32,
) -> MinionRuntime {
    let path = lane_path_for(team, lane);
    let (move_speed, attack_range, attack_damage, _) = minion_stats(kind);
    let max_hp = if kind == "ranged" {
        MINION_RANGED_MAX_HP
    } else {
        MINION_MELEE_MAX_HP
    };

    let id = format!("m-{}", session.next_minion_id);
    session.next_minion_id += 1;

    MinionRuntime {
        id,
        team: team.to_string(),
        lane: normalized_lane(lane).to_string(),
        pos: spawn_formation_position(&path, kind, slot),
        hp: max_hp,
        max_hp,
        alive: true,
        kind: kind.to_string(),
        last_hit_by_champion_id: None,
        owner_champion_id: None,
        summon_kind: None,
        summon_expires_at: 0.0,
        attack_cd_until: 0.0,
        move_speed,
        attack_range,
        attack_damage,
        path,
        path_index: 1,
    }
}
