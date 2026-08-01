use log::{error, info, warn};
use rusqlite::Connection;
use std::fs;
use std::path::Path;

use crate::game::Game;
use crate::player_identity;

use crate::db::save_manager::SaveManager;

/// A row extracted from the legacy `saves.db` file.
#[derive(Debug)]
#[allow(dead_code)]
struct LegacySaveRow {
    id: String,
    name: String,
    manager_name: String,
    game_data: String,
    created_at: String,
    last_played_at: String,
}

/// Result of migrating one legacy save.
#[derive(Debug)]
pub enum LegacyMigrationResult {
    /// Successfully migrated to a new per-save DB.
    Success {
        old_id: String,
        new_id: String,
        name: String,
    },
    /// Failed to migrate (corrupt JSON, etc).
    Failed {
        old_id: String,
        name: String,
        reason: String,
    },
}

/// Check if a legacy `saves.db` exists at the given path.
pub fn has_legacy_db(app_data_dir: &Path) -> bool {
    app_data_dir.join("saves.db").exists()
}

/// Migrate all saves from the legacy `saves.db` into per-save databases
/// managed by the SaveManager. Returns results for each save attempted.
///
/// After successful migration, renames `saves.db` to `saves.db.migrated`.
pub fn migrate_legacy_saves(
    app_data_dir: &Path,
    save_manager: &mut SaveManager,
) -> Result<Vec<LegacyMigrationResult>, String> {
    let legacy_path = app_data_dir.join("saves.db");
    if !legacy_path.exists() {
        info!("[legacy] no saves.db found, nothing to migrate");
        return Ok(Vec::new());
    }

    info!("[legacy] found legacy saves.db, starting migration");

    let rows = extract_legacy_rows(&legacy_path)?;
    info!(
        "[legacy] extracted {} saves from legacy database",
        rows.len()
    );

    let mut results = Vec::new();

    for row in &rows {
        match migrate_single_save(row, save_manager) {
            Ok(new_id) => {
                info!(
                    "[legacy] migrated save '{}' ({}) -> {}",
                    row.name, row.id, new_id
                );
                results.push(LegacyMigrationResult::Success {
                    old_id: row.id.clone(),
                    new_id,
                    name: row.name.clone(),
                });
            }
            Err(reason) => {
                error!(
                    "[legacy] failed to migrate save '{}' ({}): {}",
                    row.name, row.id, reason
                );
                results.push(LegacyMigrationResult::Failed {
                    old_id: row.id.clone(),
                    name: row.name.clone(),
                    reason,
                });
            }
        }
    }

    // Rename the old database to prevent re-migration
    let migrated_path = app_data_dir.join("saves.db.migrated");
    fs::rename(&legacy_path, &migrated_path)
        .map_err(|e| format!("Failed to rename saves.db: {}", e))?;
    info!(
        "[legacy] renamed saves.db -> saves.db.migrated ({} saves processed)",
        results.len()
    );

    Ok(results)
}

/// Extract all save rows from the legacy database.
fn extract_legacy_rows(legacy_path: &Path) -> Result<Vec<LegacySaveRow>, String> {
    let conn = Connection::open(legacy_path)
        .map_err(|e| format!("Failed to open legacy database: {}", e))?;

    // Check if the saves table exists
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='saves'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .map_err(|e| format!("Failed to check for saves table: {}", e))?;

    if !table_exists {
        warn!("[legacy] saves.db has no 'saves' table");
        return Ok(Vec::new());
    }

    let mut stmt = conn
        .prepare(
            "SELECT id, name, manager_name, game_data, created_at, last_played_at
             FROM saves ORDER BY last_played_at DESC",
        )
        .map_err(|e| format!("Failed to prepare legacy query: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(LegacySaveRow {
                id: row.get(0)?,
                name: row.get(1)?,
                manager_name: row.get(2)?,
                game_data: row.get(3)?,
                created_at: row.get(4)?,
                last_played_at: row.get(5)?,
            })
        })
        .map_err(|e| format!("Failed to query legacy saves: {}", e))?;

    let mut saves = Vec::new();
    for row in rows {
        saves.push(row.map_err(|e| format!("Failed to read legacy row: {}", e))?);
    }
    Ok(saves)
}

/// Migrate a single legacy save by deserializing the JSON blob and
/// creating a new save via SaveManager.
fn migrate_single_save(
    row: &LegacySaveRow,
    save_manager: &mut SaveManager,
) -> Result<String, String> {
    let mut game: Game = serde_json::from_str(&row.game_data)
        .map_err(|e| format!("Failed to parse game JSON: {}", e))?;

    player_identity::upgrade_game_player_identities(&mut game);
    crate::identity_upgrade::upgrade_game_football_identities(&mut game);

    save_manager.create_save(&game, &row.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::fs;

    /// Create a legacy saves.db with the old schema and some test data.
    fn create_legacy_db(path: &Path, saves: &[(&str, &str, &str, &str)]) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE saves (
                id              TEXT PRIMARY KEY,
                name            TEXT NOT NULL,
                manager_name    TEXT NOT NULL,
                game_data       TEXT NOT NULL,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                last_played_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();

        for (id, name, manager_name, game_data) in saves {
            conn.execute(
                "INSERT INTO saves (id, name, manager_name, game_data) VALUES (?1, ?2, ?3, ?4)",
                params![id, name, manager_name, game_data],
            )
            .unwrap();
        }
    }

    /// Generate a minimal valid Game JSON.
    fn minimal_game_json() -> String {
        use crate::clock::GameClock;
        use crate::domain::player::PlayerAttributes;
        use crate::domain::staff::{StaffAttributes, StaffRole};
        use crate::domain::stats::LolRole;
        use crate::game::Game;
        use chrono::{TimeZone, Utc};

        let start = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
        let clock = GameClock::new(start);
        let manager = crate::domain::manager::Manager::new(
            "mgr-001".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1990-01-01".to_string(),
            "GB".to_string(),
        );
        let team = crate::domain::team::Team::new(
            "team-001".to_string(),
            "Test FC".to_string(),
            "TFC".to_string(),
            "GB".to_string(),
            "London".to_string(),
            "Test Stadium".to_string(),
            30000,
        );
        let player = crate::domain::player::Player::new(
            "p-001".to_string(),
            "J. Test".to_string(),
            "John Test".to_string(),
            "2000-01-01".to_string(),
            "GB".to_string(),
            LolRole::Mid,
            PlayerAttributes {
                mechanics: 50,
                laning: 50,
                teamfighting: 50,
                macro_play: 50,
                consistency: 50,
                shotcalling: 50,
                champion_pool: 50,
                discipline: 50,
                mental_resilience: 50,
            },
        );
        let staff = crate::domain::staff::Staff::new(
            "staff-001".to_string(),
            "A".to_string(),
            "Coach".to_string(),
            "1980-01-01".to_string(),
            StaffRole::Coach,
            StaffAttributes {
                coaching: 50,
                judging_ability: 50,
                judging_potential: 50,
                physiotherapy: 50,
            },
        );

        let game = Game::new(
            clock,
            manager,
            vec![team],
            vec![player],
            vec![staff],
            vec![],
        );
        serde_json::to_string(&game).unwrap()
    }

    fn legacy_game_json_with_partial_morale_core() -> String {
        let mut json: serde_json::Value =
            serde_json::from_str(&minimal_game_json()).expect("minimal game json should parse");

        json["players"][0]["morale_core"] = serde_json::json!({
            "manager_trust": 63,
            "unresolved_issue": {
                "category": "PlayingTime",
                "severity": 55
            },
            "recent_treatment": null
        });

        serde_json::to_string(&json).expect("legacy game json should serialize")
    }

    fn legacy_game_json_with_mirrored_starting_xi() -> String {
        use crate::clock::GameClock;
        use crate::domain::player::{Player, PlayerAttributes};
        use crate::domain::stats::LolRole;
        use chrono::{TimeZone, Utc};

        let start = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
        let clock = GameClock::new(start);
        let mut manager = crate::domain::manager::Manager::new(
            "mgr-001".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1990-01-01".to_string(),
            "GB".to_string(),
        );
        manager.hire("team-001".to_string());

        let mut team = crate::domain::team::Team::new(
            "team-001".to_string(),
            "Test FC".to_string(),
            "TFC".to_string(),
            "GB".to_string(),
            "London".to_string(),
            "Test Stadium".to_string(),
            30000,
        );
        // No formation field — LoL uses fixed 5-role lineup
        team.active_lineup_ids = vec!["top", "jng", "mid", "adc", "sup"]
            .into_iter()
            .map(str::to_string)
            .collect();

        let make_player = |id: &str, role: LolRole| {
            let mut player = Player::new(
                id.to_string(),
                id.to_uppercase(),
                format!("Player {}", id),
                "2000-01-01".to_string(),
                "GB".to_string(),
                role,
                PlayerAttributes {
                    mechanics: 70,
                    laning: 70,
                    teamfighting: 70,
                    macro_play: 70,
                    consistency: 70,
                    shotcalling: 70,
                    champion_pool: 70,
                    discipline: 70,
                    mental_resilience: 70,
                },
            );
            player.natural_position = role;
            player.team_id = Some("team-001".to_string());
            player
        };

        let players = vec![
            make_player("top", LolRole::Top),
            make_player("jng", LolRole::Jungle),
            make_player("mid", LolRole::Mid),
            make_player("adc", LolRole::Adc),
            make_player("sup", LolRole::Support),
        ];

        let game = Game::new(clock, manager, vec![team], players, vec![], vec![]);
        serde_json::to_string(&game).expect("legacy mirrored xi game json should serialize")
    }

    fn legacy_game_json_for_position_identity_upgrade() -> String {
        let mut json: serde_json::Value =
            serde_json::from_str(&minimal_game_json()).expect("minimal game json should parse");

        json["teams"][0]["starting_xi_ids"] = serde_json::json!(["legacy-gk", "p-001"]);
        json["players"][0]["position"] = serde_json::json!("Defender");
        json["players"][0]["natural_position"] = serde_json::json!("Defender");
        json["players"][0]["alternate_positions"] = serde_json::json!([]);
        json["players"][0]["attributes"] = serde_json::json!({
            "pace": 84,
            "stamina": 82,
            "strength": 63,
            "agility": 72,
            "passing": 64,
            "shooting": 40,
            "tackling": 77,
            "dribbling": 62,
            "defending": 72,
            "positioning": 66,
            "vision": 58,
            "decisions": 64,
            "composure": 60,
            "aggression": 64,
            "teamwork": 74,
            "leadership": 44,
            "handling": 20,
            "reflexes": 20,
            "aerial": 46
        });

        serde_json::to_string(&json).expect("legacy game json should serialize")
    }

    fn legacy_game_json_with_partial_transfer_offer() -> String {
        let mut json: serde_json::Value =
            serde_json::from_str(&minimal_game_json()).expect("minimal game json should parse");

        json["players"][0]["transfer_offers"] = serde_json::json!([
            {
                "id": "offer-legacy-1",
                "from_team_id": "team-999",
                "fee": 900000,
                "wage_offered": 0
            }
        ]);

        serde_json::to_string(&json).expect("legacy game json should serialize")
    }

    fn legacy_game_json_with_partial_facilities() -> String {
        let mut json: serde_json::Value =
            serde_json::from_str(&minimal_game_json()).expect("minimal game json should parse");

        json["teams"][0]["facilities"] = serde_json::json!({
            "training": 3
        });

        serde_json::to_string(&json).expect("legacy game json should serialize")
    }

    fn legacy_game_json_with_partial_sponsorship() -> String {
        let mut json: serde_json::Value =
            serde_json::from_str(&minimal_game_json()).expect("minimal game json should parse");

        json["teams"][0]["sponsorship"] = serde_json::json!({
            "sponsor_name": "Acme Corp"
        });

        serde_json::to_string(&json).expect("legacy game json should serialize")
    }

    fn legacy_game_json_with_partial_recent_treatment() -> String {
        let mut json: serde_json::Value =
            serde_json::from_str(&minimal_game_json()).expect("minimal game json should parse");

        json["players"][0]["morale_core"] = serde_json::json!({
            "manager_trust": 63,
            "recent_treatment": {
                "action_key": "praise"
            }
        });

        serde_json::to_string(&json).expect("legacy game json should serialize")
    }

    fn legacy_game_json_with_partial_pending_promise() -> String {
        let mut json: serde_json::Value =
            serde_json::from_str(&minimal_game_json()).expect("minimal game json should parse");

        json["players"][0]["morale_core"] = serde_json::json!({
            "manager_trust": 63,
            "pending_promise": {
                "kind": "PlayingTime"
            }
        });

        serde_json::to_string(&json).expect("legacy game json should serialize")
    }

    #[test]
    fn test_has_legacy_db() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_legacy_db(dir.path()));

        fs::write(dir.path().join("saves.db"), "").unwrap();
        assert!(has_legacy_db(dir.path()));
    }

    #[test]
    fn test_migrate_no_legacy_db() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        let mut sm = SaveManager::init(&saves_dir).unwrap();

        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_migrate_single_save() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("saves.db");
        let saves_dir = dir.path().join("saves");
        let json = minimal_game_json();

        create_legacy_db(
            &legacy_path,
            &[("old-save-1", "Test Career", "Test Manager", &json)],
        );

        let mut sm = SaveManager::init(&saves_dir).unwrap();
        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], LegacyMigrationResult::Success { .. }));

        // saves.db should be renamed
        assert!(!legacy_path.exists());
        assert!(dir.path().join("saves.db.migrated").exists());

        // New save should be loadable
        assert_eq!(sm.list_saves().len(), 1);
        let save_id = sm.list_saves()[0].id.clone();
        let loaded = sm.load_game(&save_id).unwrap();
        assert_eq!(loaded.manager.first_name, "Test");
    }

    #[test]
    fn test_migrate_multiple_saves() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("saves.db");
        let saves_dir = dir.path().join("saves");
        let json = minimal_game_json();

        create_legacy_db(
            &legacy_path,
            &[
                ("old-1", "Career 1", "Manager A", &json),
                ("old-2", "Career 2", "Manager B", &json),
                ("old-3", "Career 3", "Manager C", &json),
            ],
        );

        let mut sm = SaveManager::init(&saves_dir).unwrap();
        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();

        assert_eq!(results.len(), 3);
        for r in &results {
            assert!(matches!(r, LegacyMigrationResult::Success { .. }));
        }
        assert_eq!(sm.list_saves().len(), 3);
    }

    #[test]
    fn test_migrate_with_corrupt_json() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("saves.db");
        let saves_dir = dir.path().join("saves");
        let json = minimal_game_json();

        create_legacy_db(
            &legacy_path,
            &[
                ("good-1", "Good Save", "Manager A", &json),
                ("bad-1", "Bad Save", "Manager B", "not valid json"),
            ],
        );

        let mut sm = SaveManager::init(&saves_dir).unwrap();
        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();

        assert_eq!(results.len(), 2);

        let successes: Vec<_> = results
            .iter()
            .filter(|r| matches!(r, LegacyMigrationResult::Success { .. }))
            .collect();
        let failures: Vec<_> = results
            .iter()
            .filter(|r| matches!(r, LegacyMigrationResult::Failed { .. }))
            .collect();

        assert_eq!(successes.len(), 1);
        assert_eq!(failures.len(), 1);

        // Good save should still be loadable
        assert_eq!(sm.list_saves().len(), 1);
    }

    #[test]
    fn test_migrate_empty_legacy_db() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("saves.db");
        let saves_dir = dir.path().join("saves");

        create_legacy_db(&legacy_path, &[]);

        let mut sm = SaveManager::init(&saves_dir).unwrap();
        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();

        assert!(results.is_empty());
        assert!(!legacy_path.exists()); // Renamed even if empty
        assert!(dir.path().join("saves.db.migrated").exists());
    }

    #[test]
    fn test_migrate_legacy_save_with_partial_morale_core_defaults_new_fields() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("saves.db");
        let saves_dir = dir.path().join("saves");
        let json = legacy_game_json_with_partial_morale_core();

        create_legacy_db(
            &legacy_path,
            &[("old-save-1", "Legacy Morale Save", "Test Manager", &json)],
        );

        let mut sm = SaveManager::init(&saves_dir).unwrap();
        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], LegacyMigrationResult::Success { .. }));

        let save_id = sm.list_saves()[0].id.clone();
        let loaded = sm.load_game(&save_id).unwrap();
        let player = loaded
            .players
            .iter()
            .find(|player| player.id == "p-001")
            .unwrap();

        assert_eq!(player.morale_core.manager_trust, 63);
        assert_eq!(
            player
                .morale_core
                .unresolved_issue
                .as_ref()
                .map(|issue| issue.severity),
            Some(55)
        );
        assert_eq!(player.morale_core.pending_promise, None);
        assert_eq!(player.morale_core.talk_cooldown_until, None);
        assert_eq!(player.morale_core.renewal_state, None);
    }

    #[test]
    fn test_migrate_legacy_save_with_partial_transfer_offer_defaults_negotiation_fields() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("saves.db");
        let saves_dir = dir.path().join("saves");
        let json = legacy_game_json_with_partial_transfer_offer();

        create_legacy_db(
            &legacy_path,
            &[("old-save-2", "Legacy Transfer Save", "Test Manager", &json)],
        );

        let mut sm = SaveManager::init(&saves_dir).unwrap();
        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], LegacyMigrationResult::Success { .. }));

        let save_id = sm.list_saves()[0].id.clone();
        let loaded = sm.load_game(&save_id).unwrap();
        let player = loaded
            .players
            .iter()
            .find(|player| player.id == "p-001")
            .unwrap();

        assert_eq!(player.transfer_offers.len(), 1);
        assert_eq!(player.transfer_offers[0].id, "offer-legacy-1");
        assert_eq!(player.transfer_offers[0].from_team_id, "team-999");
        assert_eq!(player.transfer_offers[0].fee, 900_000);
        assert_eq!(format!("{:?}", player.transfer_offers[0].status), "Pending");
        assert_eq!(player.transfer_offers[0].date, "");
    }

    #[test]
    fn test_migrate_legacy_save_with_partial_facilities_defaults_missing_levels() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("saves.db");
        let saves_dir = dir.path().join("saves");
        let json = legacy_game_json_with_partial_facilities();

        create_legacy_db(
            &legacy_path,
            &[(
                "old-save-3",
                "Legacy Facilities Save",
                "Test Manager",
                &json,
            )],
        );

        let mut sm = SaveManager::init(&saves_dir).unwrap();
        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], LegacyMigrationResult::Success { .. }));

        let save_id = sm.list_saves()[0].id.clone();
        let loaded = sm.load_game(&save_id).unwrap();
        let team = loaded
            .teams
            .iter()
            .find(|team| team.id == "team-001")
            .unwrap();

        assert_eq!(team.facilities.training, 3);
        assert_eq!(team.facilities.medical, 1);
        assert_eq!(team.facilities.scouting, 1);
    }

    #[test]
    fn test_migrate_legacy_save_with_partial_sponsorship_defaults_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("saves.db");
        let saves_dir = dir.path().join("saves");
        let json = legacy_game_json_with_partial_sponsorship();

        create_legacy_db(
            &legacy_path,
            &[(
                "old-save-4",
                "Legacy Sponsorship Save",
                "Test Manager",
                &json,
            )],
        );

        let mut sm = SaveManager::init(&saves_dir).unwrap();
        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], LegacyMigrationResult::Success { .. }));

        let save_id = sm.list_saves()[0].id.clone();
        let loaded = sm.load_game(&save_id).unwrap();
        let team = loaded
            .teams
            .iter()
            .find(|team| team.id == "team-001")
            .unwrap();
        let sponsorship = team
            .sponsorship
            .as_ref()
            .expect("sponsorship should be present");

        assert_eq!(sponsorship.sponsor_name, "Acme Corp");
        assert_eq!(sponsorship.base_value, 0);
        assert_eq!(sponsorship.remaining_months, 0);
        assert!(sponsorship.bonus_criteria.is_empty());
    }

    #[test]
    fn test_migrate_legacy_save_with_partial_recent_treatment_defaults_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("saves.db");
        let saves_dir = dir.path().join("saves");
        let json = legacy_game_json_with_partial_recent_treatment();

        create_legacy_db(
            &legacy_path,
            &[(
                "old-save-5",
                "Legacy Recent Treatment Save",
                "Test Manager",
                &json,
            )],
        );

        let mut sm = SaveManager::init(&saves_dir).unwrap();
        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], LegacyMigrationResult::Success { .. }));

        let save_id = sm.list_saves()[0].id.clone();
        let loaded = sm.load_game(&save_id).unwrap();
        let player = loaded
            .players
            .iter()
            .find(|player| player.id == "p-001")
            .unwrap();
        let recent_treatment = player
            .morale_core
            .recent_treatment
            .as_ref()
            .expect("recent treatment should be present");

        assert_eq!(recent_treatment.action_key, "praise");
        assert_eq!(recent_treatment.times_recently_used, 0);
    }

    #[test]
    fn test_migrate_legacy_save_with_partial_pending_promise_defaults_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("saves.db");
        let saves_dir = dir.path().join("saves");
        let json = legacy_game_json_with_partial_pending_promise();

        create_legacy_db(
            &legacy_path,
            &[(
                "old-save-6",
                "Legacy Pending Promise Save",
                "Test Manager",
                &json,
            )],
        );

        let mut sm = SaveManager::init(&saves_dir).unwrap();
        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], LegacyMigrationResult::Success { .. }));

        let save_id = sm.list_saves()[0].id.clone();
        let loaded = sm.load_game(&save_id).unwrap();
        let player = loaded
            .players
            .iter()
            .find(|player| player.id == "p-001")
            .unwrap();
        let pending_promise = player
            .morale_core
            .pending_promise
            .as_ref()
            .expect("pending promise should be present");

        assert_eq!(format!("{:?}", pending_promise.kind), "PlayingTime");
        assert_eq!(pending_promise.matches_remaining, 0);
    }

    #[test]
    fn test_migrate_legacy_db_no_saves_table() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("saves.db");
        let saves_dir = dir.path().join("saves");

        // Create a DB with no saves table
        let conn = Connection::open(&legacy_path).unwrap();
        conn.execute_batch("CREATE TABLE other (id TEXT);").unwrap();
        drop(conn);

        let mut sm = SaveManager::init(&saves_dir).unwrap();
        let results = migrate_legacy_saves(dir.path(), &mut sm).unwrap();

        assert!(results.is_empty());
        assert!(!legacy_path.exists());
        assert!(dir.path().join("saves.db.migrated").exists());
    }
}
