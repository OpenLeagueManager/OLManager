use std::cmp::Ordering;

use super::{
    ChampionRuntime, FIRST_WAVE_CONTEST_UNTIL, LANE_EMPTY_ANCHOR_PROGRESS_MAX_INDEX, LanePressure,
    LaneRoleProfile, MINION_FIRST_WAVE_AT, MinionRuntime, RuntimeState, StructureRuntime, Vec2,
    base_position_for, clamp, closest_lane_path_index, dist, lane_path_for, normalize,
    normalized_lane, normalized_team,
};

pub(super) fn lane_role_profile(champion: &ChampionRuntime) -> Option<LaneRoleProfile> {
    if champion.role == "JGL" {
        return None;
    }
    match champion.role.as_str() {
        "TOP" => Some(LaneRoleProfile {
            chase_leash: 0.11,
            approach_leash: 0.062,
            retreat_hp: 0.27,
            outnumber_tolerance: 0.25,
        }),
        "MID" => Some(LaneRoleProfile {
            chase_leash: 0.10,
            approach_leash: 0.058,
            retreat_hp: 0.28,
            outnumber_tolerance: 0.20,
        }),
        "ADC" => Some(LaneRoleProfile {
            chase_leash: 0.095,
            approach_leash: 0.058,
            retreat_hp: 0.35,
            outnumber_tolerance: 0.08,
        }),
        _ => Some(LaneRoleProfile {
            chase_leash: 0.09,
            approach_leash: 0.055,
            retreat_hp: 0.33,
            outnumber_tolerance: 0.08,
        }),
    }
}

pub(super) fn is_first_wave_contest_active(champion: &ChampionRuntime, now: f64) -> bool {
    if champion.role == "JGL" {
        return false;
    }
    now >= MINION_FIRST_WAVE_AT && now <= FIRST_WAVE_CONTEST_UNTIL
}

pub(super) fn choose_lane_anchor_index(
    champion: &ChampionRuntime,
    minions: &[MinionRuntime],
    _structures: &[StructureRuntime],
) -> usize {
    let lane_path = lane_path_for(&champion.team, &champion.lane);
    if lane_path.is_empty() {
        return 0;
    }
    let lane_last_idx = lane_path.len().saturating_sub(1);
    if lane_last_idx == 0 {
        return 0;
    }

    // 1. Dónde está el frente aliado (nuestro escudo)
    let allied_front = minions
        .iter()
        .filter(|m| {
            m.alive
                && normalized_team(&m.team) == normalized_team(&champion.team)
                && normalized_lane(&m.lane) == normalized_lane(&champion.lane)
        })
        .max_by(|a, b| a.path_index.cmp(&b.path_index));

    if let Some(front) = allied_front {
        let mut idx = front.path_index.saturating_sub(1).clamp(1, lane_last_idx);

        // MID can get pinned too far back when allied-front sampling catches trailing minions.
        // Bias anchor forward with enemy minion context when available.
        if champion.role == "MID" {
            if let Some(enemy_unit) = minions
                .iter()
                .filter(|m| {
                    m.alive
                        && normalized_team(&m.team) != normalized_team(&champion.team)
                        && normalized_lane(&m.lane) == normalized_lane(&champion.lane)
                })
                .min_by(|a, b| {
                    dist(a.pos, champion.pos)
                        .partial_cmp(&dist(b.pos, champion.pos))
                        .unwrap_or(Ordering::Equal)
                })
            {
                let enemy_idx = closest_lane_path_index(enemy_unit.pos, &lane_path);
                let enemy_bias = enemy_idx.saturating_sub(1).clamp(1, lane_last_idx);
                idx = idx.max(enemy_bias);
            }
        }

        return idx;
    }

    let current_index = closest_lane_path_index(champion.pos, &lane_path);
    // Empty-lane fallback was too defensive for MID and could pin under own tower.
    // Allow MID to keep a more forward neutral anchor even when no minions are nearby.
    let empty_lane_cap = if champion.role == "MID" {
        LANE_EMPTY_ANCHOR_PROGRESS_MAX_INDEX + 3
    } else {
        LANE_EMPTY_ANCHOR_PROGRESS_MAX_INDEX
    };
    let capped_current = current_index.min(empty_lane_cap);
    let min_floor = if champion.role == "MID" { 3 } else { 1 };
    capped_current.clamp(min_floor, lane_last_idx)
}
pub(super) fn lane_anchor_pos(
    champion: &ChampionRuntime,
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> super::Vec2 {
    let lane_path = lane_path_for(&champion.team, &champion.lane);
    let idx = choose_lane_anchor_index(champion, minions, structures);
    lane_path
        .get(idx)
        .copied()
        .unwrap_or(base_position_for(&champion.team))
}

pub(super) fn lane_fallback_pos_from_tower(
    champion: &ChampionRuntime,
    tower_pos: Vec2,
    toward_base: bool,
) -> Vec2 {
    let lane_path = lane_path_for(&champion.team, &champion.lane);
    let tower_idx = closest_lane_path_index(tower_pos, &lane_path);
    let lane_target = if toward_base {
        lane_path
            .get(tower_idx.saturating_sub(1))
            .copied()
            .unwrap_or(base_position_for(&champion.team))
    } else {
        lane_path
            .get((tower_idx + 1).min(lane_path.len().saturating_sub(1)))
            .copied()
            .unwrap_or(tower_pos)
    };

    let dir = normalize(Vec2 {
        x: lane_target.x - tower_pos.x,
        y: lane_target.y - tower_pos.y,
    });
    let offset = if toward_base { 0.019 } else { 0.024 };
    Vec2 {
        x: clamp(tower_pos.x + dir.x * offset, 0.01, 0.99),
        y: clamp(tower_pos.y + dir.y * offset, 0.01, 0.99),
    }
}

pub(super) fn lane_pre_wave_hold_pos(
    champion: &ChampionRuntime,
    structures: &[StructureRuntime],
) -> Vec2 {
    let lane_path = lane_path_for(&champion.team, &champion.lane);
    let allied_lane_tower = structures
        .iter()
        .filter(|s| {
            s.alive
                && s.kind == "tower"
                && normalized_team(&s.team) == normalized_team(&champion.team)
                && normalized_lane(&s.lane) == normalized_lane(&champion.lane)
        })
        .max_by(|a, b| {
            let idx_a = closest_lane_path_index(a.pos, &lane_path);
            let idx_b = closest_lane_path_index(b.pos, &lane_path);
            idx_a.cmp(&idx_b)
        });

    if let Some(tower) = allied_lane_tower {
        return lane_fallback_pos_from_tower(champion, tower.pos, false);
    }

    lane_path
        .get(2.min(lane_path.len().saturating_sub(1)))
        .copied()
        .unwrap_or(base_position_for(&champion.team))
}

pub(super) fn lane_wave_front_pos(
    champion: &ChampionRuntime,
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> super::Vec2 {
    let champion_team = normalized_team(&champion.team);
    let champion_lane = normalized_lane(&champion.lane);
    let sample_wave = |allied: bool| -> Option<super::Vec2> {
        let mut front: [Option<(usize, super::Vec2)>; 3] = [None, None, None];
        for minion in minions.iter().filter(|m| {
            m.alive
                && normalized_lane(&m.lane) == champion_lane
                && (normalized_team(&m.team) == champion_team) == allied
        }) {
            let entry = (minion.path_index, minion.pos);
            for slot in 0..front.len() {
                if front[slot]
                    .map(|(path_index, _)| entry.0 > path_index)
                    .unwrap_or(true)
                {
                    for shift in ((slot + 1)..front.len()).rev() {
                        front[shift] = front[shift - 1];
                    }
                    front[slot] = Some(entry);
                    break;
                }
            }
        }

        let mut count = 0usize;
        let mut sum = super::Vec2 { x: 0.0, y: 0.0 };
        for (_, pos) in front.into_iter().flatten() {
            count += 1;
            sum.x += pos.x;
            sum.y += pos.y;
        }
        (count > 0).then_some(super::Vec2 {
            x: sum.x / count as f64,
            y: sum.y / count as f64,
        })
    };

    let allied_wave = sample_wave(true);
    let enemy_wave = sample_wave(false);

    match (allied_wave, enemy_wave) {
        (Some(a), Some(e)) => super::Vec2 {
            x: (a.x + e.x) * 0.5,
            y: (a.y + e.y) * 0.5,
        },
        (Some(a), None) => a,
        (None, Some(e)) => e,
        (None, None) => lane_anchor_pos(champion, minions, structures),
    }
}

pub(super) fn lane_pressure_at(
    champion: &ChampionRuntime,
    pos: super::Vec2,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    radius: f64,
) -> LanePressure {
    let ally_champions = champions
        .iter()
        .filter(|u| {
            u.alive
                && normalized_team(&u.team) == normalized_team(&champion.team)
                && dist(u.pos, pos) <= radius
        })
        .count();
    let enemy_champions = champions
        .iter()
        .filter(|u| {
            u.alive
                && normalized_team(&u.team) != normalized_team(&champion.team)
                && dist(u.pos, pos) <= radius
        })
        .count();
    let ally_lane_minions = minions
        .iter()
        .filter(|m| {
            m.alive
                && normalized_team(&m.team) == normalized_team(&champion.team)
                && normalized_lane(&m.lane) == normalized_lane(&champion.lane)
                && dist(m.pos, pos) <= radius
        })
        .count();
    let enemy_lane_minions = minions
        .iter()
        .filter(|m| {
            m.alive
                && normalized_team(&m.team) != normalized_team(&champion.team)
                && normalized_lane(&m.lane) == normalized_lane(&champion.lane)
                && dist(m.pos, pos) <= radius
        })
        .count();

    let ally_score = ally_champions as f64 * 1.25 + ally_lane_minions as f64 * 0.48;
    let enemy_score = enemy_champions as f64 * 1.25 + enemy_lane_minions as f64 * 0.48;

    LanePressure {
        ally_champions,
        enemy_champions,
        ally_lane_minions,
        enemy_lane_minions,
        ally_score,
        enemy_score,
    }
}

pub(super) fn move_champions(runtime: &mut RuntimeState, dt: f64) {
    let now = runtime.time_sec;
    let champion_snapshot = runtime.champions.clone();
    let neutral_timers_snapshot = super::decode_neutral_timers_state(&runtime.neutral_timers);
    let team_tactics_snapshot = runtime.extra.get("teamTactics").cloned();
    let team_buffs_snapshot = runtime.extra.get("teamBuffs").cloned();

    for champion in &mut runtime.champions {
        if champion.realm_banished_until > 0.0 {
            if now >= champion.realm_banished_until {
                champion.realm_banished_until = 0.0;
                if let Some(return_pos) = champion.realm_return_pos {
                    champion.pos = return_pos;
                }
                champion.realm_return_pos = None;
                champion.target_path.clear();
                champion.target_path_index = 0;
                champion.next_decision_at = now;
                continue;
            } else {
                continue;
            }
        }

        if !champion.alive {
            if now >= champion.respawn_at {
                champion.alive = true;
                champion.hp = champion.max_hp;
                champion.pos = base_position_for(&champion.team);
                super::maybe_upgrade_trinket_to_oracle(champion, now);
                champion.attack_cd_until = now;
                champion.state = "lane".to_string();
                champion.recall_anchor = None;
                champion.recall_channel_until = 0.0;
                champion.target_path.clear();
                champion.target_path_index = 0;
                champion.next_decision_at = now;
            } else {
                continue;
            }
        }

        if dist(champion.pos, base_position_for(&champion.team)) <= 0.075 {
            champion.hp = champion.max_hp;
        }

        if now >= champion.next_decision_at {
            let old_state = champion.state.clone();

            super::decide_champion_state(
                champion,
                now,
                &runtime.minions,
                &runtime.structures,
                &champion_snapshot,
                neutral_timers_snapshot.as_ref(),
                &super::team_tactics_for_runtime(team_tactics_snapshot.as_ref(), &champion.team),
                &super::team_buffs_for_runtime(team_buffs_snapshot.as_ref(), &champion.team),
            );

            if old_state != champion.state || champion.debug_ai_decision.is_empty() {
                champion.debug_ai_decision = format!(
                    "state:{}->{}|hp:{:.0}%",
                    old_state,
                    champion.state,
                    champion.hp / champion.max_hp.max(1.0) * 100.0,
                );
            }

            champion.next_decision_at = now
                + (super::CHAMPION_DECISION_CADENCE_SEC
                    / champion.staff_execution.clamp(0.96, 1.10));
        }

        if champion.state == "recall" {
            champion.path_stuck_for_sec = 0.0;
            super::tick_recall(
                champion,
                now,
                &champion_snapshot,
                &runtime.minions,
                &runtime.structures,
                &mut runtime.events,
            );
            if champion.state == "recall" && champion.recall_channel_until > now {
                continue;
            }
        }

        if champion.target_path.is_empty() {
            if champion.role == "JGL" {
                champion.next_decision_at = now;
                continue;
            }
            champion.target_path = lane_path_for(&champion.team, &champion.lane);
            champion.target_path_index = 1;
        }

        if champion.target_path_index >= champion.target_path.len() {
            champion.target_path_index = champion.target_path.len().saturating_sub(1);
        }

        if let Some(target) = champion
            .target_path
            .get(champion.target_path_index)
            .copied()
        {
            let pre_dist = dist(champion.pos, target);
            let buffs = super::team_buffs_for_runtime(team_buffs_snapshot.as_ref(), &champion.team);
            let mut speed_multiplier =
                1.0 + buffs.cloud_stacks as f64 * 0.015 + buffs.hextech_stacks as f64 * 0.01;
            if buffs.soul_kind.as_deref() == Some("cloud") {
                speed_multiplier += 0.08;
            }
            if buffs.soul_kind.as_deref() == Some("hextech") {
                speed_multiplier += 0.04;
            }
            super::move_entity(
                &mut champion.pos,
                target,
                champion.move_speed * speed_multiplier,
                dt,
            );
            let post_dist = dist(champion.pos, target);
            let progress = (pre_dist - post_dist).max(0.0);
            if pre_dist > 0.012 && progress < super::CHAMPION_STUCK_PROGRESS_EPSILON {
                champion.path_stuck_for_sec += dt;
            } else {
                champion.path_stuck_for_sec = 0.0;
            }

            if champion.path_stuck_for_sec >= super::CHAMPION_STUCK_TRIGGER_SEC {
                champion.path_stuck_for_sec = 0.0;
                champion.next_decision_at = now;
                if champion.role == "JGL" {
                    champion.target_path.clear();
                    champion.target_path_index = 0;
                    continue;
                }
                champion.target_path.clear();
                champion.target_path_index = 0;
                continue;
            }

            if dist(champion.pos, target) < 0.01
                && champion.target_path_index < champion.target_path.len().saturating_sub(1)
            {
                champion.target_path_index += 1;
            }
        }

        let buffs = super::team_buffs_for_runtime(team_buffs_snapshot.as_ref(), &champion.team);
        let mut ocean_regen = buffs.ocean_stacks as f64 * 0.45;
        if buffs.soul_kind.as_deref() == Some("ocean") {
            ocean_regen += 1.2;
        }
        if ocean_regen > 0.0 && (now - champion.last_damaged_at) >= 5.0 {
            champion.hp = (champion.hp + ocean_regen * dt).min(champion.max_hp);
        }

        champion.pos.x = clamp(champion.pos.x, 0.01, 0.99);
        champion.pos.y = clamp(champion.pos.y, 0.01, 0.99);

        if champion.state == "recall" {
            super::tick_recall(
                champion,
                now,
                &champion_snapshot,
                &runtime.minions,
                &runtime.structures,
                &mut runtime.events,
            );
        }
    }
}

pub(super) fn lane_minion_context_distance(
    champion: &ChampionRuntime,
    pos: super::Vec2,
    minions: &[MinionRuntime],
) -> f64 {
    minions
        .iter()
        .filter(|m| m.alive && normalized_lane(&m.lane) == normalized_lane(&champion.lane))
        .map(|m| dist(pos, m.pos))
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal))
        .unwrap_or(f64::INFINITY)
}
