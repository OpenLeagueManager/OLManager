use crate::domain::player::{LolRole, PlayerAttributes};
use crate::game::Game;
use crate::meta_relationships::{canonical_champion_id, counter_value, synergy_pair_value};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const DEFAULT_MASTERY: u8 = 25;
const MAX_ENGINE_MODIFIER: f64 = 0.035;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDemandProfile {
    pub mechanics: u8,
    pub laning: u8,
    pub teamfighting: u8,
    pub macro_play: u8,
    pub consistency: u8,
    pub shotcalling: u8,
    pub champion_pool: u8,
    pub discipline: u8,
    pub mental_resilience: u8,
}

impl SkillDemandProfile {
    pub fn for_role(role: LolRole) -> Self {
        match role {
            LolRole::Top => Self {
                mechanics: 18,
                laning: 17,
                teamfighting: 11,
                macro_play: 8,
                consistency: 10,
                shotcalling: 6,
                champion_pool: 8,
                discipline: 11,
                mental_resilience: 11,
            },
            LolRole::Jungle => Self {
                mechanics: 10,
                laning: 5,
                teamfighting: 12,
                macro_play: 19,
                consistency: 9,
                shotcalling: 15,
                champion_pool: 10,
                discipline: 9,
                mental_resilience: 11,
            },
            LolRole::Mid => Self {
                mechanics: 19,
                laning: 18,
                teamfighting: 12,
                macro_play: 10,
                consistency: 9,
                shotcalling: 7,
                champion_pool: 9,
                discipline: 7,
                mental_resilience: 9,
            },
            LolRole::Adc => Self {
                mechanics: 20,
                laning: 16,
                teamfighting: 16,
                macro_play: 5,
                consistency: 13,
                shotcalling: 4,
                champion_pool: 7,
                discipline: 8,
                mental_resilience: 11,
            },
            LolRole::Support => Self {
                mechanics: 7,
                laning: 8,
                teamfighting: 16,
                macro_play: 17,
                consistency: 9,
                shotcalling: 16,
                champion_pool: 9,
                discipline: 11,
                mental_resilience: 7,
            },
            LolRole::Unknown => Self {
                mechanics: 12,
                laning: 12,
                teamfighting: 12,
                macro_play: 12,
                consistency: 11,
                shotcalling: 10,
                champion_pool: 11,
                discipline: 10,
                mental_resilience: 10,
            },
        }
    }

    pub fn for_champion(champion_id: &str, role: LolRole) -> Self {
        // Canonical champion identities refine the role baseline; unknown champions retain it.
        match champion_id.to_ascii_lowercase().as_str() {
            "azir" | "orianna" | "viktor" => Self {
                mechanics: 17,
                laning: 15,
                teamfighting: 17,
                macro_play: 12,
                consistency: 12,
                shotcalling: 6,
                champion_pool: 8,
                discipline: 7,
                mental_resilience: 6,
            },
            "zed" | "leblanc" | "akali" | "yone" => Self {
                mechanics: 25,
                laning: 19,
                teamfighting: 9,
                macro_play: 7,
                consistency: 7,
                shotcalling: 5,
                champion_pool: 12,
                discipline: 6,
                mental_resilience: 10,
            },
            "leesin" | "nidalee" | "elise" => Self {
                mechanics: 20,
                laning: 6,
                teamfighting: 9,
                macro_play: 18,
                consistency: 7,
                shotcalling: 14,
                champion_pool: 13,
                discipline: 6,
                mental_resilience: 7,
            },
            "sejuani" | "skarner" | "poppy" => Self {
                mechanics: 7,
                laning: 5,
                teamfighting: 18,
                macro_play: 17,
                consistency: 12,
                shotcalling: 15,
                champion_pool: 7,
                discipline: 11,
                mental_resilience: 8,
            },
            "jinx" | "aphelios" | "kaisa" | "kalista" => Self {
                mechanics: 23,
                laning: 15,
                teamfighting: 18,
                macro_play: 5,
                consistency: 11,
                shotcalling: 3,
                champion_pool: 9,
                discipline: 6,
                mental_resilience: 10,
            },
            "nautilus" | "rell" | "leona" => Self {
                mechanics: 6,
                laning: 7,
                teamfighting: 20,
                macro_play: 16,
                consistency: 9,
                shotcalling: 17,
                champion_pool: 7,
                discipline: 12,
                mental_resilience: 6,
            },
            _ => Self::for_role(role),
        }
    }

    fn weighted_fit(self, attrs: &PlayerAttributes) -> f64 {
        let values = [
            attrs.mechanics,
            attrs.laning,
            attrs.teamfighting,
            attrs.macro_play,
            attrs.consistency,
            attrs.shotcalling,
            attrs.champion_pool,
            attrs.discipline,
            attrs.mental_resilience,
        ];
        let weights = [
            self.mechanics,
            self.laning,
            self.teamfighting,
            self.macro_play,
            self.consistency,
            self.shotcalling,
            self.champion_pool,
            self.discipline,
            self.mental_resilience,
        ];
        values
            .into_iter()
            .zip(weights)
            .map(|(value, weight)| value as f64 * weight as f64)
            .sum::<f64>()
            / 100.0
    }

    fn execution_stability(self, attrs: &PlayerAttributes) -> f64 {
        (attrs.consistency as f64 * self.consistency as f64
            + attrs.discipline as f64 * self.discipline as f64
            + attrs.mental_resilience as f64 * self.mental_resilience as f64)
            / (self.consistency as f64 + self.discipline as f64 + self.mental_resilience as f64)
                .max(1.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChampionProfile {
    pub champion_id: String,
    pub role: LolRole,
    pub meta_power: u8,
    pub demands: SkillDemandProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PickEvaluation {
    pub champion_id: String,
    pub meta_power: u8,
    pub mastery: u8,
    pub skill_fit: u8,
    pub execution_risk: u8,
    pub total: u8,
    pub engine_modifier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftPickInput {
    pub player_id: String,
    pub champion_id: String,
    pub effective_role: LolRole,
}

/// Relationship deltas are deliberately small so they complement, rather than replace,
/// the existing meta/mastery/skill evaluation.
pub const MAX_RELATIONSHIP_CONTRIBUTION: i8 = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftSideInput {
    pub team_id: String,
    pub picks: Vec<DraftPickInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DraftKnowledgeInput {
    /// Exact opponent champion IDs whose relationship data was revealed elsewhere.
    #[serde(default)]
    pub authorized_relationship_champion_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftStateInput {
    pub viewer_team_id: String,
    pub blue: DraftSideInput,
    pub red: DraftSideInput,
    #[serde(default)]
    pub knowledge: DraftKnowledgeInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftRelationshipContribution {
    pub synergy: i8,
    pub counter: i8,
    pub total: i8,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DraftStateEvaluation {
    /// Existing evaluations are restricted to the manager's own players and redacted
    /// when their meta entry has not been discovered.
    pub pick_evaluations: Vec<VisiblePickEvaluation>,
    /// Only the viewer's authorized relationships are serialized. The other side is neutral.
    pub blue_relationship: DraftRelationshipContribution,
    pub red_relationship: DraftRelationshipContribution,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VisiblePickEvaluation {
    pub champion_id: String,
    pub meta_power: Option<u8>,
    pub mastery: u8,
    pub skill_fit: u8,
    pub execution_risk: u8,
    pub total: Option<u8>,
    pub engine_modifier: Option<f64>,
}

fn relationship_contribution(
    own_picks: &[DraftPickInput],
    enemy_picks: &[DraftPickInput],
    authorized_enemy_ids: &std::collections::HashSet<String>,
) -> DraftRelationshipContribution {
    let mut synergy = 0_i8;
    let mut counter = 0_i8;
    let mut reasons = Vec::new();
    for (index, pick) in own_picks.iter().enumerate() {
        for teammate in &own_picks[index + 1..] {
            let value = synergy_pair_value(&pick.champion_id, &teammate.champion_id);
            if value > 0 {
                synergy += value;
                reasons.push("team synergy".to_string());
            }
        }
        for enemy in enemy_picks {
            if !authorized_enemy_ids.contains(&canonical_champion_id(&enemy.champion_id)) {
                continue;
            }
            counter += counter_value(&pick.champion_id, &enemy.champion_id);
            counter -= counter_value(&enemy.champion_id, &pick.champion_id);
        }
    }
    synergy = synergy.clamp(
        -MAX_RELATIONSHIP_CONTRIBUTION,
        MAX_RELATIONSHIP_CONTRIBUTION,
    );
    counter = counter.clamp(
        -MAX_RELATIONSHIP_CONTRIBUTION,
        MAX_RELATIONSHIP_CONTRIBUTION,
    );
    DraftRelationshipContribution {
        synergy,
        counter,
        total: (synergy + counter).clamp(
            -MAX_RELATIONSHIP_CONTRIBUTION,
            MAX_RELATIONSHIP_CONTRIBUTION,
        ),
        reasons: if reasons.is_empty() {
            Vec::new()
        } else {
            vec!["team synergy".to_string()]
        },
    }
}

/// Evaluates canonical relationships for both fully resolved sides. This stays
/// crate-private because auto-simulation may use complete hidden information.
pub(crate) fn evaluate_internal_draft_relationships(
    blue_picks: &[DraftPickInput],
    red_picks: &[DraftPickInput],
) -> (DraftRelationshipContribution, DraftRelationshipContribution) {
    let all_enemy_ids = |picks: &[DraftPickInput]| {
        picks
            .iter()
            .map(|pick| canonical_champion_id(&pick.champion_id))
            .collect::<HashSet<_>>()
    };
    (
        relationship_contribution(blue_picks, red_picks, &all_enemy_ids(red_picks)),
        relationship_contribution(red_picks, blue_picks, &all_enemy_ids(blue_picks)),
    )
}

/// Evaluates a completed draft through one privacy-preserving seam. Hidden meta and
/// unapproved opponent relationships never cross this API.
pub fn evaluate_draft_state(
    game: &Game,
    input: &DraftStateInput,
) -> Result<DraftStateEvaluation, String> {
    let viewer_is_blue = input.blue.team_id == input.viewer_team_id;
    let viewer_is_red = input.red.team_id == input.viewer_team_id;
    if !viewer_is_blue && !viewer_is_red {
        return Err("Viewer team is not in this draft".to_string());
    }
    let own = if viewer_is_blue {
        &input.blue
    } else {
        &input.red
    };
    let enemy = if viewer_is_blue {
        &input.red
    } else {
        &input.blue
    };
    let authorized_enemy_ids = input
        .knowledge
        .authorized_relationship_champion_ids
        .iter()
        .map(|id| canonical_champion_id(id))
        .collect();
    if own.picks.iter().any(|pick| {
        game.players
            .iter()
            .find(|player| player.id == pick.player_id)
            .and_then(|player| player.team_id.as_deref())
            != Some(input.viewer_team_id.as_str())
    }) {
        return Err("Draft contains a player outside the viewer team".to_string());
    }
    let visible_relationship =
        relationship_contribution(&own.picks, &enemy.picks, &authorized_enemy_ids);
    let neutral = DraftRelationshipContribution {
        synergy: 0,
        counter: 0,
        total: 0,
        reasons: Vec::new(),
    };
    let discovered = game
        .champion_patch
        .discovered_champion_ids
        .iter()
        .map(|id| canonical_champion_id(id))
        .collect::<HashSet<_>>();
    let pick_evaluations = evaluate_draft_picks(game, &own.picks)?
        .into_iter()
        .map(|evaluation| {
            let meta_is_discovered =
                discovered.contains(&canonical_champion_id(&evaluation.champion_id));
            VisiblePickEvaluation {
                champion_id: evaluation.champion_id,
                meta_power: meta_is_discovered.then_some(evaluation.meta_power),
                mastery: evaluation.mastery,
                skill_fit: evaluation.skill_fit,
                execution_risk: evaluation.execution_risk,
                total: meta_is_discovered.then_some(evaluation.total),
                engine_modifier: meta_is_discovered.then_some(evaluation.engine_modifier),
            }
        })
        .collect();
    Ok(DraftStateEvaluation {
        pick_evaluations,
        blue_relationship: if viewer_is_blue {
            visible_relationship.clone()
        } else {
            neutral.clone()
        },
        red_relationship: if viewer_is_red {
            visible_relationship
        } else {
            neutral
        },
    })
}

fn meta_power_for(game: &Game, champion_id: &str, role: LolRole) -> u8 {
    game.champion_patch
        .hidden_meta
        .iter()
        .find(|entry| {
            entry.champion_id.eq_ignore_ascii_case(champion_id)
                && entry.role.eq_ignore_ascii_case(&format!("{role:?}"))
        })
        .map(|entry| match entry.tier.to_ascii_uppercase().as_str() {
            "S" => 90,
            "A" => 75,
            "B" => 60,
            "C" => 45,
            "D" => 30,
            _ => 60,
        })
        .unwrap_or(60)
}

pub fn evaluate_draft_picks(
    game: &Game,
    picks: &[DraftPickInput],
) -> Result<Vec<PickEvaluation>, String> {
    picks
        .iter()
        .map(|pick| {
            let player = game
                .players
                .iter()
                .find(|player| player.id == pick.player_id)
                .ok_or_else(|| format!("Player not found: {}", pick.player_id))?;
            let mastery = game
                .champion_masteries
                .iter()
                .find(|entry| {
                    entry.player_id == player.id
                        && entry.champion_id.eq_ignore_ascii_case(&pick.champion_id)
                })
                .map(|entry| entry.mastery);
            let profile = ChampionProfile {
                champion_id: pick.champion_id.clone(),
                role: pick.effective_role,
                meta_power: meta_power_for(game, &pick.champion_id, pick.effective_role),
                demands: SkillDemandProfile::for_champion(&pick.champion_id, pick.effective_role),
            };
            Ok(evaluate_pick(&player.attributes, mastery, &profile))
        })
        .collect()
}

pub fn evaluate_pick(
    attrs: &PlayerAttributes,
    mastery: Option<u8>,
    profile: &ChampionProfile,
) -> PickEvaluation {
    let mastery = mastery
        .unwrap_or(DEFAULT_MASTERY)
        .clamp(DEFAULT_MASTERY, 100);
    let skill_fit = profile
        .demands
        .weighted_fit(attrs)
        .round()
        .clamp(0.0, 100.0) as u8;
    let stability = profile.demands.execution_stability(attrs);
    let execution_risk = (100.0 - ((mastery as f64 * 0.45) + (stability * 0.55)))
        .round()
        .clamp(0.0, 100.0) as u8;
    // OVR is intentionally absent: the engine already models the underlying attributes.
    let raw = profile.meta_power as f64 * 0.22 + mastery as f64 * 0.34 + skill_fit as f64 * 0.34
        - execution_risk as f64 * 0.10;
    let total = raw.round().clamp(0.0, 100.0) as u8;
    let engine_modifier = ((total as f64 - 50.0) / 50.0 * MAX_ENGINE_MODIFIER)
        .clamp(-MAX_ENGINE_MODIFIER, MAX_ENGINE_MODIFIER);
    PickEvaluation {
        champion_id: profile.champion_id.clone(),
        meta_power: profile.meta_power,
        mastery,
        skill_fit,
        execution_risk,
        total,
        engine_modifier,
    }
}

pub fn select_pick(
    attrs: &PlayerAttributes,
    mastery_by_champion: &[(String, u8)],
    candidates: &[ChampionProfile],
    unavailable: &HashSet<String>,
) -> Option<PickEvaluation> {
    candidates
        .iter()
        .filter(|candidate| !unavailable.contains(&candidate.champion_id))
        .map(|candidate| {
            evaluate_pick(
                attrs,
                mastery_by_champion
                    .iter()
                    .find(|(id, _)| id.eq_ignore_ascii_case(&candidate.champion_id))
                    .map(|(_, value)| *value),
                candidate,
            )
        })
        .max_by(|left, right| {
            left.total
                .cmp(&right.total)
                .then_with(|| right.champion_id.cmp(&left.champion_id))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::champions::{ChampionMasteryEntry, ChampionMetaEntry};
    use crate::clock::GameClock;
    use crate::domain::manager::Manager;
    use crate::domain::player::Player;
    use crate::domain::team::Team;
    use chrono::{TimeZone, Utc};
    fn attrs(value: u8) -> PlayerAttributes {
        PlayerAttributes {
            mechanics: value,
            laning: value,
            teamfighting: value,
            macro_play: value,
            consistency: value,
            shotcalling: value,
            champion_pool: value,
            discipline: value,
            mental_resilience: value,
        }
    }
    fn profile(id: &str, role: LolRole, meta: u8) -> ChampionProfile {
        ChampionProfile {
            champion_id: id.into(),
            role,
            meta_power: meta,
            demands: SkillDemandProfile::for_champion(id, role),
        }
    }
    fn game_with_player() -> Game {
        let mut manager = Manager::new(
            "manager".into(),
            "Manager".into(),
            "One".into(),
            "1980-01-01".into(),
            "ES".into(),
        );
        manager.hire("team".into());
        let team = Team::new(
            "team".into(),
            "Team".into(),
            "TEM".into(),
            "ES".into(),
            "City".into(),
            "Arena".into(),
            1,
        );
        let date = Utc.with_ymd_and_hms(2026, 1, 6, 12, 0, 0).single().unwrap();
        let mut player = Player::new(
            "player".into(),
            "Player".into(),
            "One".into(),
            "2000-01-01".into(),
            "ES".into(),
            LolRole::Mid,
            attrs(60),
        );
        player.team_id = Some("team".into());
        Game::new(
            GameClock::new(date),
            manager,
            vec![team],
            vec![player],
            vec![],
            vec![],
        )
    }
    #[test]
    fn evaluation_is_deterministic_and_clamped() {
        let value = evaluate_pick(&attrs(100), Some(150), &profile("A", LolRole::Mid, 150));
        assert_eq!(
            value,
            evaluate_pick(&attrs(100), Some(150), &profile("A", LolRole::Mid, 150))
        );
        assert_eq!(value.mastery, 100);
        assert!(value.engine_modifier <= MAX_ENGINE_MODIFIER);
    }
    #[test]
    fn role_demand_uses_all_attributes() {
        let mut jungle = attrs(50);
        jungle.macro_play = 95;
        jungle.shotcalling = 95;
        let mut mid = jungle.clone();
        mid.mechanics = 10;
        mid.laning = 10;
        assert!(
            evaluate_pick(&jungle, Some(60), &profile("J", LolRole::Jungle, 60)).skill_fit
                > evaluate_pick(&mid, Some(60), &profile("M", LolRole::Mid, 60)).skill_fit
        );
    }
    #[test]
    fn mastery_and_pool_related_fit_change_selection() {
        let candidates = [
            profile("comfort", LolRole::Adc, 60),
            profile("meta", LolRole::Adc, 90),
        ];
        let selected = select_pick(
            &attrs(65),
            &[("comfort".into(), 100)],
            &candidates,
            &HashSet::new(),
        )
        .unwrap();
        assert_eq!(selected.champion_id, "comfort");
    }
    #[test]
    fn champion_pool_contributes_to_skill_fit() {
        let mut versatile = attrs(60);
        versatile.champion_pool = 100;
        let mut narrow = versatile.clone();
        narrow.champion_pool = 0;
        assert!(
            evaluate_pick(&versatile, Some(60), &profile("A", LolRole::Adc, 60)).skill_fit
                > evaluate_pick(&narrow, Some(60), &profile("A", LolRole::Adc, 60)).skill_fit
        );
    }
    #[test]
    fn selection_respects_legality() {
        let candidates = [
            profile("taken", LolRole::Top, 100),
            profile("open", LolRole::Top, 60),
        ];
        let unavailable = HashSet::from(["taken".to_string()]);
        assert_eq!(
            select_pick(&attrs(70), &[], &candidates, &unavailable)
                .unwrap()
                .champion_id,
            "open"
        );
    }
    #[test]
    fn canonical_champion_profiles_change_same_role_fit() {
        let mut attrs = attrs(50);
        attrs.mechanics = 95;
        attrs.teamfighting = 25;
        let control = profile("Azir", LolRole::Mid, 60);
        let assassin = profile("Zed", LolRole::Mid, 60);
        assert_ne!(control.demands, assassin.demands);
        assert_ne!(
            evaluate_pick(&attrs, Some(60), &control).skill_fit,
            evaluate_pick(&attrs, Some(60), &assassin).skill_fit
        );
    }
    #[test]
    fn batch_matches_individual_and_preserves_input_order() {
        let mut game = game_with_player();
        game.champion_masteries.push(ChampionMasteryEntry {
            player_id: "player".into(),
            champion_id: "Azir".into(),
            mastery: 80,
            last_active_on: "2026-01-06".into(),
        });
        let picks = vec![
            DraftPickInput {
                player_id: "player".into(),
                champion_id: "Azir".into(),
                effective_role: LolRole::Mid,
            },
            DraftPickInput {
                player_id: "player".into(),
                champion_id: "Zed".into(),
                effective_role: LolRole::Mid,
            },
        ];
        let batch = evaluate_draft_picks(&game, &picks).unwrap();
        assert_eq!(batch[0].champion_id, "Azir");
        assert_eq!(batch[1].champion_id, "Zed");
        let profile = ChampionProfile {
            champion_id: "Azir".into(),
            role: LolRole::Mid,
            meta_power: 60,
            demands: SkillDemandProfile::for_champion("Azir", LolRole::Mid),
        };
        assert_eq!(
            batch[0],
            evaluate_pick(&game.players[0].attributes, Some(80), &profile)
        );
    }
    #[test]
    fn batch_uses_effective_role_for_meta_and_profile() {
        let mut game = game_with_player();
        game.players[0].attributes.mechanics = 10;
        game.players[0].attributes.laning = 10;
        game.players[0].attributes.macro_play = 100;
        game.players[0].attributes.shotcalling = 100;
        game.champion_patch.hidden_meta = vec![
            ChampionMetaEntry {
                champion_id: "Garen".into(),
                role: "MID".into(),
                tier: "D".into(),
            },
            ChampionMetaEntry {
                champion_id: "Garen".into(),
                role: "JUNGLE".into(),
                tier: "S".into(),
            },
        ];
        let mid = evaluate_draft_picks(
            &game,
            &[DraftPickInput {
                player_id: "player".into(),
                champion_id: "Garen".into(),
                effective_role: LolRole::Mid,
            }],
        )
        .unwrap();
        let jungle = evaluate_draft_picks(
            &game,
            &[DraftPickInput {
                player_id: "player".into(),
                champion_id: "Garen".into(),
                effective_role: LolRole::Jungle,
            }],
        )
        .unwrap();
        assert_eq!(mid[0].meta_power, 30);
        assert_eq!(jungle[0].meta_power, 90);
        assert_ne!(mid[0].skill_fit, jungle[0].skill_fit);
    }
    #[test]
    fn batch_reports_missing_player_in_input_order() {
        let game = game_with_player();
        let error = evaluate_draft_picks(
            &game,
            &[
                DraftPickInput {
                    player_id: "player".into(),
                    champion_id: "Azir".into(),
                    effective_role: LolRole::Mid,
                },
                DraftPickInput {
                    player_id: "missing".into(),
                    champion_id: "Zed".into(),
                    effective_role: LolRole::Mid,
                },
            ],
        )
        .unwrap_err();
        assert_eq!(error, "Player not found: missing");
    }

    #[test]
    fn state_evaluation_filters_unapproved_counter_knowledge_and_clamps_relationships() {
        let game = game_with_player();
        let blue = DraftSideInput {
            team_id: "team".into(),
            picks: vec![
                DraftPickInput {
                    player_id: "player".into(),
                    champion_id: "Aatrox".into(),
                    effective_role: LolRole::Top,
                },
                DraftPickInput {
                    player_id: "player".into(),
                    champion_id: "Kindred".into(),
                    effective_role: LolRole::Jungle,
                },
            ],
        };
        let red = DraftSideInput {
            team_id: "rival".into(),
            picks: vec![DraftPickInput {
                player_id: "player".into(),
                champion_id: "Chogath".into(),
                effective_role: LolRole::Top,
            }],
        };
        let hidden = evaluate_draft_state(
            &game,
            &DraftStateInput {
                viewer_team_id: "team".into(),
                blue: blue.clone(),
                red: red.clone(),
                knowledge: DraftKnowledgeInput::default(),
            },
        )
        .unwrap();
        assert_eq!(hidden.blue_relationship.synergy, 2);
        assert_eq!(hidden.blue_relationship.counter, 0);
        assert_eq!(hidden.red_relationship.total, 0);
        assert!(
            hidden
                .blue_relationship
                .reasons
                .iter()
                .all(|reason| reason == "team synergy")
        );
        assert_eq!(hidden.pick_evaluations[0].meta_power, None);
        assert_eq!(hidden.pick_evaluations[0].total, None);

        let reversed = evaluate_draft_state(
            &game,
            &DraftStateInput {
                viewer_team_id: "team".into(),
                blue: DraftSideInput {
                    team_id: "team".into(),
                    picks: blue.picks.iter().cloned().rev().collect(),
                },
                red: red.clone(),
                knowledge: DraftKnowledgeInput::default(),
            },
        )
        .unwrap();
        assert_eq!(
            reversed.blue_relationship.synergy,
            hidden.blue_relationship.synergy
        );

        let authorized = evaluate_draft_state(
            &game,
            &DraftStateInput {
                viewer_team_id: "team".into(),
                blue,
                red,
                knowledge: DraftKnowledgeInput {
                    authorized_relationship_champion_ids: vec!["Cho'Gath".into()],
                },
            },
        )
        .unwrap();
        assert_eq!(authorized.blue_relationship.counter, 1);
        assert!(authorized.blue_relationship.total.abs() <= MAX_RELATIONSHIP_CONTRIBUTION);
    }

    #[test]
    fn internal_relationship_evaluation_uses_complete_opponent_drafts() {
        let blue = vec![
            DraftPickInput {
                player_id: "blue-top".into(),
                champion_id: "Aatrox".into(),
                effective_role: LolRole::Top,
            },
            DraftPickInput {
                player_id: "blue-jungle".into(),
                champion_id: "Kindred".into(),
                effective_role: LolRole::Jungle,
            },
        ];
        let red = vec![DraftPickInput {
            player_id: "red-top".into(),
            champion_id: "Cho'Gath".into(),
            effective_role: LolRole::Top,
        }];

        let (blue_relationship, red_relationship) =
            evaluate_internal_draft_relationships(&blue, &red);

        assert_eq!(blue_relationship.synergy, 2);
        assert_eq!(blue_relationship.counter, 1);
        assert_eq!(blue_relationship.total, 3);
        assert_eq!(red_relationship.counter, -1);
        assert_eq!(red_relationship.total, -1);

        let repeated_blue = (0..7)
            .flat_map(|index| {
                [
                    DraftPickInput {
                        player_id: format!("blue-aatrox-{index}"),
                        champion_id: "Aatrox".into(),
                        effective_role: LolRole::Top,
                    },
                    DraftPickInput {
                        player_id: format!("blue-kindred-{index}"),
                        champion_id: "Kindred".into(),
                        effective_role: LolRole::Jungle,
                    },
                ]
            })
            .collect::<Vec<_>>();
        let repeated_red = vec![red[0].clone(); 7];
        let (clamped_blue, clamped_red) =
            evaluate_internal_draft_relationships(&repeated_blue, &repeated_red);
        assert_eq!(clamped_blue.synergy, MAX_RELATIONSHIP_CONTRIBUTION);
        assert_eq!(clamped_blue.counter, MAX_RELATIONSHIP_CONTRIBUTION);
        assert_eq!(clamped_blue.total, MAX_RELATIONSHIP_CONTRIBUTION);
        assert_eq!(clamped_red.counter, -MAX_RELATIONSHIP_CONTRIBUTION);
        assert_eq!(clamped_red.total, -MAX_RELATIONSHIP_CONTRIBUTION);
    }

    #[test]
    fn state_evaluation_is_deterministic_and_rejects_unknown_viewer() {
        let game = game_with_player();
        let input = DraftStateInput {
            viewer_team_id: "team".into(),
            blue: DraftSideInput {
                team_id: "team".into(),
                picks: vec![],
            },
            red: DraftSideInput {
                team_id: "rival".into(),
                picks: vec![],
            },
            knowledge: DraftKnowledgeInput::default(),
        };
        assert_eq!(
            evaluate_draft_state(&game, &input),
            evaluate_draft_state(&game, &input)
        );
        let mut invalid = input;
        invalid.viewer_team_id = "other".into();
        assert_eq!(
            evaluate_draft_state(&game, &invalid).unwrap_err(),
            "Viewer team is not in this draft"
        );
    }
}
