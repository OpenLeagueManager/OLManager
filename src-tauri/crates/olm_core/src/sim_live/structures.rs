use serde_json::{Value, json};

use std::cmp::Ordering;

use super::{
    ChampionRuntime, INHIBITOR_HP, MinionRuntime, NEXUS_HP, STRUCTURE_LAYOUT, StructureRuntime,
    StructureSeed, TOWER_INHIB_HP, TOWER_INNER_HP, TOWER_NEXUS_HP, TOWER_OUTER_HP, Vec2,
    champion_is_banished, dist, normalized_team,
};

pub(super) enum StructureAttackTarget {
    Minion(usize),
    Champion(usize),
}

pub(super) fn create_structures() -> Vec<Value> {
    STRUCTURE_LAYOUT
        .iter()
        .map(|s| {
            let hp = structure_base_hp(s);
            json!({
                "id": s.id,
                "team": s.team,
                "lane": s.lane,
                "kind": s.kind,
                "pos": { "x": s.pos.x, "y": s.pos.y },
                "hp": hp,
                "maxHp": hp,
                "alive": true,
                "attackCdUntil": 0.0,
                "forcedTargetChampionId": Value::Null,
                "forcedTargetUntil": 0.0,
            })
        })
        .collect()
}

fn structure_base_hp(seed: &StructureSeed) -> f64 {
    match seed.kind {
        "nexus" => NEXUS_HP,
        "inhib" => INHIBITOR_HP,
        "tower" => {
            if seed.id.contains("nexus") {
                TOWER_NEXUS_HP
            } else if seed.id.contains("inhib") {
                TOWER_INHIB_HP
            } else if seed.id.contains("inner") {
                TOWER_INNER_HP
            } else {
                TOWER_OUTER_HP
            }
        }
        _ => TOWER_OUTER_HP,
    }
}

pub(super) fn is_structure_targetable(
    structures: &[StructureRuntime],
    attacker_team: &str,
    structure: &StructureRuntime,
) -> bool {
    if !structure.alive || normalized_team(&structure.team) == normalized_team(attacker_team) {
        return false;
    }

    if structure.kind == "nexus" {
        return !team_has_alive_nexus_towers(structures, &structure.team);
    }

    if structure.kind == "tower" {
        if let Some(prereq_alive) = prerequisite_tower_alive(structures, &structure.id) {
            if prereq_alive {
                return false;
            }
        }
    }

    if structure.kind == "inhib" {
        if let Some(lane) = lane_tag_from_structure_id(&structure.id) {
            return !inhib_tower_alive_for_lane(structures, &structure.team, lane);
        }
    }

    true
}

pub(super) fn nearest_enemy_minion_for_structure(
    minions: &[MinionRuntime],
    structure_team: &str,
    from: Vec2,
    range: f64,
) -> Option<usize> {
    minions
        .iter()
        .enumerate()
        .filter(|(_, minion)| {
            minion.alive
                && normalized_team(&minion.team) != normalized_team(structure_team)
                && dist(minion.pos, from) <= range
        })
        .min_by(|(idx_a, a), (idx_b, b)| {
            dist(a.pos, from)
                .partial_cmp(&dist(b.pos, from))
                .unwrap_or(Ordering::Equal)
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx)
}

pub(super) fn nearest_enemy_champion_for_structure(
    champions: &[ChampionRuntime],
    structure_team: &str,
    from: Vec2,
    range: f64,
) -> Option<usize> {
    champions
        .iter()
        .enumerate()
        .filter(|(_, champion)| {
            champion.alive
                && !champion_is_banished(champion)
                && normalized_team(&champion.team) != normalized_team(structure_team)
                && dist(champion.pos, from) <= range
        })
        .min_by(|(idx_a, a), (idx_b, b)| {
            dist(a.pos, from)
                .partial_cmp(&dist(b.pos, from))
                .unwrap_or(Ordering::Equal)
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx)
}

pub(super) fn locked_tower_target_champion_idx(
    champions: &[ChampionRuntime],
    target_id: &str,
    structure_team: &str,
    structure_pos: Vec2,
    range: f64,
) -> Option<usize> {
    champions
        .iter()
        .enumerate()
        .find(|(_, champion)| {
            champion.alive
                && !champion_is_banished(champion)
                && champion.id == target_id
                && normalized_team(&champion.team) != normalized_team(structure_team)
                && dist(champion.pos, structure_pos) <= range
        })
        .map(|(champion_idx, _)| champion_idx)
}

pub(super) fn compute_tower_minion_shot_damage(
    base_damage: f64,
    is_target_baron_empowered: bool,
    baron_damage_reduction: f64,
) -> f64 {
    if is_target_baron_empowered {
        base_damage * (1.0 - baron_damage_reduction)
    } else {
        base_damage
    }
}

pub(super) fn structure_attack_cd_until(now: f64, cadence_sec: f64) -> f64 {
    now + cadence_sec
}

pub(super) fn select_structure_attack_target(
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structure: &StructureRuntime,
    now: f64,
    range: f64,
) -> (Option<StructureAttackTarget>, bool) {
    if let Some(target_id) = structure.forced_target_champion_id.as_deref() {
        if now <= structure.forced_target_until {
            if let Some(champion_idx) = locked_tower_target_champion_idx(
                champions,
                target_id,
                &structure.team,
                structure.pos,
                range,
            ) {
                return (Some(StructureAttackTarget::Champion(champion_idx)), false);
            }
        }
        // forced target existed but is no longer valid/active
        if now > structure.forced_target_until
            || locked_tower_target_champion_idx(
                champions,
                target_id,
                &structure.team,
                structure.pos,
                range,
            )
            .is_none()
        {
            if let Some(minion_idx) =
                nearest_enemy_minion_for_structure(minions, &structure.team, structure.pos, range)
            {
                return (Some(StructureAttackTarget::Minion(minion_idx)), true);
            }
            if let Some(champion_idx) = nearest_enemy_champion_for_structure(
                champions,
                &structure.team,
                structure.pos,
                range,
            ) {
                return (Some(StructureAttackTarget::Champion(champion_idx)), true);
            }
            return (None, true);
        }
    }

    if let Some(minion_idx) =
        nearest_enemy_minion_for_structure(minions, &structure.team, structure.pos, range)
    {
        return (Some(StructureAttackTarget::Minion(minion_idx)), false);
    }

    if let Some(champion_idx) =
        nearest_enemy_champion_for_structure(champions, &structure.team, structure.pos, range)
    {
        return (Some(StructureAttackTarget::Champion(champion_idx)), false);
    }

    (None, false)
}

pub(super) fn resolve_structure_combat(runtime: &mut super::RuntimeState) {
    let now = runtime.time_sec;

    for idx in 0..runtime.structures.len() {
        if !runtime.structures[idx].alive
            || runtime.structures[idx].kind != "tower"
            || now < runtime.structures[idx].attack_cd_until
        {
            continue;
        }

        let (target, should_clear_forced_target) = select_structure_attack_target(
            &runtime.champions,
            &runtime.minions,
            &runtime.structures[idx],
            now,
            super::TOWER_ATTACK_RANGE,
        );

        if should_clear_forced_target {
            runtime.structures[idx].forced_target_champion_id = None;
            runtime.structures[idx].forced_target_until = 0.0;
        }

        if let Some(target) = target {
            match target {
                StructureAttackTarget::Minion(minion_idx) => {
                    let incoming = compute_tower_minion_shot_damage(
                        super::TOWER_SHOT_DAMAGE_TO_MINION,
                        super::minion_is_baron_empowered(runtime, &runtime.minions[minion_idx]),
                        super::BARON_MINION_DAMAGE_REDUCTION,
                    );
                    runtime.minions[minion_idx].hp -= incoming;
                    runtime.structures[idx].attack_cd_until =
                        structure_attack_cd_until(now, super::TOWER_ATTACK_CADENCE_SEC);
                    if runtime.minions[minion_idx].hp <= 0.0 {
                        super::register_minion_death(runtime, minion_idx);
                    }
                }
                StructureAttackTarget::Champion(champion_idx) => {
                    apply_tower_shot_to_champion(runtime, idx, champion_idx);
                }
            }
        }
    }
}

pub(super) fn apply_tower_shot_to_champion(
    runtime: &mut super::RuntimeState,
    structure_idx: usize,
    champion_idx: usize,
) {
    let now = runtime.time_sec;
    runtime.champions[champion_idx].hp -= super::TOWER_SHOT_DAMAGE;
    runtime.champions[champion_idx].last_damaged_at = now;
    super::cancel_recall(
        &mut runtime.champions[champion_idx],
        now,
        &mut runtime.events,
    );
    runtime.structures[structure_idx].attack_cd_until =
        structure_attack_cd_until(now, super::TOWER_ATTACK_CADENCE_SEC);
    if runtime.champions[champion_idx].hp <= 0.0 && runtime.champions[champion_idx].alive {
        runtime.champions[champion_idx].alive = false;
        runtime.champions[champion_idx].hp = 0.0;
        runtime.champions[champion_idx].deaths += 1;
        let respawn = super::champion_respawn_seconds(runtime.champions[champion_idx].level, now);
        runtime.champions[champion_idx].respawn_at = now + respawn;
        super::award_recent_champion_kill_credit(runtime, champion_idx, now, "tower");
    }
}

pub(super) fn apply_damage_to_structure(
    runtime: &mut super::RuntimeState,
    structure_idx: usize,
    raw_damage: f64,
    attacker_team: &str,
) {
    if structure_idx >= runtime.structures.len() {
        return;
    }
    if !is_structure_targetable(
        &runtime.structures,
        attacker_team,
        &runtime.structures[structure_idx],
    ) {
        return;
    }

    let multiplier =
        super::tower_damage_multiplier(runtime.time_sec, &runtime.structures[structure_idx]);
    let mut damage = raw_damage.max(0.0) * multiplier;
    if runtime.structures[structure_idx].kind == "tower"
        && runtime.time_sec >= super::EARLY_TOWER_FORTIFICATION_END_AT
    {
        let buffs = super::team_buffs_for_runtime(runtime.extra.get("teamBuffs"), attacker_team);
        let voidgrub_bonus = (buffs.voidgrub_stacks as f64
            * super::VOIDGRUB_TOWER_DAMAGE_PER_STACK)
            .min(super::VOIDGRUB_TOWER_DAMAGE_MAX)
            .max(0.0);
        damage *= 1.0 + voidgrub_bonus;
    }
    if damage <= 0.0 {
        return;
    }

    runtime.structures[structure_idx].hp -= damage;
    if runtime.structures[structure_idx].hp <= 0.0 {
        super::destroy_structure(runtime, structure_idx, attacker_team);
    }
}

fn team_has_alive_nexus_towers(structures: &[StructureRuntime], team: &str) -> bool {
    structures.iter().any(|structure| {
        structure.alive
            && normalized_team(&structure.team) == normalized_team(team)
            && structure.kind == "tower"
            && structure.id.contains("nexus")
    })
}

fn lane_tag_from_structure_id(id: &str) -> Option<&'static str> {
    if id.contains("-top") {
        Some("top")
    } else if id.contains("-mid") {
        Some("mid")
    } else if id.contains("-bot") {
        Some("bot")
    } else {
        None
    }
}

fn inhib_tower_alive_for_lane(
    structures: &[StructureRuntime],
    defending_team: &str,
    lane: &str,
) -> bool {
    structures.iter().any(|candidate| {
        candidate.alive
            && candidate.kind == "tower"
            && normalized_team(&candidate.team) == normalized_team(defending_team)
            && candidate.id.contains("inhib-tower")
            && candidate.id.contains(lane)
    })
}

fn structure_alive_by_id(structures: &[StructureRuntime], id: &str) -> bool {
    structures
        .iter()
        .any(|structure| structure.alive && structure.id == id)
}

fn prerequisite_tower_alive(structures: &[StructureRuntime], structure_id: &str) -> Option<bool> {
    if structure_id.contains("-inner") {
        let prerequisite = structure_id.replace("-inner", "-outer");
        return Some(structure_alive_by_id(structures, &prerequisite));
    }
    if structure_id.contains("-inhib-tower") {
        let prerequisite = structure_id.replace("-inhib-tower", "-inner");
        return Some(structure_alive_by_id(structures, &prerequisite));
    }
    if structure_id.contains("-nexus-top-tower") {
        let prerequisite = structure_id.replace("-nexus-top-tower", "-inhib-top");
        return Some(structure_alive_by_id(structures, &prerequisite));
    }
    if structure_id.contains("-nexus-bot-tower") {
        let prerequisite = structure_id.replace("-nexus-bot-tower", "-inhib-bot");
        return Some(structure_alive_by_id(structures, &prerequisite));
    }
    None
}
