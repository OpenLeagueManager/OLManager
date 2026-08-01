use super::{
    ChampionRuntime, LANE_CHAMPION_TRADE_RADIUS, LANE_CHASE_MINION_CONTEXT_RADIUS,
    LANE_LOCAL_PRESSURE_RADIUS, LANE_MINION_CONTEXT_RADIUS, LanerCombatStateRuntime, MinionRuntime,
    SimulatorAiMode, SimulatorPolicyConfig, StructureRuntime, TRADE_SCORE_WEIGHT_BIAS,
    TRADE_SCORE_WEIGHT_CHAMP_NUMBERS, TRADE_SCORE_WEIGHT_ENEMY_HP,
    TRADE_SCORE_WEIGHT_ENEMY_OVEREXTENDED, TRADE_SCORE_WEIGHT_FIRST_WAVE,
    TRADE_SCORE_WEIGHT_MINION_NUMBERS, TRADE_SCORE_WEIGHT_SELF_HP,
    TRADE_SCORE_WEIGHT_TOWER_DISTANCE, TradeConfidenceFeatures, TradeDecisionEvaluation, Vec2,
    clamp_ratio_01, closest_lane_path_index, dist, is_first_wave_contest_active, lane_anchor_pos,
    lane_minion_context_distance, lane_path_for, lane_pressure_at, lane_recent_trade_lock_active,
    lane_role_profile, lane_trade_cooldown_active, lane_wave_front_pos, normalized_lane,
    normalized_team, should_force_laner_disengage, sigmoid,
};
use std::collections::HashMap;

pub(super) fn evaluate_open_trade_window(
    champion: &ChampionRuntime,
    enemy: &ChampionRuntime,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
    lane_combat_state_by_champion: &HashMap<String, LanerCombatStateRuntime>,
    ai_mode: SimulatorAiMode,
    policy: &SimulatorPolicyConfig,
) -> TradeDecisionEvaluation {
    if champion.role == "JGL" {
        let self_hp = if champion.max_hp <= 0.0 {
            1.0
        } else {
            champion.hp / champion.max_hp
        };
        let enemy_hp = if enemy.max_hp <= 0.0 {
            1.0
        } else {
            enemy.hp / enemy.max_hp
        };
        let pressure = lane_pressure_at(
            champion,
            enemy.pos,
            champions,
            minions,
            LANE_LOCAL_PRESSURE_RADIUS,
        );
        let can_force = self_hp >= 0.42
            && (enemy_hp <= 0.50
                || pressure.ally_champions >= pressure.enemy_champions
                || pressure.ally_score >= pressure.enemy_score + 0.2);
        return TradeDecisionEvaluation {
            decision: can_force,
            rule_decision: can_force,
            confidence: if can_force { 0.9 } else { 0.1 },
            flipped_by_hybrid: false,
        };
    }
    if dist(champion.pos, enemy.pos) > LANE_CHAMPION_TRADE_RADIUS {
        return TradeDecisionEvaluation {
            decision: false,
            rule_decision: false,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }
    if !in_lane_trade_context(
        champion,
        champion.pos,
        false,
        champions,
        minions,
        structures,
    ) {
        return TradeDecisionEvaluation {
            decision: false,
            rule_decision: false,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }
    if !in_lane_trade_context(champion, enemy.pos, true, champions, minions, structures) {
        return TradeDecisionEvaluation {
            decision: false,
            rule_decision: false,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }
    if should_force_laner_disengage(
        champion,
        enemy.pos,
        Some(enemy),
        champions,
        minions,
        structures,
    ) {
        return TradeDecisionEvaluation {
            decision: false,
            rule_decision: false,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }
    let clear_win_condition = should_commit_all_in_trade(champion, enemy, champions, minions);
    if (lane_trade_cooldown_active(champion, now, lane_combat_state_by_champion)
        || lane_recent_trade_lock_active(champion, now, lane_combat_state_by_champion))
        && !clear_win_condition
    {
        return TradeDecisionEvaluation {
            decision: false,
            rule_decision: false,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }

    let hp_ratio = if champion.max_hp <= 0.0 {
        1.0
    } else {
        champion.hp / champion.max_hp
    };
    let enemy_hp_ratio = if enemy.max_hp <= 0.0 {
        1.0
    } else {
        enemy.hp / enemy.max_hp
    };

    let pressure = lane_pressure_at(
        champion,
        enemy.pos,
        champions,
        minions,
        LANE_LOCAL_PRESSURE_RADIUS,
    );
    let numbers_advantage = pressure.ally_champions > pressure.enemy_champions;
    if numbers_advantage && hp_ratio + 0.02 >= enemy_hp_ratio && hp_ratio >= 0.32 {
        return TradeDecisionEvaluation {
            decision: true,
            rule_decision: true,
            confidence: 1.0,
            flipped_by_hybrid: false,
        };
    }

    let ally_minions_near_fight = minions
        .iter()
        .filter(|m| {
            m.alive
                && normalized_team(&m.team) == normalized_team(&champion.team)
                && normalized_lane(&m.lane) == normalized_lane(&champion.lane)
                && dist(m.pos, enemy.pos) <= 0.1
        })
        .count();
    let enemy_minions_near_fight = minions
        .iter()
        .filter(|m| {
            m.alive
                && normalized_team(&m.team) != normalized_team(&champion.team)
                && normalized_lane(&m.lane) == normalized_lane(&champion.lane)
                && dist(m.pos, enemy.pos) <= 0.1
        })
        .count();

    let total_wave_context = ally_minions_near_fight + enemy_minions_near_fight;
    if total_wave_context < 1 {
        return TradeDecisionEvaluation {
            decision: false,
            rule_decision: false,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }
    if is_first_wave_contest_active(champion, now)
        && (ally_minions_near_fight < 2 || enemy_minions_near_fight < 2)
    {
        return TradeDecisionEvaluation {
            decision: false,
            rule_decision: false,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }
    if ally_minions_near_fight == 0 {
        let low_enemy_window = enemy_hp_ratio <= 0.34;
        let hp_safe_to_trade = hp_ratio >= 0.5;
        if !(low_enemy_window && hp_safe_to_trade) {
            return TradeDecisionEvaluation {
                decision: false,
                rule_decision: false,
                confidence: 0.0,
                flipped_by_hybrid: false,
            };
        }
    }

    let hp_advantage = hp_ratio + 0.08 >= enemy_hp_ratio;
    let wave_pressure = pressure.ally_lane_minions >= pressure.enemy_lane_minions;
    let score_pressure = pressure.ally_score >= pressure.enemy_score - 0.05;
    let rule_decision = hp_advantage && wave_pressure && score_pressure;

    if ai_mode != SimulatorAiMode::Hybrid {
        return TradeDecisionEvaluation {
            decision: rule_decision,
            rule_decision,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }

    let features = trade_confidence_features(champion, enemy, now, champions, minions, structures);
    let confidence = calibrate_trade_confidence(trade_confidence_score(features));
    let hp_gap = enemy_hp_ratio - (hp_ratio + 0.08);
    let wave_gap = pressure.enemy_lane_minions as i64 - pressure.ally_lane_minions as i64;
    let score_gap = pressure.enemy_score - (pressure.ally_score + 0.05);
    let borderline_reject = !rule_decision && hp_gap <= 0.08 && wave_gap <= 2 && score_gap <= 0.35;
    let hybrid_decision = rule_decision
        || (borderline_reject && confidence >= policy.hybrid_open_trade_confidence_high);

    TradeDecisionEvaluation {
        decision: hybrid_decision,
        rule_decision,
        confidence,
        flipped_by_hybrid: hybrid_decision != rule_decision,
    }
}

pub(super) fn evaluate_disengage_champion_trade(
    champion: &ChampionRuntime,
    enemy: &ChampionRuntime,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
    ai_mode: SimulatorAiMode,
    policy: &SimulatorPolicyConfig,
) -> TradeDecisionEvaluation {
    if champion.role == "JGL" {
        let self_hp_ratio = if champion.max_hp <= 0.0 {
            1.0
        } else {
            champion.hp / champion.max_hp
        };
        let enemy_hp_ratio = if enemy.max_hp <= 0.0 {
            1.0
        } else {
            enemy.hp / enemy.max_hp
        };
        let should_back_off = self_hp_ratio < 0.30 || self_hp_ratio + 0.02 < enemy_hp_ratio;
        return TradeDecisionEvaluation {
            decision: should_back_off,
            rule_decision: should_back_off,
            confidence: 1.0,
            flipped_by_hybrid: false,
        };
    }

    if should_force_laner_disengage(
        champion,
        enemy.pos,
        Some(enemy),
        champions,
        minions,
        structures,
    ) {
        return TradeDecisionEvaluation {
            decision: true,
            rule_decision: true,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }

    let self_hp_ratio = if champion.max_hp <= 0.0 {
        1.0
    } else {
        champion.hp / champion.max_hp
    };
    let enemy_hp_ratio = if enemy.max_hp <= 0.0 {
        1.0
    } else {
        enemy.hp / enemy.max_hp
    };
    if self_hp_ratio < policy.trade_retreat_hp_ratio {
        return TradeDecisionEvaluation {
            decision: true,
            rule_decision: true,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }
    if self_hp_ratio + policy.trade_hp_disadvantage_allowance < enemy_hp_ratio {
        return TradeDecisionEvaluation {
            decision: true,
            rule_decision: true,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }

    let ally_champions = champions
        .iter()
        .filter(|u| {
            u.alive
                && normalized_team(&u.team) == normalized_team(&champion.team)
                && dist(u.pos, enemy.pos) <= 0.11
        })
        .count();
    let enemy_champions = champions
        .iter()
        .filter(|u| {
            u.alive
                && normalized_team(&u.team) != normalized_team(&champion.team)
                && dist(u.pos, enemy.pos) <= 0.11
        })
        .count();
    let ally_lane_minions = minions
        .iter()
        .filter(|m| {
            m.alive
                && normalized_team(&m.team) == normalized_team(&champion.team)
                && normalized_lane(&m.lane) == normalized_lane(&champion.lane)
                && dist(m.pos, enemy.pos) <= 0.085
        })
        .count();
    let enemy_lane_minions = minions
        .iter()
        .filter(|m| {
            m.alive
                && normalized_team(&m.team) != normalized_team(&champion.team)
                && normalized_lane(&m.lane) == normalized_lane(&champion.lane)
                && dist(m.pos, enemy.pos) <= 0.085
        })
        .count();

    let allied_pressure = ally_champions as f64 + ally_lane_minions as f64 * 0.5;
    let enemy_pressure = enemy_champions as f64 + enemy_lane_minions as f64 * 0.5;
    if enemy_pressure > allied_pressure + 1.05 {
        return TradeDecisionEvaluation {
            decision: true,
            rule_decision: true,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }

    let lane_anchor = lane_anchor_pos(champion, minions, structures);
    let rule_decision = dist(enemy.pos, lane_anchor) > policy.lane_chase_leash_radius
        && enemy_pressure >= allied_pressure;
    if ai_mode != SimulatorAiMode::Hybrid {
        return TradeDecisionEvaluation {
            decision: rule_decision,
            rule_decision,
            confidence: 0.0,
            flipped_by_hybrid: false,
        };
    }

    let features = trade_confidence_features(champion, enemy, now, champions, minions, structures);
    let confidence = calibrate_trade_confidence(trade_confidence_score(features));
    let pressure_margin = enemy_pressure - (allied_pressure + 0.7);
    let hp_margin = (self_hp_ratio + policy.trade_hp_disadvantage_allowance) - enemy_hp_ratio;
    let leash_margin = dist(enemy.pos, lane_anchor) - policy.lane_chase_leash_radius;
    let borderline_risk = !rule_decision
        && (pressure_margin > -0.2 || hp_margin < 0.04 || leash_margin > -0.008)
        && (self_hp_ratio < policy.trade_retreat_hp_ratio + 0.08);
    let hybrid_decision =
        rule_decision || (borderline_risk && confidence <= policy.hybrid_disengage_confidence_low);

    TradeDecisionEvaluation {
        decision: hybrid_decision,
        rule_decision,
        confidence,
        flipped_by_hybrid: hybrid_decision != rule_decision,
    }
}

pub(super) fn nearest_enemy_lane_tower_distance(
    champion: &ChampionRuntime,
    target_pos: Vec2,
    structures: &[StructureRuntime],
) -> f64 {
    structures
        .iter()
        .filter(|tower| {
            tower.alive
                && normalized_team(&tower.team) != normalized_team(&champion.team)
                && tower.kind == "TOWER"
                && normalized_lane(&tower.lane) == normalized_lane(&champion.lane)
        })
        .map(|tower| dist(tower.pos, target_pos))
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.4)
}

pub(super) fn enemy_overextended_in_lane(
    champion: &ChampionRuntime,
    enemy: &ChampionRuntime,
) -> bool {
    let lane_path = lane_path_for(&champion.team, &champion.lane);
    if lane_path.len() < 2 {
        return false;
    }
    let enemy_idx = closest_lane_path_index(enemy.pos, &lane_path);
    let overextended_max_idx = lane_path.len().saturating_sub(1).min(2);
    enemy_idx <= overextended_max_idx
}

pub(super) fn should_commit_all_in_trade(
    champion: &ChampionRuntime,
    enemy: &ChampionRuntime,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
) -> bool {
    if champion.role == "JGL" {
        return true;
    }

    let self_hp = if champion.max_hp <= 0.0 {
        1.0
    } else {
        champion.hp / champion.max_hp
    };
    let enemy_hp = if enemy.max_hp <= 0.0 {
        1.0
    } else {
        enemy.hp / enemy.max_hp
    };

    if enemy_hp <= 0.2 && self_hp >= 0.25 {
        return true;
    }

    let pressure = lane_pressure_at(
        champion,
        enemy.pos,
        champions,
        minions,
        LANE_LOCAL_PRESSURE_RADIUS,
    );
    if pressure.ally_champions > pressure.enemy_champions && self_hp >= 0.32 {
        return true;
    }

    pressure.ally_score >= pressure.enemy_score + 0.65 && self_hp >= enemy_hp
}

fn trade_confidence_score(features: TradeConfidenceFeatures) -> f64 {
    let champion_numbers = clamp_ratio_01(
        (features.ally_champions_local as f64 - features.enemy_champions_local as f64 + 2.0) / 4.0,
    );
    let minion_numbers = clamp_ratio_01(
        (features.ally_minions_local as f64 - features.enemy_minions_local as f64 + 5.0) / 10.0,
    );
    let enemy_tower_distance_norm = clamp_ratio_01(features.nearest_enemy_tower_distance / 0.18);
    let enemy_overextended = if features.enemy_overextended {
        1.0
    } else {
        0.0
    };
    let first_wave_window = if features.first_wave_window { 1.0 } else { 0.0 };

    let logit = TRADE_SCORE_WEIGHT_BIAS
        + TRADE_SCORE_WEIGHT_SELF_HP * clamp_ratio_01(features.self_hp_ratio)
        + TRADE_SCORE_WEIGHT_ENEMY_HP * clamp_ratio_01(features.enemy_hp_ratio)
        + TRADE_SCORE_WEIGHT_CHAMP_NUMBERS * champion_numbers
        + TRADE_SCORE_WEIGHT_MINION_NUMBERS * minion_numbers
        + TRADE_SCORE_WEIGHT_TOWER_DISTANCE * enemy_tower_distance_norm
        + TRADE_SCORE_WEIGHT_ENEMY_OVEREXTENDED * enemy_overextended
        + TRADE_SCORE_WEIGHT_FIRST_WAVE * first_wave_window;

    clamp_ratio_01(sigmoid(logit))
}

fn calibrate_trade_confidence(raw_confidence: f64) -> f64 {
    let raw = clamp_ratio_01(raw_confidence);

    if raw <= 0.7 {
        return raw;
    }

    0.7 + (raw - 0.7) * 0.35
}

fn trade_confidence_features(
    champion: &ChampionRuntime,
    enemy: &ChampionRuntime,
    now: f64,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> TradeConfidenceFeatures {
    let self_hp_ratio = if champion.max_hp <= 0.0 {
        1.0
    } else {
        champion.hp / champion.max_hp
    };
    let enemy_hp_ratio = if enemy.max_hp <= 0.0 {
        1.0
    } else {
        enemy.hp / enemy.max_hp
    };

    let pressure = lane_pressure_at(
        champion,
        enemy.pos,
        champions,
        minions,
        LANE_LOCAL_PRESSURE_RADIUS,
    );
    let nearest_enemy_tower_distance =
        nearest_enemy_lane_tower_distance(champion, enemy.pos, structures);

    TradeConfidenceFeatures {
        self_hp_ratio,
        enemy_hp_ratio,
        ally_champions_local: pressure.ally_champions,
        enemy_champions_local: pressure.enemy_champions,
        ally_minions_local: pressure.ally_lane_minions,
        enemy_minions_local: pressure.enemy_lane_minions,
        nearest_enemy_tower_distance,
        enemy_overextended: enemy_overextended_in_lane(champion, enemy),
        first_wave_window: is_first_wave_contest_active(champion, now),
    }
}

pub(super) fn in_lane_trade_context(
    champion: &ChampionRuntime,
    pos: super::Vec2,
    for_chase: bool,
    champions: &[ChampionRuntime],
    minions: &[MinionRuntime],
    structures: &[StructureRuntime],
) -> bool {
    if champion.role == "JGL" {
        return true;
    }
    let Some(profile) = lane_role_profile(champion) else {
        return true;
    };

    let lane_anchor = lane_anchor_pos(champion, minions, structures);
    let wave_front = lane_wave_front_pos(champion, minions, structures);
    let _local_pressure = lane_pressure_at(
        champion,
        pos,
        champions,
        minions,
        LANE_LOCAL_PRESSURE_RADIUS,
    );

    let mid_context_mult = if champion.role == "MID" { 1.18 } else { 1.0 };
    let anchor_budget = profile.chase_leash
        * if for_chase {
            1.05 * mid_context_mult
        } else {
            0.92 * mid_context_mult
        };
    let wave_budget = profile.chase_leash
        * if for_chase {
            1.15 * mid_context_mult
        } else {
            1.0 * mid_context_mult
        };
    let minion_budget = if for_chase {
        LANE_CHASE_MINION_CONTEXT_RADIUS
    } else {
        LANE_MINION_CONTEXT_RADIUS
    } * mid_context_mult;

    if dist(pos, lane_anchor) > anchor_budget {
        return false;
    }
    if dist(pos, wave_front) > wave_budget {
        return false;
    }
    if lane_minion_context_distance(champion, pos, minions) > minion_budget {
        return false;
    }
    true
}
