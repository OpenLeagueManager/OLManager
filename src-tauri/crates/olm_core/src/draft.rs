use crate::domain::player::{LolRole, PlayerAttributes};
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
}
