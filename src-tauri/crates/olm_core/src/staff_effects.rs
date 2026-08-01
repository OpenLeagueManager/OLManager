use crate::domain::staff::{Staff, StaffRole};

/// Conservative, LoL-specific staff influence surface.
///
/// Values are intentionally narrow multipliers/bonuses. Staff improves preparation,
/// learning, information, recovery and execution reliability; it does not replace
/// player skill or champion mastery.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LolStaffEffects {
    pub coaching: f64,
    pub development: f64,
    pub tactics: f64,
    pub analysis: f64,
    pub recovery: f64,
    pub morale: f64,
    pub meta_discovery: f64,
    pub execution: f64,
}

impl Default for LolStaffEffects {
    fn default() -> Self {
        Self {
            coaching: 0.85,
            development: 0.90,
            tactics: 0.95,
            analysis: 0.95,
            recovery: 1.00,
            morale: 1.00,
            meta_discovery: 0.90,
            execution: 0.98,
        }
    }
}

fn avg(values: &[u8]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().map(|value| f64::from(*value)).sum::<f64>() / values.len() as f64)
}

fn quality_mult(avg_value: Option<f64>, empty: f64, min: f64, max: f64) -> f64 {
    let Some(value) = avg_value else {
        return empty;
    };
    (min + (value.clamp(0.0, 100.0) / 100.0) * (max - min)).clamp(min, max)
}

impl LolStaffEffects {
    pub fn for_team(staff: &[Staff], team_id: &str) -> Self {
        let team_staff: Vec<&Staff> = staff
            .iter()
            .filter(|member| member.team_id.as_deref() == Some(team_id))
            .collect();

        if team_staff.is_empty() {
            return Self::default();
        }

        let coaches: Vec<&Staff> = team_staff
            .iter()
            .copied()
            .filter(|member| matches!(member.role, StaffRole::Coach | StaffRole::AssistantManager))
            .collect();
        let scouts: Vec<&Staff> = team_staff
            .iter()
            .copied()
            .filter(|member| member.role == StaffRole::Scout)
            .collect();
        let physios: Vec<&Staff> = team_staff
            .iter()
            .copied()
            .filter(|member| member.role == StaffRole::Physio)
            .collect();

        let coaching_avg = avg(&coaches
            .iter()
            .map(|member| member.attributes.coaching)
            .collect::<Vec<_>>());
        let judging_ability_avg = avg(&scouts
            .iter()
            .map(|member| member.attributes.judging_ability)
            .collect::<Vec<_>>());
        let judging_potential_avg = avg(&scouts
            .iter()
            .map(|member| member.attributes.judging_potential)
            .collect::<Vec<_>>());
        let physio_avg = avg(&physios
            .iter()
            .map(|member| member.attributes.physiotherapy)
            .collect::<Vec<_>>());

        let coaching = quality_mult(coaching_avg, 0.85, 0.88, 1.22);
        let development = quality_mult(coaching_avg, 0.90, 0.92, 1.18);
        let tactics = quality_mult(coaching_avg, 0.95, 0.94, 1.14);
        let analysis = quality_mult(judging_ability_avg, 0.95, 0.94, 1.14);
        let recovery = quality_mult(physio_avg, 1.00, 1.00, 1.20);
        let morale = quality_mult(coaching_avg, 1.00, 0.96, 1.12);
        let meta_discovery = (quality_mult(judging_ability_avg, 0.90, 0.92, 1.18) * 0.75
            + quality_mult(judging_potential_avg, 1.00, 0.98, 1.16) * 0.25)
            .clamp(0.90, 1.20);
        let execution = ((tactics + analysis) / 2.0).clamp(0.96, 1.08);

        Self {
            coaching: coaching.clamp(0.85, 1.25),
            development: development.clamp(0.88, 1.22),
            tactics: tactics.clamp(0.90, 1.18),
            analysis: analysis.clamp(0.90, 1.16),
            recovery: recovery.clamp(0.95, 1.25),
            morale: morale.clamp(0.95, 1.15),
            meta_discovery,
            execution: execution.clamp(0.96, 1.10),
        }
    }

    pub fn match_mastery_multiplier(self) -> f64 {
        ((self.development * 0.65) + (self.analysis * 0.35)).clamp(0.88, 1.18)
    }

    pub fn draft_power_bonus(self) -> f64 {
        (((self.tactics - 1.0) * 4.0) + ((self.analysis - 1.0) * 3.0)).clamp(-1.0, 3.0)
    }
}

#[cfg(test)]
mod tests {
    use super::LolStaffEffects;
    use crate::domain::staff::{Staff, StaffAttributes, StaffRole};

    fn staff(
        id: &str,
        role: StaffRole,
        coaching: u8,
        ability: u8,
        potential: u8,
        physio: u8,
    ) -> Staff {
        let mut member = Staff::new(
            id.to_string(),
            "Test".to_string(),
            id.to_string(),
            "1980-01-01".to_string(),
            role,
            StaffAttributes {
                coaching,
                judging_ability: ability,
                judging_potential: potential,
                physiotherapy: physio,
            },
        );
        member.team_id = Some("team-1".to_string());
        member
    }

    #[test]
    fn no_staff_keeps_conservative_floor() {
        let effects = LolStaffEffects::for_team(&[], "team-1");

        assert_eq!(effects, LolStaffEffects::default());
        assert!(effects.coaching < 1.0);
        assert!(effects.execution < 1.0);
    }

    #[test]
    fn strong_staff_improves_multiple_preparation_surfaces_with_caps() {
        let coach = staff("coach", StaffRole::Coach, 92, 30, 30, 20);
        let scout = staff("scout", StaffRole::Scout, 20, 90, 86, 20);
        let physio = staff("physio", StaffRole::Physio, 20, 20, 20, 88);
        let effects = LolStaffEffects::for_team(&[coach, scout, physio], "team-1");

        assert!(effects.coaching > 1.15 && effects.coaching <= 1.25);
        assert!(effects.tactics > 1.10 && effects.tactics <= 1.18);
        assert!(effects.analysis > 1.10 && effects.analysis <= 1.16);
        assert!(effects.recovery > 1.15 && effects.recovery <= 1.25);
        assert!(effects.execution <= 1.10);
    }
}
