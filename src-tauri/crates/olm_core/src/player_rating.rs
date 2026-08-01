use crate::domain::player::{LolRole, Player};

/// Returns the 5 starting positions for a team in LoL format.
/// In LoL, the lineup is always 5 players: Top, Jungle, Mid, ADC, Support
pub fn position_slots() -> Vec<LolRole> {
    vec![
        LolRole::Top,
        LolRole::Jungle,
        LolRole::Mid,
        LolRole::Adc,
        LolRole::Support,
    ]
}

/// Calculate overall rating for a player at a specific LolRole
pub fn ovr_for_position(player: &Player, _role: &LolRole) -> f64 {
    natural_ovr(player)
}

pub fn effective_rating_for_assignment(player: &Player, slot_role: &LolRole) -> f64 {
    let base = ovr_for_position(player, slot_role);
    let compat = compatibility_penalty(player, slot_role);
    let foot = footedness_penalty(player, slot_role);
    base - compat - foot
}

pub fn natural_ovr(player: &Player) -> f64 {
    let attrs = &player.attributes;
    // Unified OVR: average of 9 visible LoL stats (matches calculate_lol_ovr in potential.rs)
    (attrs.mechanics as f64
        + attrs.laning as f64
        + attrs.teamfighting as f64
        + attrs.macro_play as f64
        + attrs.consistency as f64
        + attrs.shotcalling as f64
        + attrs.champion_pool as f64
        + attrs.discipline as f64
        + attrs.mental_resilience as f64)
        / 9.0
}

fn primary_position(player: &Player) -> LolRole {
    player.natural_position
}

fn compatibility_penalty(player: &Player, slot_role: &LolRole) -> f64 {
    let primary = primary_position(player);
    if &primary == slot_role {
        return 0.0;
    }

    let alternates: Vec<LolRole> = player.alternate_positions.clone();

    if alternates.iter().any(|role| role == slot_role) {
        4.0
    } else if role_compatibility(&primary, slot_role) {
        8.0
    } else {
        14.0
    }
}

fn role_compatibility(primary: &LolRole, slot: &LolRole) -> bool {
    // Define role compatibility groups
    match (primary, slot) {
        // Top can flex to Jungle, Mid
        (LolRole::Top, LolRole::Top | LolRole::Jungle | LolRole::Mid) => true,
        // Jungle can flex to Top, Mid
        (LolRole::Jungle, LolRole::Jungle | LolRole::Top | LolRole::Mid) => true,
        // Mid can flex to Top, Jungle, ADC
        (LolRole::Mid, LolRole::Mid | LolRole::Top | LolRole::Jungle | LolRole::Adc) => true,
        // ADC can flex to Mid
        (LolRole::Adc, LolRole::Adc | LolRole::Mid) => true,
        // Support is most flexible (can play any role)
        (LolRole::Support, _) => true,
        // Unknown can't play anywhere
        (LolRole::Unknown, _) => false,
        // Exact match handled earlier
        _ => false,
    }
}

fn footedness_penalty(_player: &Player, _slot_role: &LolRole) -> f64 {
    // Footedness doesn't apply to LoL - return 0
    // TODO: Consider lane preference (top/mid prefer right side, bot prefer left)
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::player::PlayerAttributes;

    fn make_player(role: LolRole) -> Player {
        let attrs = PlayerAttributes {
            mechanics: 68,
            laning: 60,
            teamfighting: 80,
            macro_play: 78,
            consistency: 70,
            shotcalling: 45,
            champion_pool: 72,
            discipline: 60,
            mental_resilience: 75,
        };
        Player::new(
            "test-1".to_string(),
            "Test Player".to_string(),
            "Test Player Full".to_string(),
            "2000-01-15".to_string(),
            "US".to_string(),
            role,
            attrs,
        )
    }

    #[test]
    fn position_slots_returns_five_roles() {
        let slots = position_slots();
        assert_eq!(slots.len(), 5);
        assert_eq!(
            slots,
            vec![
                LolRole::Top,
                LolRole::Jungle,
                LolRole::Mid,
                LolRole::Adc,
                LolRole::Support,
            ]
        );
    }

    #[test]
    fn ovr_for_position_returns_natural_ovr() {
        let player = make_player(LolRole::Mid);
        let ovr = ovr_for_position(&player, &LolRole::Mid);
        let natural = natural_ovr(&player);
        assert!((ovr - natural).abs() < 0.001);
    }

    #[test]
    fn compatibility_penalty_exact_match() {
        let player = make_player(LolRole::Mid);
        let penalty = compatibility_penalty(&player, &LolRole::Mid);
        assert_eq!(penalty, 0.0);
    }

    #[test]
    fn effective_rating_for_assignment() {
        let player = make_player(LolRole::Mid);
        let rating = super::effective_rating_for_assignment(&player, &LolRole::Mid);
        assert!(rating > 0.0);
    }
}
