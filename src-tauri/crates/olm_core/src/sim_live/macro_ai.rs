use std::cmp::Ordering;

use super::{
    BASE_DEFENSE_RECALL_DISTANCE, ChampionRuntime, FIRST_WAVE_CONTEST_UNTIL,
    JUNGLE_CAMP_WAIT_FOR_SPAWN_SEC, JUNGLE_STICKY_CAMP_RADIUS, LANE_COMBAT_UNLOCK_AT,
    LANE_HEALTHY_RETREAT_HP_RATIO, LANE_LOCAL_PRESSURE_RADIUS,
    LANE_STRONG_UNFAVORABLE_PRESSURE_DELTA, LANE_STRUCTURE_PRESSURE_RADIUS,
    MAJOR_OBJECTIVE_TEAM_ASSIST_RADIUS, MINION_XP_SHARE_RADIUS, MinionRuntime,
    NEXUS_DEFENSE_THREAT_RADIUS, NeutralTimerRuntime, NeutralTimersRuntime,
    OBJECTIVE_ASSIST_RADIUS, OBJECTIVE_ATTEMPT_RADIUS, OBJECTIVE_PATH_MIN_TARGET_DELTA,
    RECALL_CANCEL_ENEMY_RADIUS, RECALL_CHANNEL_SEC, RECALL_REACH_BUFFER_SEC,
    RECALL_TRIGGER_HP_RATIO, RuntimeTeamBuffState, RuntimeTeamTactics, SUPPORT_OPEN_ROAM_AT_SEC,
    SUPPORT_ROAM_UNLOCK_AT_SEC, StructureRuntime, Vec2, base_position_for,
    champion_can_afford_next_item, clamp, dist, is_first_wave_contest_active,
    lane_fallback_pos_from_tower, lane_farm_anchor_pos_v2, lane_path_for, lane_pre_wave_hold_pos,
    lane_pressure_at, lane_role_profile, lane_wave_front_pos, normalize, normalized_lane,
    normalized_team, set_champion_direct_path, set_champion_direct_path_hysteresis, start_recall,
    stat_delta,
};

const FORCED_LANE_RECALL_COOLDOWN_SEC: f64 = 55.0;
const FORCED_LANE_RECALL_MAX_HP_RATIO: f64 = 0.58;
const WALK_TO_BASE_HEAL_DISTANCE: f64 = 0.17;

pub(super) fn nearest_enemy_champion_snapshot<'a>(
    champion: &ChampionRuntime,
    champions: &'a [ChampionRuntime],
    radius: f64,
) -> Option<&'a ChampionRuntime> {
    champions
        .iter()
        .filter(|enemy| {
            enemy.alive
                && enemy.id != champion.id
                && normalized_team(&enemy.team) != normalized_team(&champion.team)
                && dist(enemy.pos, champion.pos) <= radius
        })
        .min_by(|a, b| {
            dist(a.pos, champion.pos)
                .partial_cmp(&dist(b.pos, champion.pos))
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        })
}

pub(super) fn should_recall_in_place(
    champion: &ChampionRuntime,
    champions: &[ChampionRuntime],
) -> bool {
    // Recall should only be blocked by VERY close enemies.
    // Distant enemies pushing the wave must not prevent backing.
    let nearest = nearest_enemy_champion_snapshot(champion, champions, RECALL_CANCEL_ENEMY_RADIUS);
    let Some(enemy) = nearest else {
        return true;
    };
    let d = dist(champion.pos, enemy.pos);
    let enemy_reach_time = d / enemy.move_speed.max(0.01);
    enemy_reach_time > RECALL_CHANNEL_SEC + RECALL_REACH_BUFFER_SEC
}

pub(super) fn recall_fallback_toward_base(
    champion: &ChampionRuntime,
    threat: Option<&ChampionRuntime>,
) -> Vec2 {
    let base = base_position_for(&champion.team);

    let direction = if let Some(enemy) = threat {
        let away = normalize(Vec2 {
            x: champion.pos.x - enemy.pos.x,
            y: champion.pos.y - enemy.pos.y,
        });
        let toward_base = normalize(Vec2 {
            x: base.x - champion.pos.x,
            y: base.y - champion.pos.y,
        });
        normalize(Vec2 {
            x: away.x * 0.8 + toward_base.x * 0.2,
            y: away.y * 0.8 + toward_base.y * 0.2,
        })
    } else {
        normalize(Vec2 {
            x: base.x - champion.pos.x,
            y: base.y - champion.pos.y,
        })
    };

    let step = if champion.role == "JGL" { 0.05 } else { 0.04 };
    Vec2 {
        x: clamp(champion.pos.x + direction.x * step, 0.01, 0.99),
        y: clamp(champion.pos.y + direction.y * step, 0.01, 0.99),
    }
}

pub(super) fn weakest_enemy_lane_for_team(
    structures: &[StructureRuntime],
    team: &str,
) -> Option<&'static str> {
    let enemy = if normalized_team(team) == "blue" {
        "red"
    } else {
        "blue"
    };
    let lane_count = |lane: &str| -> usize {
        structures
            .iter()
            .filter(|structure| {
                structure.alive
                    && structure.kind == "tower"
                    && normalized_team(&structure.team) == enemy
                    && normalized_lane(&structure.lane) == lane
            })
            .count()
    };

    let top = lane_count("top");
    let mid = lane_count("mid");
    let bot = lane_count("bot");

    if top <= mid && top <= bot {
        Some("top")
    } else if mid <= top && mid <= bot {
        Some("mid")
    } else {
        Some("bot")
    }
}

pub(super) fn baron_push_target_for_lane(
    structures: &[StructureRuntime],
    team: &str,
    lane: &str,
    is_targetable: impl Fn(&[StructureRuntime], &str, &StructureRuntime) -> bool,
) -> Option<Vec2> {
    let enemy = if normalized_team(team) == "blue" {
        "red"
    } else {
        "blue"
    };
    let lane_tower = structures
        .iter()
        .filter(|structure| {
            structure.alive
                && structure.kind == "tower"
                && normalized_team(&structure.team) == enemy
                && normalized_lane(&structure.lane) == lane
        })
        .min_by(|a, b| a.id.cmp(&b.id));

    if let Some(tower) = lane_tower {
        return Some(tower.pos);
    }

    let lane_inhib = structures.iter().find(|structure| {
        structure.alive
            && normalized_team(&structure.team) == enemy
            && structure.kind == "inhib"
            && structure.id.contains(lane)
            && is_targetable(structures, team, structure)
    });

    if let Some(inhib) = lane_inhib {
        return Some(inhib.pos);
    }

    let nexus_tower = structures.iter().find(|structure| {
        structure.alive
            && normalized_team(&structure.team) == enemy
            && structure.kind == "tower"
            && structure.lane == "base"
            && structure.id.contains("nexus")
            && is_targetable(structures, team, structure)
    });

    if let Some(tower) = nexus_tower {
        return Some(tower.pos);
    }

    structures
        .iter()
        .find(|structure| {
            structure.alive
                && normalized_team(&structure.team) == enemy
                && structure.kind == "nexus"
                && is_targetable(structures, team, structure)
        })
        .map(|nexus| nexus.pos)
}

pub(super) fn allied_wave_ready_for_baron_siege(
    minions: &[MinionRuntime],
    team: &str,
    lane: &str,
    target_pos: Vec2,
) -> bool {
    minions
        .iter()
        .filter(|minion| {
            minion.alive
                && normalized_team(&minion.team) == normalized_team(team)
                && normalized_lane(&minion.lane) == normalized_lane(lane)
                && dist(minion.pos, target_pos) <= 0.12
        })
        .count()
        >= 2
}

pub(super) fn baron_push_rally_target(
    champion: &ChampionRuntime,
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
    team: &str,
    lane: &str,
    is_targetable: impl Fn(&[StructureRuntime], &str, &StructureRuntime) -> bool,
) -> Option<Vec2> {
    let siege_target = baron_push_target_for_lane(structures, team, lane, &is_targetable)?;
    if allied_wave_ready_for_baron_siege(minions, team, lane, siege_target) {
        return Some(siege_target);
    }

    let allied_wave_anchor = minions
        .iter()
        .filter(|minion| {
            minion.alive
                && normalized_team(&minion.team) == normalized_team(team)
                && normalized_lane(&minion.lane) == normalized_lane(lane)
        })
        .min_by(|a, b| {
            dist(a.pos, siege_target)
                .partial_cmp(&dist(b.pos, siege_target))
                .unwrap_or(Ordering::Equal)
        });

    if let Some(anchor) = allied_wave_anchor {
        let dir = normalize(Vec2 {
            x: anchor.pos.x - siege_target.x,
            y: anchor.pos.y - siege_target.y,
        });
        return Some(Vec2 {
            x: clamp(anchor.pos.x + dir.x * 0.012, 0.01, 0.99),
            y: clamp(anchor.pos.y + dir.y * 0.012, 0.01, 0.99),
        });
    }

    let wave_front = lane_wave_front_pos(champion, minions, structures);
    let dir = normalize(Vec2 {
        x: wave_front.x - siege_target.x,
        y: wave_front.y - siege_target.y,
    });
    Some(Vec2 {
        x: clamp(wave_front.x + dir.x * 0.018, 0.01, 0.99),
        y: clamp(wave_front.y + dir.y * 0.018, 0.01, 0.99),
    })
}

fn closest_lane_path_index(pos: Vec2, path: &[Vec2]) -> usize {
    path.iter()
        .enumerate()
        .min_by(|(idx_a, a), (idx_b, b)| {
            dist(**a, pos)
                .partial_cmp(&dist(**b, pos))
                .unwrap_or(Ordering::Equal)
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

pub(super) fn pick_allied_lane_fallback_tower(
    champion: &ChampionRuntime,
    threat_pos: Vec2,
    allow_emergency_retreat: bool,
    structures: &[StructureRuntime],
    lane_path: &[Vec2],
) -> Option<usize> {
    let mut towers: Vec<(usize, usize)> = structures
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s.alive
                && s.kind == "tower"
                && normalized_team(&s.team) == normalized_team(&champion.team)
                && normalized_lane(&s.lane) == normalized_lane(&champion.lane)
        })
        .map(|(idx, tower)| (idx, closest_lane_path_index(tower.pos, lane_path)))
        .collect();

    towers.sort_by(|(idx_a, path_a), (idx_b, path_b)| {
        path_a.cmp(path_b).then_with(|| idx_a.cmp(idx_b))
    });
    if towers.is_empty() {
        return None;
    }

    let threat_index = closest_lane_path_index(threat_pos, lane_path);
    let mut selected = towers
        .iter()
        .filter(|(_, path_index)| *path_index <= threat_index + 1)
        .max_by(|(idx_a, path_a), (idx_b, path_b)| {
            path_a.cmp(path_b).then_with(|| idx_a.cmp(idx_b))
        })
        .copied();

    if selected.is_none() {
        selected = towers
            .iter()
            .min_by(|(idx_a, path_a), (idx_b, path_b)| {
                dist(threat_pos, structures[*idx_a].pos)
                    .partial_cmp(&dist(threat_pos, structures[*idx_b].pos))
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| path_b.cmp(path_a))
                    .then_with(|| idx_a.cmp(idx_b))
            })
            .copied();
    }

    let Some(selected_entry) = selected else {
        return None;
    };
    if allow_emergency_retreat || towers.len() < 2 {
        return Some(selected_entry.0);
    }

    let mut lane_defense_band = towers.clone();
    lane_defense_band.sort_by(|(idx_a, path_a), (idx_b, path_b)| {
        path_b.cmp(path_a).then_with(|| idx_a.cmp(idx_b))
    });
    lane_defense_band.truncate(2);
    let min_safe_band_index = lane_defense_band
        .iter()
        .map(|(_, path_index)| *path_index)
        .min()
        .unwrap_or(selected_entry.1);

    if selected_entry.1 >= min_safe_band_index {
        return Some(selected_entry.0);
    }

    towers
        .iter()
        .filter(|(_, path_index)| *path_index >= min_safe_band_index)
        .min_by(|(idx_a, path_a), (idx_b, path_b)| {
            path_a
                .abs_diff(min_safe_band_index)
                .cmp(&path_b.abs_diff(min_safe_band_index))
                .then_with(|| path_b.cmp(path_a))
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(idx, _)| *idx)
        .or(Some(selected_entry.0))
}

pub(super) fn should_allow_emergency_retreat(
    champion: &ChampionRuntime,
    threat_pos: Vec2,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
) -> bool {
    if champion.role == "JGL" {
        return false;
    }
    let Some(profile) = lane_role_profile(champion) else {
        return false;
    };

    let hp_ratio = if champion.max_hp <= 0.0 {
        1.0
    } else {
        champion.hp / champion.max_hp
    };
    if hp_ratio <= profile.retreat_hp {
        return true;
    }

    let pressure = lane_pressure_at(
        champion,
        threat_pos,
        champions,
        minions,
        LANE_LOCAL_PRESSURE_RADIUS,
    );
    let strongly_unfavorable = pressure.enemy_score
        >= pressure.ally_score
            + profile.outnumber_tolerance
            + LANE_STRONG_UNFAVORABLE_PRESSURE_DELTA
        || pressure.enemy_champions >= pressure.ally_champions + 1;
    if !strongly_unfavorable {
        return false;
    }

    hp_ratio < LANE_HEALTHY_RETREAT_HP_RATIO
        || pressure.enemy_champions >= pressure.ally_champions + 2
}

pub(super) fn lane_retreat_anchor_pos(
    champion: &ChampionRuntime,
    threat_pos: Vec2,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> Vec2 {
    if champion.role == "JGL" {
        return base_position_for(&champion.team);
    }

    let hp_ratio = if champion.max_hp <= 0.0 {
        1.0
    } else {
        champion.hp / champion.max_hp
    };
    if is_first_wave_contest_active(champion, now) && hp_ratio >= 0.45 {
        return lane_farm_anchor_pos_v2(champion, now, champions, minions, structures);
    }

    let farm_anchor = lane_farm_anchor_pos_v2(champion, now, champions, minions, structures);
    let emergency = should_allow_emergency_retreat(champion, threat_pos, champions, minions);
    let Some(tower_idx) = pick_allied_lane_fallback_tower(
        champion,
        threat_pos,
        emergency,
        structures,
        &lane_path_for(&champion.team, &champion.lane),
    ) else {
        if champion.state == "recall" {
            return base_position_for(&champion.team);
        }
        return farm_anchor;
    };
    let tower = &structures[tower_idx];

    let tower_fallback = lane_fallback_pos_from_tower(champion, tower.pos, emergency);
    if champion.state == "recall" {
        return tower_fallback;
    }
    if emergency {
        return tower_fallback;
    }

    // Non-emergency disengage should not pin laners under tower.
    // Keep pressure/farm behavior unless we are in explicit emergency retreat.
    farm_anchor
}

pub(super) fn decide_champion_state(
    champion: &mut ChampionRuntime,
    now: f64,
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
    champions: &[ChampionRuntime],
    neutral_timers: Option<&NeutralTimersRuntime>,
    team_tactics: &RuntimeTeamTactics,
    team_buffs: &RuntimeTeamBuffState,
) {
    if champion.state == "recall" {
        return;
    }

    if champion_can_afford_next_item(champion) {
        start_recall(champion, now, champions, minions, structures);
        return;
    }

    let hp_ratio = if champion.max_hp <= 0.0 {
        1.0
    } else {
        champion.hp / champion.max_hp
    };
    if hp_ratio <= RECALL_TRIGGER_HP_RATIO {
        let base = base_position_for(&champion.team);
        if dist(champion.pos, base) <= WALK_TO_BASE_HEAL_DISTANCE {
            champion.state = "lane".to_string();
            set_champion_direct_path_hysteresis(champion, base, OBJECTIVE_PATH_MIN_TARGET_DELTA);
            return;
        }
        start_recall(champion, now, champions, minions, structures);
        return;
    }

    if champion.role == "JGL" {
        if let Some(timers) = neutral_timers {
            if let Some(objective_pos) = pick_live_neutral_objective_pos(champion, timers) {
                champion.state = "objective".to_string();
                set_champion_direct_path_hysteresis(
                    champion,
                    objective_pos,
                    OBJECTIVE_PATH_MIN_TARGET_DELTA,
                );
                return;
            }

            if let Some(camp_pos) = pick_sticky_or_next_jungle_camp_pos(
                champion,
                timers,
                &team_tactics.jungle_pathing,
                now,
            ) {
                champion.state = "jungle".to_string();
                set_champion_direct_path_hysteresis(
                    champion,
                    camp_pos,
                    OBJECTIVE_PATH_MIN_TARGET_DELTA,
                );
                return;
            }
        }
    }

    // Hard anti-stuck rule:
    // if a laner drifts behind their own lane tower after opening phase,
    // force immediate recall (ignore threat/range heuristics).
    if champion.role != "JGL"
        && now > FIRST_WAVE_CONTEST_UNTIL
        && now >= champion.forced_lane_recall_cd_until
        && hp_ratio <= FORCED_LANE_RECALL_MAX_HP_RATIO
        && is_behind_own_lane_tower(champion, structures)
    {
        champion.forced_lane_recall_cd_until = now + FORCED_LANE_RECALL_COOLDOWN_SEC;
        start_recall(champion, now, champions, minions, structures);
        return;
    }

    if let Some(defense_pos) =
        allied_nexus_under_threat_pos(champion, champions, minions, structures)
    {
        if dist(champion.pos, defense_pos) > BASE_DEFENSE_RECALL_DISTANCE {
            start_recall(champion, now, champions, minions, structures);
        } else {
            champion.state = "objective".to_string();
            set_champion_direct_path_hysteresis(
                champion,
                defense_pos,
                OBJECTIVE_PATH_MIN_TARGET_DELTA,
            );
        }
        return;
    }

    if team_buffs.baron_until > now {
        if let Some(lane) = weakest_enemy_lane_for_team(structures, &champion.team) {
            if let Some(push_target) = baron_push_rally_target(
                champion,
                minions,
                structures,
                &champion.team,
                lane,
                super::is_structure_targetable,
            ) {
                champion.state = "objective".to_string();
                set_champion_direct_path_hysteresis(
                    champion,
                    push_target,
                    OBJECTIVE_PATH_MIN_TARGET_DELTA,
                );
                return;
            }
        }
    }

    if let Some(timers) = neutral_timers {
        if champion.state == "objective" {
            let allied_nearby = champions
                .iter()
                .filter(|ally| {
                    ally.alive
                        && normalized_team(&ally.team) == normalized_team(&champion.team)
                        && dist(ally.pos, champion.pos) <= OBJECTIVE_ASSIST_RADIUS
                })
                .count();
            let enemy_nearby = champions
                .iter()
                .filter(|enemy| {
                    enemy.alive
                        && normalized_team(&enemy.team) != normalized_team(&champion.team)
                        && dist(enemy.pos, champion.pos) <= OBJECTIVE_ASSIST_RADIUS
                })
                .count();
            if allied_nearby >= 2 && enemy_nearby >= 1 {
                if let Some(enemy) = champions
                    .iter()
                    .filter(|enemy| {
                        enemy.alive
                            && normalized_team(&enemy.team) != normalized_team(&champion.team)
                            && dist(enemy.pos, champion.pos) <= OBJECTIVE_ASSIST_RADIUS
                    })
                    .min_by(|a, b| a.hp.partial_cmp(&b.hp).unwrap_or(Ordering::Equal))
                {
                    champion.state = "objective".to_string();
                    set_champion_direct_path_hysteresis(
                        champion,
                        enemy.pos,
                        OBJECTIVE_PATH_MIN_TARGET_DELTA,
                    );
                    return;
                }
            }
        }

        let contested_dragon = contested_dragon_attempt_for_team(&champion.team, champions, timers);
        if should_hard_assist_contested_dragon(champion, contested_dragon) {
            if let Some(dragon) = contested_dragon {
                champion.state = "objective".to_string();
                set_champion_direct_path_hysteresis(
                    champion,
                    dragon.pos,
                    OBJECTIVE_PATH_MIN_TARGET_DELTA,
                );
                return;
            }
        }

        if should_assist_objective_attempt(champion, champions, timers) {
            if let Some(attempt) =
                active_objective_attempt_for_team(&champion.team, champions, timers)
            {
                champion.state = "objective".to_string();
                set_champion_direct_path_hysteresis(
                    champion,
                    attempt.pos,
                    OBJECTIVE_PATH_MIN_TARGET_DELTA,
                );
                return;
            }
        }

        if champion.role == "JGL" {
            if let Some(objective_pos) =
                pick_macro_objective_pos(champion, champions, timers, now, team_tactics)
            {
                champion.state = "objective".to_string();
                set_champion_direct_path_hysteresis(
                    champion,
                    objective_pos,
                    OBJECTIVE_PATH_MIN_TARGET_DELTA,
                );
                return;
            }
        }

        if champion.role == "SUP" && now >= SUPPORT_ROAM_UNLOCK_AT_SEC {
            if now < SUPPORT_OPEN_ROAM_AT_SEC {
                let roam_target_role = match team_tactics.support_roaming.as_str() {
                    "RoamMid" => Some("MID"),
                    "RoamTop" => Some("TOP"),
                    _ => None,
                };
                if let Some(target_role) = roam_target_role {
                    if champion.support_roam_uses < 2 && now >= champion.support_roam_cd_until {
                        let ally_target = champions.iter().find(|ally| {
                            ally.alive
                                && ally.id != champion.id
                                && normalized_team(&ally.team) == normalized_team(&champion.team)
                                && ally.role == target_role
                        });
                        if let Some(ally_target) = ally_target {
                            champion.state = "objective".to_string();
                            champion.support_roam_uses += 1;
                            champion.support_roam_cd_until = now + 85.0;
                            champion.support_last_roam_role = target_role.to_string();
                            set_champion_direct_path_hysteresis(
                                champion,
                                ally_target.pos,
                                OBJECTIVE_PATH_MIN_TARGET_DELTA,
                            );
                            return;
                        }
                    }
                }
            } else if now >= champion.support_roam_cd_until {
                let ally_target = champions
                    .iter()
                    .filter(|ally| {
                        ally.alive
                            && ally.id != champion.id
                            && normalized_team(&ally.team) == normalized_team(&champion.team)
                            && (ally.role == "TOP" || ally.role == "MID" || ally.role == "ADC")
                    })
                    .min_by(|a, b| {
                        let a_ratio = if a.max_hp <= 0.0 {
                            1.0
                        } else {
                            a.hp / a.max_hp
                        };
                        let b_ratio = if b.max_hp <= 0.0 {
                            1.0
                        } else {
                            b.hp / b.max_hp
                        };
                        let a_repeat_penalty = if !champion.support_last_roam_role.is_empty()
                            && a.role
                                .eq_ignore_ascii_case(&champion.support_last_roam_role)
                        {
                            1
                        } else {
                            0
                        };
                        let b_repeat_penalty = if !champion.support_last_roam_role.is_empty()
                            && b.role
                                .eq_ignore_ascii_case(&champion.support_last_roam_role)
                        {
                            1
                        } else {
                            0
                        };

                        a_repeat_penalty
                            .cmp(&b_repeat_penalty)
                            .then_with(|| a_ratio.partial_cmp(&b_ratio).unwrap_or(Ordering::Equal))
                            .then_with(|| {
                                dist(champion.pos, a.pos)
                                    .partial_cmp(&dist(champion.pos, b.pos))
                                    .unwrap_or(Ordering::Equal)
                            })
                    });

                if let Some(ally_target) = ally_target {
                    champion.state = "objective".to_string();
                    champion.support_roam_cd_until = now + 55.0;
                    champion.support_last_roam_role = ally_target.role.clone();
                    set_champion_direct_path_hysteresis(
                        champion,
                        ally_target.pos,
                        OBJECTIVE_PATH_MIN_TARGET_DELTA,
                    );
                    return;
                }
            }
        }
    }

    if champion.role == "JGL" {
        champion.state = "jungle".to_string();
        if let Some(timers) = neutral_timers {
            if let Some(objective_pos) = pick_live_neutral_objective_pos(champion, timers) {
                champion.state = "objective".to_string();
                set_champion_direct_path_hysteresis(
                    champion,
                    objective_pos,
                    OBJECTIVE_PATH_MIN_TARGET_DELTA,
                );
                return;
            }

            if let Some(camp_pos) = pick_sticky_or_next_jungle_camp_pos(
                champion,
                timers,
                &team_tactics.jungle_pathing,
                now,
            ) {
                set_champion_direct_path_hysteresis(
                    champion,
                    camp_pos,
                    OBJECTIVE_PATH_MIN_TARGET_DELTA,
                );
                return;
            }

            if neutral_objective_alive(timers) {
                return;
            }
        }

        if let Some(gank_pos) = pick_jungler_gank_pos(champion, champions, structures) {
            set_champion_direct_path_hysteresis(
                champion,
                gank_pos,
                OBJECTIVE_PATH_MIN_TARGET_DELTA,
            );
            return;
        }
        return;
    }

    champion.state = "lane".to_string();

    if let Some(push_anchor) = post_tower_push_anchor(champion, minions, structures) {
        set_champion_direct_path_hysteresis(champion, push_anchor, OBJECTIVE_PATH_MIN_TARGET_DELTA);
        return;
    }

    let target = if now < LANE_COMBAT_UNLOCK_AT {
        lane_pre_wave_hold_pos(champion, structures)
    } else {
        lane_farm_anchor_pos_v2(champion, now, champions, minions, structures)
    };

    let lane_pressure = lane_pressure_at(
        champion,
        champion.pos,
        champions,
        minions,
        LANE_LOCAL_PRESSURE_RADIUS,
    );
    let enemy_advantage = lane_pressure.enemy_score - lane_pressure.ally_score;
    let safe_enough_for_xp = enemy_advantage <= 1.1 && hp_ratio >= 0.24;
    let target = if safe_enough_for_xp {
        maybe_xp_soak_anchor(champion, target, minions).unwrap_or(target)
    } else {
        target
    };

    set_champion_direct_path(champion, target);
}

fn is_behind_own_lane_tower(champion: &ChampionRuntime, structures: &[StructureRuntime]) -> bool {
    let lane_path = lane_path_for(&champion.team, &champion.lane);
    if lane_path.len() < 2 {
        return false;
    }

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

    let Some(tower) = allied_lane_tower else {
        return false;
    };

    let champ_idx = closest_lane_path_index(champion.pos, &lane_path);
    let tower_idx = closest_lane_path_index(tower.pos, &lane_path);
    champ_idx + 1 < tower_idx
}

fn post_tower_push_anchor(
    champion: &ChampionRuntime,
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> Option<Vec2> {
    if champion.role == "JGL" || champion.role == "SUP" {
        return None;
    }

    let lane = normalized_lane(&champion.lane);
    let team = normalized_team(&champion.team);

    let enemy_tower_down_in_lane = structures.iter().any(|structure| {
        normalized_team(&structure.team) != team
            && structure.kind == "tower"
            && normalized_lane(&structure.lane) == lane
            && !structure.alive
    });
    if !enemy_tower_down_in_lane {
        return None;
    }

    let allied_wave_nearby = minions
        .iter()
        .filter(|minion| {
            minion.alive
                && normalized_team(&minion.team) == team
                && normalized_lane(&minion.lane) == lane
                && dist(minion.pos, champion.pos) <= 0.20
        })
        .count();
    if allied_wave_nearby < 2 {
        return None;
    }

    let next_enemy_tower = structures
        .iter()
        .filter(|structure| {
            structure.alive
                && structure.kind == "tower"
                && normalized_team(&structure.team) != team
                && normalized_lane(&structure.lane) == lane
        })
        .min_by(|a, b| {
            dist(champion.pos, a.pos)
                .partial_cmp(&dist(champion.pos, b.pos))
                .unwrap_or(Ordering::Equal)
        })?;

    let dir_to_base = normalize(Vec2 {
        x: base_position_for(&champion.team).x - next_enemy_tower.pos.x,
        y: base_position_for(&champion.team).y - next_enemy_tower.pos.y,
    });

    Some(Vec2 {
        x: clamp(next_enemy_tower.pos.x + dir_to_base.x * 0.055, 0.01, 0.99),
        y: clamp(next_enemy_tower.pos.y + dir_to_base.y * 0.055, 0.01, 0.99),
    })
}

fn maybe_xp_soak_anchor(
    champion: &ChampionRuntime,
    fallback: Vec2,
    minions: &[MinionRuntime],
) -> Option<Vec2> {
    let lane = normalized_lane(&champion.lane);
    let enemy_minion = minions
        .iter()
        .filter(|minion| {
            minion.alive
                && normalized_team(&minion.team) != normalized_team(&champion.team)
                && normalized_lane(&minion.lane) == lane
        })
        .min_by(|a, b| {
            dist(champion.pos, a.pos)
                .partial_cmp(&dist(champion.pos, b.pos))
                .unwrap_or(Ordering::Equal)
        })?;

    let d = dist(champion.pos, enemy_minion.pos);
    if d <= MINION_XP_SHARE_RADIUS * 0.95 {
        return None;
    }

    let dir = normalize(Vec2 {
        x: champion.pos.x - enemy_minion.pos.x,
        y: champion.pos.y - enemy_minion.pos.y,
    });
    let soak = Vec2 {
        x: enemy_minion.pos.x + dir.x * (MINION_XP_SHARE_RADIUS * 0.8),
        y: enemy_minion.pos.y + dir.y * (MINION_XP_SHARE_RADIUS * 0.8),
    };

    if dist(soak, fallback) < 0.25 {
        Some(Vec2 {
            x: clamp(soak.x, 0.01, 0.99),
            y: clamp(soak.y, 0.01, 0.99),
        })
    } else {
        None
    }
}

pub(super) fn is_objective_neutral_key(key: &str) -> bool {
    matches!(
        key,
        "dragon" | "baron" | "herald" | "voidgrubs" | "elder" | "scuttle-top" | "scuttle-bot"
    )
}

fn objective_adjacent_lanes(key: &str) -> &'static [&'static str] {
    if key == "dragon" || key == "elder" || key == "scuttle-bot" {
        &["mid", "bot"]
    } else {
        &["mid", "top"]
    }
}

pub(super) fn is_jungle_camp_key(key: &str) -> bool {
    matches!(
        key,
        "blue-buff-blue"
            | "blue-buff-red"
            | "red-buff-blue"
            | "red-buff-red"
            | "wolves-blue"
            | "wolves-red"
            | "raptors-blue"
            | "raptors-red"
            | "gromp-blue"
            | "gromp-red"
            | "krugs-blue"
            | "krugs-red"
            | "scuttle-top"
            | "scuttle-bot"
    )
}

fn is_enemy_jungle_camp_key_for_team(key: &str, team: &str) -> bool {
    if !is_jungle_camp_key(key) {
        return false;
    }
    let own_suffix = if normalized_team(team) == "blue" {
        "-blue"
    } else {
        "-red"
    };
    (key.ends_with("-blue") || key.ends_with("-red")) && !key.ends_with(own_suffix)
}

fn is_own_jungle_camp_key_for_team(key: &str, team: &str) -> bool {
    if !is_jungle_camp_key(key) {
        return false;
    }
    let own_suffix = if normalized_team(team) == "blue" {
        "-blue"
    } else {
        "-red"
    };
    key.ends_with(own_suffix)
}

fn pick_sticky_or_next_jungle_camp_pos(
    champion: &ChampionRuntime,
    neutral_timers: &NeutralTimersRuntime,
    jungle_pathing: &str,
    now: f64,
) -> Option<Vec2> {
    if let Some(current_camp) = neutral_timers
        .entities
        .values()
        .filter(|timer| {
            timer.alive
                && timer.unlocked
                && is_own_jungle_camp_key_for_team(&timer.key, &champion.team)
                && dist(champion.pos, timer.pos) <= JUNGLE_STICKY_CAMP_RADIUS
        })
        .min_by(|a, b| {
            dist(champion.pos, a.pos)
                .partial_cmp(&dist(champion.pos, b.pos))
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.key.cmp(&b.key))
        })
    {
        return Some(current_camp.pos);
    }

    for key in jungler_macro_jungle_priority_for_team(&champion.team, jungle_pathing) {
        if !is_own_jungle_camp_key_for_team(key, &champion.team) {
            continue;
        }
        let Some(camp) = neutral_timers.entities.get(key) else {
            continue;
        };
        let spawning_soon = camp
            .next_spawn_at
            .map(|spawn_at| spawn_at >= now && spawn_at - now <= JUNGLE_CAMP_WAIT_FOR_SPAWN_SEC)
            .unwrap_or(false);
        if camp.unlocked && (camp.alive || spawning_soon) {
            return Some(camp.pos);
        }
    }

    None
}

fn neutral_objective_alive(neutral_timers: &NeutralTimersRuntime) -> bool {
    neutral_timers
        .entities
        .values()
        .any(|timer| timer.alive && timer.unlocked && is_objective_neutral_key(&timer.key))
}

fn pick_live_neutral_objective_pos(
    champion: &ChampionRuntime,
    neutral_timers: &NeutralTimersRuntime,
) -> Option<Vec2> {
    neutral_timers
        .entities
        .values()
        .filter(|timer| timer.alive && timer.unlocked && is_objective_neutral_key(&timer.key))
        .min_by(|a, b| {
            dist(champion.pos, a.pos)
                .partial_cmp(&dist(champion.pos, b.pos))
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.key.cmp(&b.key))
        })
        .map(|timer| timer.pos)
}

fn pick_jungler_gank_pos(
    champion: &ChampionRuntime,
    champions: &[ChampionRuntime],
    structures: &[StructureRuntime],
) -> Option<Vec2> {
    let enemy_team = if normalized_team(&champion.team) == "blue" {
        "red"
    } else {
        "blue"
    };
    champions
        .iter()
        .filter(|enemy| {
            enemy.alive
                && normalized_team(&enemy.team) == enemy_team
                && enemy.role != "JGL"
                && champions.iter().any(|ally| {
                    ally.alive
                        && ally.role != "JGL"
                        && normalized_team(&ally.team) == normalized_team(&champion.team)
                        && normalized_lane(&ally.lane) == normalized_lane(&enemy.lane)
                        && dist(ally.pos, enemy.pos) <= 0.17
                })
                && !structures.iter().any(|structure| {
                    structure.alive
                        && structure.kind == "tower"
                        && normalized_team(&structure.team) == enemy_team
                        && normalized_lane(&structure.lane) == normalized_lane(&enemy.lane)
                        && dist(structure.pos, enemy.pos) <= LANE_STRUCTURE_PRESSURE_RADIUS + 0.04
                })
        })
        .min_by(|a, b| {
            dist(champion.pos, a.pos)
                .partial_cmp(&dist(champion.pos, b.pos))
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        })
        .map(|enemy| enemy.pos)
}

pub(super) fn contested_dragon_attempt_for_team<'a>(
    team: &str,
    champions: &[ChampionRuntime],
    neutral_timers: &'a NeutralTimersRuntime,
) -> Option<&'a NeutralTimerRuntime> {
    let dragon = neutral_timers.entities.get("dragon")?;
    if !dragon.alive {
        return None;
    }
    let allied_jungler = champions.iter().find(|champion| {
        champion.alive
            && normalized_team(&champion.team) == normalized_team(team)
            && champion.role == "JGL"
    })?;
    if dist(allied_jungler.pos, dragon.pos) > OBJECTIVE_ASSIST_RADIUS {
        return None;
    }
    let enemy_team = if normalized_team(team) == "blue" {
        "red"
    } else {
        "blue"
    };
    let enemy_contestants = champions
        .iter()
        .filter(|enemy| {
            enemy.alive
                && normalized_team(&enemy.team) == enemy_team
                && dist(enemy.pos, dragon.pos) <= OBJECTIVE_ASSIST_RADIUS
        })
        .count();
    if enemy_contestants == 0 {
        return None;
    }
    let dragon_being_done = dragon.hp <= dragon.max_hp * 0.97
        || dist(allied_jungler.pos, dragon.pos) <= OBJECTIVE_ATTEMPT_RADIUS;
    if !dragon_being_done {
        return None;
    }
    Some(dragon)
}

pub(super) fn nearby_neutral_objective_key(
    champion: &ChampionRuntime,
    neutral_timers: &NeutralTimersRuntime,
) -> Option<String> {
    neutral_timers
        .entities
        .values()
        .filter(|timer| timer.alive && is_objective_neutral_key(&timer.key))
        .filter(|timer| dist(champion.pos, timer.pos) <= OBJECTIVE_ATTEMPT_RADIUS)
        .min_by(|a, b| {
            dist(champion.pos, a.pos)
                .partial_cmp(&dist(champion.pos, b.pos))
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.key.cmp(&b.key))
        })
        .map(|timer| timer.key.clone())
}

pub(super) fn active_objective_attempt_for_team<'a>(
    team: &str,
    champions: &[ChampionRuntime],
    neutral_timers: &'a NeutralTimersRuntime,
) -> Option<&'a NeutralTimerRuntime> {
    let allied_jungler = champions.iter().find(|champion| {
        champion.alive
            && normalized_team(&champion.team) == normalized_team(team)
            && champion.role == "JGL"
    })?;
    let enemy_team = if normalized_team(team) == "blue" {
        "red"
    } else {
        "blue"
    };
    neutral_timers
        .entities
        .values()
        .filter(|timer| timer.alive && is_objective_neutral_key(&timer.key))
        .filter_map(|timer| {
            let d = dist(allied_jungler.pos, timer.pos);
            if d > OBJECTIVE_ASSIST_RADIUS {
                return None;
            }
            let enemy_contest = champions.iter().any(|enemy| {
                enemy.alive
                    && normalized_team(&enemy.team) == enemy_team
                    && dist(enemy.pos, timer.pos) <= OBJECTIVE_ASSIST_RADIUS
            });
            let is_damaged = timer.hp <= timer.max_hp * 0.9;
            if !(enemy_contest || is_damaged) {
                return None;
            }
            Some((timer, d))
        })
        .min_by(|(a, d_a), (b, d_b)| {
            d_a.partial_cmp(d_b)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.key.cmp(&b.key))
        })
        .map(|(timer, _)| timer)
        .or_else(|| {
            // Proactive setup around objective windows: if jungler is already near,
            // laners can rotate to create/force the fight even before objective damage starts.
            neutral_timers
                .entities
                .values()
                .filter(|timer| timer.alive && is_objective_neutral_key(&timer.key))
                .filter(|timer| dist(allied_jungler.pos, timer.pos) <= OBJECTIVE_ASSIST_RADIUS)
                .filter(|timer| {
                    champions.iter().any(|enemy| {
                        enemy.alive
                            && normalized_team(&enemy.team) == enemy_team
                            && dist(enemy.pos, timer.pos) <= MAJOR_OBJECTIVE_TEAM_ASSIST_RADIUS
                    })
                })
                .min_by(|a, b| {
                    dist(allied_jungler.pos, a.pos)
                        .partial_cmp(&dist(allied_jungler.pos, b.pos))
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| a.key.cmp(&b.key))
                })
        })
}

pub(super) fn should_assist_objective_attempt(
    champion: &ChampionRuntime,
    champions: &[ChampionRuntime],
    neutral_timers: &NeutralTimersRuntime,
) -> bool {
    if champion.role == "JGL" {
        return false;
    }
    let Some(attempt) =
        active_objective_attempt_for_team(&champion.team, champions, neutral_timers)
    else {
        return false;
    };
    let iq_delta = stat_delta(champion.iq_score);
    let discipline_delta = stat_delta(champion.competitive_score);
    let proactive_rotation = iq_delta > -0.2;
    if is_major_teamfight_objective(attempt, neutral_timers) {
        return dist(champion.pos, attempt.pos) <= MAJOR_OBJECTIVE_TEAM_ASSIST_RADIUS
            && can_rotate_without_suicide(champion, attempt.pos, champions);
    }
    let lane = normalized_lane(&champion.lane);
    let role = champion.role.as_str();
    let role_priority = match attempt.key.as_str() {
        "voidgrubs" | "herald" | "baron" => role == "TOP" || role == "MID",
        "dragon" | "elder" => role == "ADC" || role == "SUP" || role == "MID",
        _ => role == "MID",
    };
    if role_priority
        && proactive_rotation
        && can_rotate_without_suicide(champion, attempt.pos, champions)
    {
        return true;
    }
    if !objective_adjacent_lanes(&attempt.key)
        .iter()
        .any(|adj| *adj == lane)
    {
        return false;
    }
    let enemy_team = if normalized_team(&champion.team) == "blue" {
        "red"
    } else {
        "blue"
    };
    let nearby_contestants = champions
        .iter()
        .filter(|enemy| {
            enemy.alive
                && normalized_team(&enemy.team) == enemy_team
                && dist(enemy.pos, attempt.pos) <= OBJECTIVE_ASSIST_RADIUS
        })
        .count();
    let patience_gate = (0.82 - iq_delta * 0.06 - discipline_delta * 0.03).clamp(0.70, 0.90);
    if nearby_contestants == 0 && attempt.hp > attempt.max_hp * patience_gate {
        return false;
    }
    true
}

pub(super) fn should_hard_assist_contested_dragon(
    champion: &ChampionRuntime,
    contested_dragon: Option<&NeutralTimerRuntime>,
) -> bool {
    if champion.role != "ADC" && champion.role != "SUP" {
        return false;
    }
    if normalized_lane(&champion.lane) != "bot" {
        return false;
    }
    contested_dragon.is_some()
}

fn is_major_teamfight_objective(
    attempt: &NeutralTimerRuntime,
    neutral_timers: &NeutralTimersRuntime,
) -> bool {
    attempt.key == "elder"
        || attempt.key == "baron"
        || (attempt.key == "dragon" && neutral_timers.dragon_soul_unlocked)
}

fn can_rotate_without_suicide(
    champion: &ChampionRuntime,
    objective_pos: Vec2,
    champions: &[ChampionRuntime],
) -> bool {
    let hp_ratio = super::ratio_or_zero(champion.hp, champion.max_hp);
    let iq_delta = stat_delta(champion.iq_score);
    let hp_floor = (0.38 - iq_delta * 0.06).clamp(0.28, 0.46);
    if hp_ratio < hp_floor {
        return false;
    }
    let ally_nearby = champions
        .iter()
        .filter(|ally| {
            ally.alive
                && normalized_team(&ally.team) == normalized_team(&champion.team)
                && dist(ally.pos, objective_pos) <= OBJECTIVE_ASSIST_RADIUS
        })
        .count();
    let enemy_nearby = champions
        .iter()
        .filter(|enemy| {
            enemy.alive
                && normalized_team(&enemy.team) != normalized_team(&champion.team)
                && dist(enemy.pos, objective_pos) <= OBJECTIVE_ASSIST_RADIUS
        })
        .count();
    let sync_bonus = if champion.iq_score >= 74.0 { 1 } else { 0 };
    ally_nearby + 1 + sync_bonus >= enemy_nearby
}

fn should_jungler_commit_major_objective(
    champion: &ChampionRuntime,
    objective: &NeutralTimerRuntime,
    champions: &[ChampionRuntime],
) -> bool {
    let hp_ratio = super::ratio_or_zero(champion.hp, champion.max_hp);
    if hp_ratio < 0.52 {
        return false;
    }
    let ally_nearby = champions
        .iter()
        .filter(|ally| {
            ally.alive
                && normalized_team(&ally.team) == normalized_team(&champion.team)
                && dist(ally.pos, objective.pos) <= OBJECTIVE_ASSIST_RADIUS
        })
        .count();
    let enemy_nearby = champions
        .iter()
        .filter(|enemy| {
            enemy.alive
                && normalized_team(&enemy.team) != normalized_team(&champion.team)
                && dist(enemy.pos, objective.pos) <= OBJECTIVE_ASSIST_RADIUS
        })
        .count();
    ally_nearby + 1 >= enemy_nearby
}

fn allied_nexus_under_threat_pos(
    champion: &ChampionRuntime,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> Option<Vec2> {
    let allied_nexus_towers: Vec<&StructureRuntime> = structures
        .iter()
        .filter(|structure| {
            structure.alive
                && structure.kind == "tower"
                && structure.id.contains("nexus")
                && normalized_team(&structure.team) == normalized_team(&champion.team)
        })
        .collect();
    if allied_nexus_towers.is_empty() {
        return None;
    }
    for tower in allied_nexus_towers {
        let champion_threat = champions.iter().any(|enemy| {
            enemy.alive
                && normalized_team(&enemy.team) != normalized_team(&champion.team)
                && dist(enemy.pos, tower.pos) <= NEXUS_DEFENSE_THREAT_RADIUS
        });
        let minion_threat = minions.iter().any(|enemy| {
            enemy.alive
                && normalized_team(&enemy.team) != normalized_team(&champion.team)
                && dist(enemy.pos, tower.pos) <= NEXUS_DEFENSE_THREAT_RADIUS
        });
        if champion_threat || minion_threat {
            return Some(tower.pos);
        }
    }
    None
}

pub(super) fn pick_macro_objective_pos(
    champion: &ChampionRuntime,
    champions: &[ChampionRuntime],
    neutral_timers: &NeutralTimersRuntime,
    now: f64,
    team_tactics: &RuntimeTeamTactics,
) -> Option<Vec2> {
    if champion.role != "JGL" {
        return None;
    }
    let objective_lead_time = match team_tactics.game_timing.as_str() {
        "Early" => 50.0,
        "Late" => 22.0,
        _ => 35.0,
    };
    for key in ["elder", "baron"] {
        let Some(timer) = neutral_timers.entities.get(key) else {
            continue;
        };
        if !timer.unlocked {
            continue;
        }
        if timer.alive {
            if !should_jungler_commit_major_objective(champion, timer, champions) {
                continue;
            }
            return Some(timer.pos);
        }
        if let Some(next_spawn_at) = timer.next_spawn_at {
            if next_spawn_at >= now && next_spawn_at - now <= objective_lead_time {
                return Some(timer.pos);
            }
        }
    }
    let side_objective_order: [&str; 5] = match team_tactics.strong_side.as_str() {
        "Top" => [
            "herald",
            "voidgrubs",
            "dragon",
            "scuttle-top",
            "scuttle-bot",
        ],
        "Mid" => [
            "dragon",
            "herald",
            "voidgrubs",
            "scuttle-bot",
            "scuttle-top",
        ],
        _ => [
            "dragon",
            "scuttle-bot",
            "herald",
            "voidgrubs",
            "scuttle-top",
        ],
    };
    let can_hard_invade = team_tactics.jungle_style == "Invader"
        || (now >= 14.0 * 60.0 && champion.kills >= champion.deaths + 2);
    if team_tactics.jungle_style == "Farmer" {
        for key in
            jungler_macro_jungle_priority_for_team(&champion.team, &team_tactics.jungle_pathing)
        {
            if is_enemy_jungle_camp_key_for_team(key, &champion.team) && !can_hard_invade {
                continue;
            }
            let Some(timer) = neutral_timers.entities.get(key) else {
                continue;
            };
            if !timer.unlocked {
                continue;
            }
            if timer.alive {
                return Some(timer.pos);
            }
            if let Some(next_spawn_at) = timer.next_spawn_at {
                if next_spawn_at >= now && next_spawn_at - now <= objective_lead_time {
                    return Some(timer.pos);
                }
            }
        }
    }
    for key in side_objective_order {
        let Some(timer) = neutral_timers.entities.get(key) else {
            continue;
        };
        if !timer.unlocked {
            continue;
        }
        if timer.alive {
            return Some(timer.pos);
        }
        if let Some(next_spawn_at) = timer.next_spawn_at {
            if next_spawn_at >= now && next_spawn_at - now <= objective_lead_time {
                return Some(timer.pos);
            }
        }
    }
    for key in jungler_macro_jungle_priority_for_team(&champion.team, &team_tactics.jungle_pathing)
    {
        if is_enemy_jungle_camp_key_for_team(key, &champion.team) && !can_hard_invade {
            continue;
        }
        let Some(timer) = neutral_timers.entities.get(key) else {
            continue;
        };
        if !timer.unlocked {
            continue;
        }
        if timer.alive {
            return Some(timer.pos);
        }
        if let Some(next_spawn_at) = timer.next_spawn_at {
            if next_spawn_at >= now && next_spawn_at - now <= objective_lead_time {
                return Some(timer.pos);
            }
        }
    }
    None
}

pub(super) fn jungler_macro_jungle_priority_for_team(
    team: &str,
    jungle_pathing: &str,
) -> Vec<&'static str> {
    let (own_top, own_bot, enemy_top, enemy_bot): ([&str; 3], [&str; 3], [&str; 3], [&str; 3]) =
        if normalized_team(team) == "red" {
            (
                ["gromp-red", "blue-buff-red", "wolves-red"],
                ["krugs-red", "red-buff-red", "raptors-red"],
                ["blue-buff-blue", "wolves-blue", "gromp-blue"],
                ["red-buff-blue", "raptors-blue", "krugs-blue"],
            )
        } else {
            (
                ["gromp-blue", "blue-buff-blue", "wolves-blue"],
                ["krugs-blue", "red-buff-blue", "raptors-blue"],
                ["blue-buff-red", "wolves-red", "gromp-red"],
                ["red-buff-red", "raptors-red", "krugs-red"],
            )
        };
    if jungle_pathing == "BotToTop" {
        vec![
            own_bot[0],
            own_bot[1],
            own_bot[2],
            "scuttle-bot",
            own_top[0],
            own_top[1],
            own_top[2],
            "scuttle-top",
            enemy_top[0],
            enemy_top[1],
            enemy_top[2],
            enemy_bot[0],
            enemy_bot[1],
            enemy_bot[2],
        ]
    } else {
        vec![
            own_top[0],
            own_top[1],
            own_top[2],
            "scuttle-top",
            own_bot[0],
            own_bot[1],
            own_bot[2],
            "scuttle-bot",
            enemy_bot[0],
            enemy_bot[1],
            enemy_bot[2],
            enemy_top[0],
            enemy_top[1],
            enemy_top[2],
        ]
    }
}
