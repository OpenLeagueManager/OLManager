use olm_core::domain::league::MatchType;
use olm_core::state::StateManager;

pub(super) fn competition_label(match_type: &MatchType) -> String {
    match match_type {
        MatchType::League => "League".to_string(),
        MatchType::Friendly => "Friendly".to_string(),
        MatchType::PreseasonTournament => "PreseasonTournament".to_string(),
        MatchType::Playoffs => "Playoffs".to_string(),
    }
}

pub(super) fn round_to(value: f32, digits: i32) -> f32 {
    let factor = 10_f32.powi(digits);
    (value * factor).round() / factor
}

pub(super) fn calculate_per_match(total: u32, matches_played: u32) -> Option<f32> {
    if matches_played == 0 {
        return None;
    }

    Some(round_to(total as f32 / matches_played as f32, 1))
}

pub(super) fn calculate_average(total: u32, count: u32) -> Option<f32> {
    if count == 0 {
        return None;
    }

    Some(round_to(total as f32 / count as f32, 1))
}

pub(super) fn percentile_rank(values: &[f32], target: Option<f32>) -> Option<u32> {
    let target = target?;
    if values.is_empty() {
        return None;
    }

    let ranked_count = values.iter().filter(|value| **value <= target).count();
    Some(((ranked_count as f32 / values.len() as f32) * 100.0).round() as u32)
}

pub(super) fn opponent_name(state: &StateManager, opponent_team_id: &str) -> String {
    state
        .get_game(|game| {
            game.teams
                .iter()
                .find(|team| team.id == opponent_team_id)
                .map(|team| team.name.clone())
        })
        .flatten()
        .unwrap_or_else(|| opponent_team_id.to_string())
}

pub(super) fn ensure_team_exists(state: &StateManager, team_id: &str) -> Result<(), String> {
    let team_exists = state
        .get_game(|game| game.teams.iter().any(|team| team.id == team_id))
        .ok_or("No active game session".to_string())?;

    if team_exists {
        Ok(())
    } else {
        Err("Team not found".to_string())
    }
}
