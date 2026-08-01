use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::f64::consts::PI;
use std::sync::OnceLock;

use crate::engine::event::{EventType, MatchEvent};
use crate::engine::types::{Side, TeamData, Zone};

use super::{LiveMatchState, MatchPhase};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LolDragonKind {
    Infernal,
    Ocean,
    Mountain,
    Cloud,
    Hextech,
    Chemtech,
    Elder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LolObjectiveState {
    pub alive: bool,
    pub next_spawn_minute: Option<u8>,
    pub last_taken_by: Option<Side>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LolDragonState {
    pub alive: bool,
    pub next_spawn_minute: Option<u8>,
    pub current_kind: Option<LolDragonKind>,
    pub first_kind: Option<LolDragonKind>,
    pub second_kind: Option<LolDragonKind>,
    pub soul_rift_kind: Option<LolDragonKind>,
    pub soul_claimed_by: Option<Side>,
    pub home_stacks: u8,
    pub away_stacks: u8,
    pub last_taken_by: Option<Side>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LolGrubsState {
    pub alive: bool,
    pub next_spawn_minute: Option<u8>,
    pub waves_taken: u8,
    pub last_taken_by: Option<Side>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LolObjectivesState {
    pub dragon: LolDragonState,
    pub baron: LolObjectiveState,
    pub herald: LolObjectiveState,
    pub grubs: LolGrubsState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LolLaneState {
    pub outer_alive: bool,
    pub outer_hp: f64,
    pub inner_alive: bool,
    pub inner_hp: f64,
    pub inhibitor_alive: bool,
    pub inhibitor_hp: f64,
    pub inhibitor_respawn_minute: Option<u8>,
    #[serde(default = "default_wave")]
    pub blue_wave: f64,
    #[serde(default = "default_wave")]
    pub red_wave: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LolTeamStructuresState {
    pub top: LolLaneState,
    pub mid: LolLaneState,
    pub bot: LolLaneState,
    pub nexus_tower_top_alive: bool,
    pub nexus_tower_top_hp: f64,
    pub nexus_tower_bot_alive: bool,
    pub nexus_tower_bot_hp: f64,
    pub nexus_alive: bool,
    pub nexus_hp: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LolMapState {
    pub objectives: LolObjectivesState,
    pub blue: LolTeamStructuresState,
    pub red: LolTeamStructuresState,
    pub destroyed_nexus_by: Option<Side>,
    pub units: Vec<LolUnitState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LolRole {
    Top,
    Jungle,
    Mid,
    Adc,
    Support,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LolTask {
    MoveToLane,
    JungleClear,
    HoldLane,
    RotateObjective,
    Recall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LolUnitState {
    pub player_id: String,
    pub side: Side,
    pub role: LolRole,
    pub task: LolTask,
    pub x: f64,
    pub y: f64,
    pub target_x: f64,
    pub target_y: f64,
    pub path_index: u8,
    #[serde(default)]
    pub spawn_slot: u8,
    #[serde(default)]
    pub recall_available_minute: u8,
    pub alive: bool,
    pub respawn_minute: Option<u8>,
    #[serde(default = "default_unit_hp")]
    pub hp: f64,
    pub kills: u8,
    pub deaths: u8,
    #[serde(default = "default_level")]
    pub level: u8,
    #[serde(default)]
    pub xp: f64,
    #[serde(default)]
    pub gold: f64,
    #[serde(default)]
    pub damage_dealt: f64,
    #[serde(default)]
    pub item_tier: u8,
    /// Short-term energy (0-100). Affects damage dealt and received in fights.
    #[serde(default = "default_condition")]
    pub condition: u8,
}

fn default_unit_hp() -> f64 {
    100.0
}

fn default_wave() -> f64 {
    1.0
}

fn default_condition() -> u8 {
    100
}

fn default_level() -> u8 {
    1
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LaneKey {
    Top,
    Mid,
    Bot,
}

#[derive(Clone, Copy)]
enum StructureTarget {
    Outer(LaneKey),
    Inner(LaneKey),
    Inhib(LaneKey),
    NexusTop,
    NexusBot,
    Nexus,
}

#[derive(Deserialize, Clone)]
struct WallPoint {
    x: f64,
    y: f64,
}

#[derive(Deserialize, Clone)]
struct WallPolygon {
    id: String,
    closed: bool,
    points: Vec<WallPoint>,
}

#[derive(Deserialize)]
struct WallFile {
    walls: Vec<WallPolygon>,
}

impl LolMapState {
    pub fn new() -> Self {
        let lane = || LolLaneState {
            outer_alive: true,
            outer_hp: 1200.0,
            inner_alive: true,
            inner_hp: 1400.0,
            inhibitor_alive: true,
            inhibitor_hp: 1200.0,
            inhibitor_respawn_minute: None,
            blue_wave: 1.0,
            red_wave: 1.0,
        };

        let team_structures = || LolTeamStructuresState {
            top: lane(),
            mid: lane(),
            bot: lane(),
            nexus_tower_top_alive: true,
            nexus_tower_top_hp: 1500.0,
            nexus_tower_bot_alive: true,
            nexus_tower_bot_hp: 1500.0,
            nexus_alive: true,
            nexus_hp: 2500.0,
        };

        Self {
            objectives: LolObjectivesState {
                dragon: LolDragonState {
                    alive: false,
                    next_spawn_minute: Some(5),
                    current_kind: None,
                    first_kind: None,
                    second_kind: None,
                    soul_rift_kind: None,
                    soul_claimed_by: None,
                    home_stacks: 0,
                    away_stacks: 0,
                    last_taken_by: None,
                },
                baron: LolObjectiveState {
                    alive: false,
                    next_spawn_minute: Some(20),
                    last_taken_by: None,
                },
                herald: LolObjectiveState {
                    alive: false,
                    next_spawn_minute: Some(14),
                    last_taken_by: None,
                },
                grubs: LolGrubsState {
                    alive: false,
                    next_spawn_minute: Some(6),
                    waves_taken: 0,
                    last_taken_by: None,
                },
            },
            blue: team_structures(),
            red: team_structures(),
            destroyed_nexus_by: None,
            units: Vec::new(),
        }
    }

    pub fn seed_units(&mut self, home: &TeamData, away: &TeamData) {
        self.units.clear();
        let roles = [
            LolRole::Top,
            LolRole::Jungle,
            LolRole::Mid,
            LolRole::Adc,
            LolRole::Support,
        ];

        for (idx, player) in home.players.iter().take(5).enumerate() {
            let role = roles[idx];
            let spawn = blue_spawn(idx);
            self.units.push(LolUnitState {
                player_id: player.id.clone(),
                side: Side::Home,
                role,
                task: LolTask::MoveToLane,
                x: spawn.0,
                y: spawn.1,
                target_x: spawn.0,
                target_y: spawn.1,
                path_index: 0,
                spawn_slot: idx as u8,
                recall_available_minute: 0,
                alive: true,
                respawn_minute: None,
                hp: 100.0,
                kills: 0,
                deaths: 0,
                level: 1,
                xp: 0.0,
                gold: 500.0,
                damage_dealt: 0.0,
                item_tier: 0,
                condition: player.condition,
            });
        }

        for (idx, player) in away.players.iter().take(5).enumerate() {
            let role = roles[idx];
            let spawn = red_spawn(idx);
            self.units.push(LolUnitState {
                player_id: player.id.clone(),
                side: Side::Away,
                role,
                task: LolTask::MoveToLane,
                x: spawn.0,
                y: spawn.1,
                target_x: spawn.0,
                target_y: spawn.1,
                path_index: 0,
                spawn_slot: idx as u8,
                recall_available_minute: 0,
                alive: true,
                respawn_minute: None,
                hp: 100.0,
                kills: 0,
                deaths: 0,
                level: 1,
                xp: 0.0,
                gold: 500.0,
                damage_dealt: 0.0,
                item_tier: 0,
                condition: player.condition,
            });
        }
    }
}

impl LiveMatchState {
    pub(super) fn step_lol_map<R: Rng>(
        &mut self,
        minute: u8,
        rng: &mut R,
        minute_events: &mut Vec<MatchEvent>,
    ) {
        if self.lol_map.destroyed_nexus_by.is_some() {
            self.phase = MatchPhase::Finished;
            return;
        }

        self.spawn_and_respawn(minute, rng, minute_events);
        self.tick_lane_waves(minute);
        self.tick_progression(minute);
        self.tick_movement(minute, rng);
        self.resolve_fights(minute, rng, minute_events);
        self.resolve_objectives(minute, rng, minute_events);
        self.resolve_structures(minute, rng, minute_events);
        self.update_possession();

        if self.lol_map.destroyed_nexus_by.is_some() {
            self.phase = MatchPhase::Finished;
        }
    }

    fn spawn_and_respawn<R: Rng>(&mut self, minute: u8, rng: &mut R, events: &mut Vec<MatchEvent>) {
        for unit in &mut self.lol_map.units {
            if unit.alive {
                continue;
            }
            if unit.respawn_minute.is_some_and(|m| minute >= m) {
                let spawn = if unit.side == Side::Home {
                    blue_spawn(unit.spawn_slot as usize)
                } else {
                    red_spawn(unit.spawn_slot as usize)
                };
                unit.alive = true;
                unit.respawn_minute = None;
                unit.hp = 100.0;
                unit.x = spawn.0;
                unit.y = spawn.1;
                unit.path_index = 0;
            }
        }

        self.lol_map.objectives.dragon.alive |= self
            .lol_map
            .objectives
            .dragon
            .next_spawn_minute
            .is_some_and(|m| minute >= m);
        self.lol_map.objectives.herald.alive |= self
            .lol_map
            .objectives
            .herald
            .next_spawn_minute
            .is_some_and(|m| minute >= m)
            && minute < 20;
        self.lol_map.objectives.grubs.alive |= self
            .lol_map
            .objectives
            .grubs
            .next_spawn_minute
            .is_some_and(|m| minute >= m)
            && minute < 20
            && self.lol_map.objectives.grubs.waves_taken < 3;
        self.lol_map.objectives.baron.alive |= self
            .lol_map
            .objectives
            .baron
            .next_spawn_minute
            .is_some_and(|m| minute >= m)
            && minute >= 20;

        if self.lol_map.objectives.dragon.alive
            && self.lol_map.objectives.dragon.current_kind.is_none()
        {
            self.lol_map.objectives.dragon.current_kind = Some(next_dragon_kind(rng));
            push_event(
                self,
                minute,
                EventType::ObjectiveSpawned,
                Side::Home,
                events,
            );
        }

        let mut inhibitor_respawn_events: Vec<Side> = Vec::new();
        for side in [Side::Home, Side::Away] {
            let team = if side == Side::Home {
                &mut self.lol_map.blue
            } else {
                &mut self.lol_map.red
            };
            for lane in [&mut team.top, &mut team.mid, &mut team.bot] {
                if !lane.inhibitor_alive
                    && lane
                        .inhibitor_respawn_minute
                        .is_some_and(|respawn| minute >= respawn)
                {
                    lane.inhibitor_alive = true;
                    lane.inhibitor_hp = 1200.0;
                    lane.inhibitor_respawn_minute = None;
                    inhibitor_respawn_events.push(side);
                }
            }
        }
        for side in inhibitor_respawn_events {
            push_event(self, minute, EventType::InhibitorRespawned, side, events);
        }
    }

    fn tick_lane_waves(&mut self, minute: u8) {
        let cadence = if minute < 14 {
            0.20
        } else if minute < 28 {
            0.28
        } else {
            0.36
        };

        for lane in [LaneKey::Top, LaneKey::Mid, LaneKey::Bot] {
            let home_anchor = lane_anchor(Side::Home, lane);
            let away_anchor = lane_anchor(Side::Away, lane);
            let home_presence = lane_presence(&self.lol_map.units, Side::Home, lane, home_anchor);
            let away_presence = lane_presence(&self.lol_map.units, Side::Away, lane, away_anchor);

            let (blue_wave, red_wave) = lane_waves_mut(&mut self.lol_map, lane);
            *blue_wave = (*blue_wave + cadence + home_presence * 0.05).clamp(0.0, 14.0);
            *red_wave = (*red_wave + cadence + away_presence * 0.05).clamp(0.0, 14.0);

            let crash = (*blue_wave).min(*red_wave);
            *blue_wave = (*blue_wave - crash * 0.90).max(0.0);
            *red_wave = (*red_wave - crash * 0.90).max(0.0);

            distribute_wave_income(
                &mut self.lol_map.units,
                Side::Home,
                home_anchor,
                crash * 32.0,
            );
            distribute_wave_income(
                &mut self.lol_map.units,
                Side::Away,
                away_anchor,
                crash * 32.0,
            );
        }
    }

    fn tick_progression(&mut self, minute: u8) {
        let passive_gold = if minute < 15 { 24.0 } else { 32.0 };
        for unit in &mut self.lol_map.units {
            if !unit.alive {
                continue;
            }
            unit.gold += passive_gold;
            unit.xp += 16.0;
            sync_progress(unit);
        }
    }

    fn tick_movement<R: Rng>(&mut self, minute: u8, rng: &mut R) {
        let dragon_alive = self.lol_map.objectives.dragon.alive;
        let herald_alive = self.lol_map.objectives.herald.alive;
        let baron_alive = self.lol_map.objectives.baron.alive;
        let grubs_alive = self.lol_map.objectives.grubs.alive;

        for _ in 0..8 {
            for unit in &mut self.lol_map.units {
                if !unit.alive {
                    continue;
                }

                if unit.hp < 20.0 && minute >= unit.recall_available_minute {
                    unit.task = LolTask::Recall;
                    let base = if unit.side == Side::Home {
                        blue_spawn(unit.spawn_slot as usize)
                    } else {
                        red_spawn(unit.spawn_slot as usize)
                    };
                    move_unit_with_walls(unit, base, 0.012, rng);
                    if distance((unit.x, unit.y), base) < 0.02 {
                        unit.hp = 80.0;
                        unit.recall_available_minute = minute.saturating_add(6);
                    }
                    continue;
                }

                let route = role_route(
                    unit.side,
                    unit.role,
                    minute,
                    dragon_alive,
                    herald_alive,
                    baron_alive,
                    grubs_alive,
                );
                if route.is_empty() {
                    continue;
                }

                let idx = (unit.path_index as usize).min(route.len() - 1);
                let target = route[idx];
                unit.target_x = target.0;
                unit.target_y = target.1;

                let speed = movement_speed(unit.role, unit.level, unit.item_tier);
                move_unit_with_walls(unit, target, speed, rng);

                if distance((unit.x, unit.y), target) < 0.025 {
                    unit.path_index = ((idx + 1) % route.len()) as u8;
                }
            }
        }
    }

    fn resolve_fights<R: Rng>(&mut self, minute: u8, rng: &mut R, events: &mut Vec<MatchEvent>) {
        let n = self.lol_map.units.len();
        if n < 2 {
            return;
        }

        let snapshot: Vec<(Side, bool, f64, f64, LolRole, u8, u8)> = self
            .lol_map
            .units
            .iter()
            .map(|u| (u.side, u.alive, u.x, u.y, u.role, u.level, u.item_tier))
            .collect();

        let mut incoming = vec![0.0_f64; n];
        let mut killer = vec![None; n];
        let mut highest = vec![0.0_f64; n];

        for i in 0..n {
            let (side, alive, x, y, role, level, items) = snapshot[i];
            if !alive {
                continue;
            }

            let mut target = None;
            let mut best_dist = f64::MAX;
            for (j, (oside, oalive, ox, oy, ..)) in snapshot.iter().enumerate() {
                if i == j || !*oalive || *oside == side {
                    continue;
                }
                let d = distance((x, y), (*ox, *oy));
                if d < 0.095 && d < best_dist {
                    best_dist = d;
                    target = Some(j);
                }
            }
            let Some(victim) = target else { continue };

            // Condition scaling: tired players deal less damage and take more.
            let attacker_cond = self.lol_map.units[i].condition as f64 / 100.0;
            let attack_mult = 0.6 + attacker_cond * 0.4; // 0.6–1.0
            let victim_cond = self.lol_map.units[victim].condition as f64 / 100.0;
            let defense_mult = 1.4 - victim_cond * 0.4; // 1.0–1.4

            let scale = 1.0 + (level.saturating_sub(1) as f64) * 0.06 + items as f64 * 0.10;
            let draft_multiplier = 1.0
                + match side {
                    Side::Home => self.config.home_draft_modifier,
                    Side::Away => self.config.away_draft_modifier,
                };
            let damage = role_power(role)
                * scale
                * game_damage_scale(minute, self.config.late_game_damage_scale)
                * rng.random_range(0.88..1.16)
                * (1.0 - best_dist / 0.095).clamp(0.45, 1.0)
                * attack_mult
                * defense_mult;
            let damage = damage * draft_multiplier.clamp(0.965, 1.035);

            incoming[victim] += damage;
            if let Some(attacker) = self.lol_map.units.get_mut(i) {
                attacker.damage_dealt += damage;
            }
            if damage > highest[victim] {
                highest[victim] = damage;
                killer[victim] = Some(i);
            }
        }

        let mut kills = Vec::new();
        for i in 0..n {
            if !self.lol_map.units[i].alive || incoming[i] <= 0.0 {
                continue;
            }
            let unit = &mut self.lol_map.units[i];
            unit.hp = (unit.hp - incoming[i]).clamp(0.0, 100.0);
            if unit.hp <= 0.0 {
                unit.alive = false;
                unit.deaths = unit.deaths.saturating_add(1);
                unit.respawn_minute = Some(minute.saturating_add(respawn_time(minute)));
                if let Some(k) = killer[i] {
                    kills.push((k, i));
                }
            }
        }

        for (killer_idx, victim_idx) in kills {
            if killer_idx >= self.lol_map.units.len() || victim_idx >= self.lol_map.units.len() {
                continue;
            }
            let (killer_side, killer_id) = {
                let k = &mut self.lol_map.units[killer_idx];
                k.kills = k.kills.saturating_add(1);
                k.gold += 300.0;
                k.xp += 180.0;
                sync_progress(k);
                (k.side, k.player_id.clone())
            };

            let victim_id = self.lol_map.units[victim_idx].player_id.clone();
            let evt = MatchEvent::new(minute, EventType::Kill, killer_side, Zone::Midfield)
                .with_player(&killer_id)
                .with_secondary(&victim_id);
            self.events.push(evt.clone());
            events.push(evt);
        }

        for unit in &mut self.lol_map.units {
            if unit.alive {
                unit.hp = (unit.hp + 0.06).min(100.0);
            }
        }
    }

    fn resolve_objectives<R: Rng>(
        &mut self,
        minute: u8,
        rng: &mut R,
        events: &mut Vec<MatchEvent>,
    ) {
        let objective_points = [
            (
                "dragon",
                self.lol_map.objectives.dragon.alive,
                objective_anchor("dragon"),
                0.12,
            ),
            (
                "baron",
                self.lol_map.objectives.baron.alive,
                objective_anchor("baron"),
                0.12,
            ),
            (
                "herald",
                self.lol_map.objectives.herald.alive,
                objective_anchor("herald"),
                0.11,
            ),
            (
                "grubs",
                self.lol_map.objectives.grubs.alive,
                objective_anchor("grubs"),
                0.11,
            ),
        ];

        for (name, alive, anchor, radius) in objective_points {
            if !alive {
                continue;
            }
            let home = objective_presence(&self.lol_map.units, Side::Home, anchor, radius);
            let away = objective_presence(&self.lol_map.units, Side::Away, anchor, radius);
            let swing =
                rng.random_range(self.config.objective_swing_min..self.config.objective_swing_max);

            let taker = if home * swing > away + 0.9 {
                Some(Side::Home)
            } else if away * swing > home + 0.9 {
                Some(Side::Away)
            } else {
                None
            };

            let Some(side) = taker else { continue };
            match name {
                "dragon" => {
                    self.lol_map.objectives.dragon.alive = false;
                    self.lol_map.objectives.dragon.next_spawn_minute =
                        Some(minute.saturating_add(5));
                    self.lol_map.objectives.dragon.last_taken_by = Some(side);
                    if side == Side::Home {
                        self.lol_map.objectives.dragon.home_stacks =
                            self.lol_map.objectives.dragon.home_stacks.saturating_add(1);
                    } else {
                        self.lol_map.objectives.dragon.away_stacks =
                            self.lol_map.objectives.dragon.away_stacks.saturating_add(1);
                    }
                }
                "baron" => {
                    self.lol_map.objectives.baron.alive = false;
                    self.lol_map.objectives.baron.next_spawn_minute =
                        Some(minute.saturating_add(6));
                    self.lol_map.objectives.baron.last_taken_by = Some(side);
                }
                "herald" => {
                    self.lol_map.objectives.herald.alive = false;
                    self.lol_map.objectives.herald.next_spawn_minute = None;
                    self.lol_map.objectives.herald.last_taken_by = Some(side);
                }
                _ => {
                    self.lol_map.objectives.grubs.alive = false;
                    self.lol_map.objectives.grubs.waves_taken =
                        self.lol_map.objectives.grubs.waves_taken.saturating_add(1);
                    self.lol_map.objectives.grubs.last_taken_by = Some(side);
                    self.lol_map.objectives.grubs.next_spawn_minute =
                        if self.lol_map.objectives.grubs.waves_taken < 3 {
                            Some(minute.saturating_add(4))
                        } else {
                            None
                        };
                }
            }

            grant_objective_rewards(&mut self.lol_map.units, side, anchor);
            push_event(self, minute, EventType::ObjectiveTaken, side, events);
        }
    }

    fn resolve_structures<R: Rng>(
        &mut self,
        minute: u8,
        rng: &mut R,
        events: &mut Vec<MatchEvent>,
    ) {
        for attacker in [Side::Home, Side::Away] {
            for lane in [LaneKey::Top, LaneKey::Mid, LaneKey::Bot] {
                let pressure = lane_pressure(&self.lol_map, attacker, lane, minute);
                if pressure < 0.25 {
                    continue;
                }

                let Some(target) = next_target(&self.lol_map, attacker, lane) else {
                    continue;
                };

                let scaling = team_scaling(&self.lol_map.units, attacker);
                let dmg = pressure
                    * scaling
                    * rng.random_range(
                        self.config.structure_damage_min..self.config.structure_damage_max,
                    );
                if !deal_structure_damage(&mut self.lol_map, attacker, target, dmg, minute) {
                    continue;
                }

                let event_type = match target {
                    StructureTarget::Outer(_) | StructureTarget::Inner(_) => {
                        EventType::TowerDestroyed
                    }
                    StructureTarget::Inhib(_) => EventType::InhibitorDestroyed,
                    StructureTarget::NexusTop | StructureTarget::NexusBot => {
                        EventType::NexusTowerDestroyed
                    }
                    StructureTarget::Nexus => EventType::NexusDestroyed,
                };
                push_event(self, minute, event_type, attacker, events);

                if matches!(target, StructureTarget::Nexus) {
                    self.lol_map.destroyed_nexus_by = Some(attacker);
                    self.add_score(attacker);
                    self.phase = MatchPhase::Finished;
                    return;
                }
            }
        }
    }

    fn update_possession(&mut self) {
        let home = self
            .lol_map
            .units
            .iter()
            .filter(|u| u.alive && u.side == Side::Home)
            .count();
        let away = self
            .lol_map
            .units
            .iter()
            .filter(|u| u.alive && u.side == Side::Away)
            .count();

        if home >= away {
            self.possession = Side::Home;
            self.home_possession_ticks = self.home_possession_ticks.saturating_add(1);
        } else {
            self.possession = Side::Away;
            self.away_possession_ticks = self.away_possession_ticks.saturating_add(1);
        }
    }
}

fn blue_spawn(slot: usize) -> (f64, f64) {
    const P: [(f64, f64); 5] = [
        (0.14, 0.90),
        (0.18, 0.88),
        (0.22, 0.86),
        (0.18, 0.93),
        (0.24, 0.91),
    ];
    P[slot.min(4)]
}

fn red_spawn(slot: usize) -> (f64, f64) {
    const P: [(f64, f64); 5] = [
        (0.86, 0.10),
        (0.82, 0.12),
        (0.78, 0.14),
        (0.82, 0.07),
        (0.76, 0.09),
    ];
    P[slot.min(4)]
}

fn lane_anchor(side: Side, lane: LaneKey) -> (f64, f64) {
    match (side, lane) {
        (Side::Home, LaneKey::Top) => (0.30, 0.20),
        (Side::Home, LaneKey::Mid) => (0.47, 0.53),
        (Side::Home, LaneKey::Bot) => (0.30, 0.78),
        (Side::Away, LaneKey::Top) => (0.70, 0.80),
        (Side::Away, LaneKey::Mid) => (0.53, 0.47),
        (Side::Away, LaneKey::Bot) => (0.70, 0.22),
    }
}

fn objective_anchor(name: &str) -> (f64, f64) {
    match name {
        "baron" => (0.3274739583333333, 0.2981770833333333),
        "dragon" => (0.673828125, 0.703125),
        "grubs" => (0.3313802083333333, 0.2994791666666667),
        "herald" => (0.3274739583333333, 0.2942708333333333),
        _ => (0.5, 0.5),
    }
}

fn role_route(
    side: Side,
    role: LolRole,
    minute: u8,
    dragon_alive: bool,
    herald_alive: bool,
    baron_alive: bool,
    grubs_alive: bool,
) -> Vec<(f64, f64)> {
    match (side, role) {
        (_, LolRole::Top) => {
            let route = vec![(0.30, 0.20), (0.55, 0.14), (0.78, 0.10)];
            if side == Side::Home {
                route
            } else {
                route.into_iter().map(|(x, y)| (1.0 - x, 1.0 - y)).collect()
            }
        }
        (_, LolRole::Mid) => {
            let route = vec![(0.47, 0.53), (0.58, 0.42), (0.70, 0.30)];
            if side == Side::Home {
                route
            } else {
                route.into_iter().map(|(x, y)| (1.0 - x, 1.0 - y)).collect()
            }
        }
        (_, LolRole::Adc) => {
            let route = vec![(0.30, 0.78), (0.54, 0.84), (0.78, 0.90)];
            if side == Side::Home {
                route
            } else {
                route.into_iter().map(|(x, y)| (1.0 - x, 1.0 - y)).collect()
            }
        }
        (_, LolRole::Support) => {
            if minute < 14 {
                let route = vec![(0.33, 0.77), (0.52, 0.83), (0.75, 0.88)];
                if side == Side::Home {
                    route
                } else {
                    route.into_iter().map(|(x, y)| (1.0 - x, 1.0 - y)).collect()
                }
            } else if dragon_alive {
                vec![(0.46, 0.70), objective_anchor("dragon")]
            } else if baron_alive {
                vec![(0.44, 0.42), objective_anchor("baron")]
            } else {
                vec![(0.42, 0.60), (0.58, 0.42)]
            }
        }
        (_, LolRole::Jungle) => {
            if minute >= 20 && baron_alive {
                vec![(0.43, 0.43), objective_anchor("baron")]
            } else if minute >= 14 && herald_alive {
                vec![(0.40, 0.40), objective_anchor("herald")]
            } else if grubs_alive {
                vec![(0.37, 0.36), objective_anchor("grubs")]
            } else if dragon_alive {
                vec![(0.56, 0.56), objective_anchor("dragon")]
            } else {
                let route = vec![(0.25, 0.46), (0.26, 0.56), (0.48, 0.64), (0.53, 0.74)];
                if side == Side::Home {
                    route
                } else {
                    route.into_iter().map(|(x, y)| (1.0 - x, 1.0 - y)).collect()
                }
            }
        }
    }
}

fn movement_speed(role: LolRole, level: u8, item_tier: u8) -> f64 {
    let base = match role {
        LolRole::Top => 0.0068,
        LolRole::Jungle => 0.0078,
        LolRole::Mid => 0.0072,
        LolRole::Adc => 0.0070,
        LolRole::Support => 0.0072,
    };
    base * (1.0 + level.saturating_sub(1) as f64 * 0.004 + item_tier as f64 * 0.01)
}

fn role_power(role: LolRole) -> f64 {
    match role {
        LolRole::Top => 6.0,
        LolRole::Jungle => 5.8,
        LolRole::Mid => 7.0,
        LolRole::Adc => 7.3,
        LolRole::Support => 4.7,
    }
}

fn game_damage_scale(minute: u8, late_game_scale: f64) -> f64 {
    if minute < 10 {
        1.0
    } else if minute < 20 {
        1.16
    } else if minute < 30 {
        1.34
    } else {
        late_game_scale.clamp(1.40, 1.60)
    }
}

fn respawn_time(minute: u8) -> u8 {
    if minute < 10 {
        4
    } else if minute < 20 {
        6
    } else if minute < 30 {
        8
    } else if minute < 40 {
        10
    } else {
        12
    }
}

fn next_dragon_kind(rng: &mut impl Rng) -> LolDragonKind {
    let all = [
        LolDragonKind::Infernal,
        LolDragonKind::Ocean,
        LolDragonKind::Mountain,
        LolDragonKind::Cloud,
        LolDragonKind::Hextech,
        LolDragonKind::Chemtech,
    ];
    all[rng.random_range(0..all.len())]
}

fn lane_presence(units: &[LolUnitState], side: Side, lane: LaneKey, anchor: (f64, f64)) -> f64 {
    units
        .iter()
        .filter(|u| u.alive && u.side == side)
        .map(|u| {
            let dist = distance((u.x, u.y), anchor);
            if dist > 0.22 {
                return 0.0;
            }
            let role_weight = match (lane, u.role) {
                (LaneKey::Top, LolRole::Top) => 1.25,
                (LaneKey::Mid, LolRole::Mid) => 1.25,
                (LaneKey::Bot, LolRole::Adc) => 1.25,
                (_, LolRole::Support) => 0.85,
                (_, LolRole::Jungle) => 0.70,
                _ => 0.55,
            };
            role_weight * (1.0 - dist / 0.22)
        })
        .sum()
}

fn distribute_wave_income(
    units: &mut [LolUnitState],
    side: Side,
    anchor: (f64, f64),
    gold_pool: f64,
) {
    let mut idxs: Vec<usize> = units
        .iter()
        .enumerate()
        .filter(|(_, u)| u.alive && u.side == side && distance((u.x, u.y), anchor) < 0.24)
        .map(|(i, _)| i)
        .collect();

    if idxs.is_empty() {
        if let Some((idx, _)) = units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && u.side == side)
            .min_by(|(_, a), (_, b)| {
                distance((a.x, a.y), anchor)
                    .partial_cmp(&distance((b.x, b.y), anchor))
                    .unwrap_or(Ordering::Equal)
            })
        {
            idxs.push(idx);
        }
    }
    if idxs.is_empty() {
        return;
    }

    let per_unit = gold_pool / idxs.len() as f64;
    for idx in idxs {
        let unit = &mut units[idx];
        unit.gold += per_unit;
        unit.xp += per_unit * 0.9;
        sync_progress(unit);
    }
}

fn sync_progress(unit: &mut LolUnitState) {
    let mut level = 1u8;
    let mut req = 240.0;
    let mut xp = unit.xp;
    while level < 18 && xp >= req {
        xp -= req;
        level += 1;
        req *= 1.17;
    }
    unit.level = level;
    unit.item_tier = ((unit.gold / 1450.0).floor() as u8).min(6);
}

fn objective_presence(units: &[LolUnitState], side: Side, anchor: (f64, f64), radius: f64) -> f64 {
    units
        .iter()
        .filter(|u| u.alive && u.side == side)
        .map(|u| {
            let d = distance((u.x, u.y), anchor);
            if d > radius {
                0.0
            } else {
                (1.0 + u.level as f64 * 0.03 + u.item_tier as f64 * 0.06) * (1.0 - d / radius)
            }
        })
        .sum()
}

fn grant_objective_rewards(units: &mut [LolUnitState], side: Side, anchor: (f64, f64)) {
    for unit in units.iter_mut().filter(|u| u.alive && u.side == side) {
        let d = distance((unit.x, unit.y), anchor);
        if d > 0.20 {
            continue;
        }
        let share = (1.0 - d / 0.20).clamp(0.35, 1.0);
        unit.gold += 240.0 * share;
        unit.xp += 160.0 * share;
        sync_progress(unit);
    }
}

fn lane_pressure(state: &LolMapState, attacker: Side, lane: LaneKey, minute: u8) -> f64 {
    let target_anchor = structure_anchor(
        attacker,
        next_target(state, attacker, lane).unwrap_or(StructureTarget::Outer(lane)),
    );
    let champ_pressure: f64 = state
        .units
        .iter()
        .filter(|u| u.alive && u.side == attacker)
        .map(|u| {
            let d = distance((u.x, u.y), target_anchor);
            if d > 0.18 {
                return 0.0;
            }
            let role = match u.role {
                LolRole::Adc => 1.35,
                LolRole::Mid => 1.10,
                LolRole::Top => 1.05,
                LolRole::Jungle => 0.95,
                LolRole::Support => 0.80,
            };
            role * (1.0 + u.level as f64 * 0.03 + u.item_tier as f64 * 0.08) * (1.0 - d / 0.18)
        })
        .sum();

    let (blue_wave, red_wave) = lane_waves(state, lane);
    let wave_adv = if attacker == Side::Home {
        (blue_wave - red_wave).max(0.0)
    } else {
        (red_wave - blue_wave).max(0.0)
    };

    if wave_adv < 0.35 {
        return 0.0;
    }

    let time = if minute < 14 {
        0.70
    } else if minute < 28 {
        1.0
    } else {
        1.25
    };
    (champ_pressure * 0.28 + wave_adv * 0.16) * time
}

fn team_scaling(units: &[LolUnitState], side: Side) -> f64 {
    let mut sum = 0.0;
    let mut count = 0.0;
    for u in units.iter().filter(|u| u.alive && u.side == side) {
        sum += u.level as f64 + u.item_tier as f64 * 1.5;
        count += 1.0;
    }
    if count <= 0.0 {
        return 1.0;
    }
    0.85 + (sum / count) * 0.06
}

fn next_target(state: &LolMapState, attacker: Side, lane: LaneKey) -> Option<StructureTarget> {
    let enemy = if attacker == Side::Home {
        &state.red
    } else {
        &state.blue
    };
    let lane_state = match lane {
        LaneKey::Top => &enemy.top,
        LaneKey::Mid => &enemy.mid,
        LaneKey::Bot => &enemy.bot,
    };

    if lane_state.outer_alive {
        return Some(StructureTarget::Outer(lane));
    }
    if lane_state.inner_alive {
        return Some(StructureTarget::Inner(lane));
    }
    if lane_state.inhibitor_alive {
        return Some(StructureTarget::Inhib(lane));
    }
    if enemy.nexus_tower_top_alive {
        return Some(StructureTarget::NexusTop);
    }
    if enemy.nexus_tower_bot_alive {
        return Some(StructureTarget::NexusBot);
    }
    if enemy.nexus_alive {
        return Some(StructureTarget::Nexus);
    }
    None
}

fn structure_anchor(attacker: Side, target: StructureTarget) -> (f64, f64) {
    let blue = match target {
        StructureTarget::Outer(LaneKey::Top) => (0.186, 0.336),
        StructureTarget::Inner(LaneKey::Top) => (0.124, 0.279),
        StructureTarget::Inhib(LaneKey::Top) => (0.111, 0.222),
        StructureTarget::Outer(LaneKey::Mid) => (0.348, 0.498),
        StructureTarget::Inner(LaneKey::Mid) => (0.284, 0.435),
        StructureTarget::Inhib(LaneKey::Mid) => (0.222, 0.374),
        StructureTarget::Outer(LaneKey::Bot) => (0.510, 0.661),
        StructureTarget::Inner(LaneKey::Bot) => (0.448, 0.597),
        StructureTarget::Inhib(LaneKey::Bot) => (0.386, 0.534),
        StructureTarget::NexusTop => (0.224, 0.766),
        StructureTarget::NexusBot => (0.346, 0.888),
        StructureTarget::Nexus => (0.231, 0.850),
    };
    if attacker == Side::Home {
        (1.0 - blue.0, 1.0 - blue.1)
    } else {
        blue
    }
}

fn deal_structure_damage(
    state: &mut LolMapState,
    attacker: Side,
    target: StructureTarget,
    damage: f64,
    minute: u8,
) -> bool {
    let enemy = if attacker == Side::Home {
        &mut state.red
    } else {
        &mut state.blue
    };

    match target {
        StructureTarget::Outer(LaneKey::Top) => {
            hit(&mut enemy.top.outer_hp, &mut enemy.top.outer_alive, damage)
        }
        StructureTarget::Outer(LaneKey::Mid) => {
            hit(&mut enemy.mid.outer_hp, &mut enemy.mid.outer_alive, damage)
        }
        StructureTarget::Outer(LaneKey::Bot) => {
            hit(&mut enemy.bot.outer_hp, &mut enemy.bot.outer_alive, damage)
        }
        StructureTarget::Inner(LaneKey::Top) => {
            hit(&mut enemy.top.inner_hp, &mut enemy.top.inner_alive, damage)
        }
        StructureTarget::Inner(LaneKey::Mid) => {
            hit(&mut enemy.mid.inner_hp, &mut enemy.mid.inner_alive, damage)
        }
        StructureTarget::Inner(LaneKey::Bot) => {
            hit(&mut enemy.bot.inner_hp, &mut enemy.bot.inner_alive, damage)
        }
        StructureTarget::Inhib(LaneKey::Top) => {
            let down = hit(
                &mut enemy.top.inhibitor_hp,
                &mut enemy.top.inhibitor_alive,
                damage,
            );
            if down {
                enemy.top.inhibitor_respawn_minute = Some(minute.saturating_add(5));
            }
            down
        }
        StructureTarget::Inhib(LaneKey::Mid) => {
            let down = hit(
                &mut enemy.mid.inhibitor_hp,
                &mut enemy.mid.inhibitor_alive,
                damage,
            );
            if down {
                enemy.mid.inhibitor_respawn_minute = Some(minute.saturating_add(5));
            }
            down
        }
        StructureTarget::Inhib(LaneKey::Bot) => {
            let down = hit(
                &mut enemy.bot.inhibitor_hp,
                &mut enemy.bot.inhibitor_alive,
                damage,
            );
            if down {
                enemy.bot.inhibitor_respawn_minute = Some(minute.saturating_add(5));
            }
            down
        }
        StructureTarget::NexusTop => hit(
            &mut enemy.nexus_tower_top_hp,
            &mut enemy.nexus_tower_top_alive,
            damage,
        ),
        StructureTarget::NexusBot => hit(
            &mut enemy.nexus_tower_bot_hp,
            &mut enemy.nexus_tower_bot_alive,
            damage,
        ),
        StructureTarget::Nexus => {
            if enemy.nexus_tower_top_alive || enemy.nexus_tower_bot_alive {
                return false;
            }
            hit(&mut enemy.nexus_hp, &mut enemy.nexus_alive, damage)
        }
    }
}

fn hit(hp: &mut f64, alive: &mut bool, damage: f64) -> bool {
    if !*alive {
        return false;
    }
    *hp = (*hp - damage).max(0.0);
    if *hp <= 0.0 {
        *alive = false;
        true
    } else {
        false
    }
}

fn push_event(
    state: &mut LiveMatchState,
    minute: u8,
    event_type: EventType,
    side: Side,
    minute_events: &mut Vec<MatchEvent>,
) {
    let evt = MatchEvent::new(minute, event_type, side, Zone::Midfield);
    state.events.push(evt.clone());
    minute_events.push(evt);
}

fn lane_waves(state: &LolMapState, lane: LaneKey) -> (f64, f64) {
    let lane_state = match lane {
        LaneKey::Top => &state.blue.top,
        LaneKey::Mid => &state.blue.mid,
        LaneKey::Bot => &state.blue.bot,
    };
    (lane_state.blue_wave, lane_state.red_wave)
}

fn lane_waves_mut(state: &mut LolMapState, lane: LaneKey) -> (&mut f64, &mut f64) {
    let lane_state = match lane {
        LaneKey::Top => &mut state.blue.top,
        LaneKey::Mid => &mut state.blue.mid,
        LaneKey::Bot => &mut state.blue.bot,
    };
    (&mut lane_state.blue_wave, &mut lane_state.red_wave)
}

fn move_unit_with_walls<R: Rng>(
    unit: &mut LolUnitState,
    target: (f64, f64),
    speed: f64,
    rng: &mut R,
) {
    let dx = target.0 - unit.x;
    let dy = target.1 - unit.y;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist <= 1e-6 {
        return;
    }

    let step = speed.min(dist);
    let angle = dy.atan2(dx);

    for offset in [0.0, PI / 8.0, -PI / 8.0, PI / 5.0, -PI / 5.0] {
        let a = angle + offset + rng.random_range(-0.01..0.01);
        let nx = (unit.x + a.cos() * step).clamp(0.0, 1.0);
        let ny = (unit.y + a.sin() * step).clamp(0.0, 1.0);
        if segment_blocked((unit.x, unit.y), (nx, ny)) {
            continue;
        }
        unit.x = nx;
        unit.y = ny;
        return;
    }
}

fn segment_blocked(a: (f64, f64), b: (f64, f64)) -> bool {
    if point_in_active_wall(b) {
        return true;
    }
    active_walls().iter().any(|w| {
        polygon_edges(&w.points)
            .iter()
            .any(|(p1, p2)| segments_intersect(a, b, *p1, *p2))
    })
}

fn point_in_active_wall(point: (f64, f64)) -> bool {
    active_walls()
        .iter()
        .any(|w| w.closed && point_in_polygon(point, &w.points))
}

fn active_walls() -> &'static [WallPolygon] {
    static WALLS: OnceLock<Vec<WallPolygon>> = OnceLock::new();
    WALLS
        .get_or_init(|| {
            let raw = include_str!("lol_walls.json");
            let Ok(file) = serde_json::from_str::<WallFile>(raw) else {
                return Vec::new();
            };
            file.walls
                .into_iter()
                .filter(|w| {
                    w.id.starts_with("river")
                        || w.id.contains("pit")
                        || w.id == "wall-5"
                        || w.id == "wall-6"
                        || w.id == "wall-7"
                        || w.id == "wall-8"
                        || w.id == "wall-9"
                        || w.id == "wall-10"
                })
                .collect()
        })
        .as_slice()
}

fn polygon_edges(points: &[WallPoint]) -> Vec<((f64, f64), (f64, f64))> {
    if points.len() < 2 {
        return Vec::new();
    }
    let mut edges = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        let a = (points[i].x, points[i].y);
        let b = (
            points[(i + 1) % points.len()].x,
            points[(i + 1) % points.len()].y,
        );
        edges.push((a, b));
    }
    edges
}

fn segments_intersect(a1: (f64, f64), a2: (f64, f64), b1: (f64, f64), b2: (f64, f64)) -> bool {
    let o1 = orient(a1, a2, b1);
    let o2 = orient(a1, a2, b2);
    let o3 = orient(b1, b2, a1);
    let o4 = orient(b1, b2, a2);
    o1 * o2 < 0.0 && o3 * o4 < 0.0
}

fn orient(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn point_in_polygon(point: (f64, f64), polygon: &[WallPoint]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let xi = polygon[i].x;
        let yi = polygon[i].y;
        let xj = polygon[j].x;
        let yj = polygon[j].y;
        let intersect = ((yi > point.1) != (yj > point.1))
            && (point.0 < (xj - xi) * (point.1 - yi) / (yj - yi + 1e-9) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    (dx * dx + dy * dy).sqrt()
}
