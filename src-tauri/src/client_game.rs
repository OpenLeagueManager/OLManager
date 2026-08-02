use std::collections::HashSet;

use olm_core::game::Game;

/// Produces the only `Game` shape that may cross the IPC seam.
///
/// The authoritative state remains untouched so persistence, autosim and AI retain
/// the complete patch meta. Clients receive only entries they have discovered.
pub fn game_for_client(game: &Game) -> Game {
    let mut projected = game.clone();
    let discovered: HashSet<String> = projected
        .champion_patch
        .discovered_champion_ids
        .iter()
        .map(|id| normalize_champion_id(id))
        .collect();
    projected
        .champion_patch
        .hidden_meta
        .retain(|entry| discovered.contains(&normalize_champion_id(&entry.champion_id)));
    projected
}

fn normalize_champion_id(id: &str) -> String {
    id.to_ascii_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::game_for_client;
    use chrono::{TimeZone, Utc};
    use olm_core::{
        champions::ChampionMetaEntry, clock::GameClock, domain::manager::Manager, game::Game,
    };

    fn game_with_secret_meta() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 2, 12, 0, 0).unwrap());
        let manager = Manager::new(
            "manager".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        let mut game = Game::new(clock, manager, vec![], vec![], vec![], vec![]);
        game.champion_patch.discovered_champion_ids = vec!["Ahri".to_string()];
        game.champion_patch.hidden_meta = vec![
            ChampionMetaEntry {
                champion_id: "Ahri".to_string(),
                role: "Mid".to_string(),
                tier: "S".to_string(),
            },
            ChampionMetaEntry {
                champion_id: "Zed".to_string(),
                role: "Mid".to_string(),
                tier: "D".to_string(),
            },
        ];
        game
    }

    #[test]
    fn projection_redacts_undiscovered_meta_without_mutating_save_state() {
        let game = game_with_secret_meta();
        let saved_before = serde_json::to_string(&game).unwrap();

        let projected = game_for_client(&game);

        assert_eq!(projected.champion_patch.hidden_meta.len(), 1);
        assert_eq!(projected.champion_patch.hidden_meta[0].champion_id, "Ahri");
        assert_eq!(game.champion_patch.hidden_meta.len(), 2);
        assert_eq!(serde_json::to_string(&game).unwrap(), saved_before);
    }
}
