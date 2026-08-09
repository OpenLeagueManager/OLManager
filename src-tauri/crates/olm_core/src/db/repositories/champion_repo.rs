use crate::domain::champion::{Champion, NewChampion};
use rusqlite::{Connection, params};
use serde_json::Value;

/// Insert a new champion into the database.
pub fn insert_champion(conn: &Connection, c: &NewChampion) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO champions (name, champion_key, roles_json, counterpicks_json, synergies_json, image_tile_url, image_splash_url)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            c.name,
            c.champion_key,
            c.roles_json,
            c.counterpicks_json,
            c.synergies_json,
            c.image_tile_url,
            c.image_splash_url,
        ],
    )
    .map_err(|e| format!("Failed to insert champion: {}", e))?;

    Ok(conn.last_insert_rowid())
}

/// Seed the champions table from the champions.json file.
/// This will only insert if the table is empty (idempotent).
pub fn seed_from_json(conn: &Connection, json_content: &str) -> Result<usize, String> {
    // Check if already seeded
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM champions", [], |row| row.get(0))
        .map_err(|e| format!("Failed to check champion count: {}", e))?;

    if count > 0 {
        return Ok(0); // Already seeded
    }

    let json: Value = serde_json::from_str(json_content)
        .map_err(|e| format!("Failed to parse champions JSON: {}", e))?;

    let roles = json
        .get("data")
        .and_then(|d| d.get("roles"))
        .ok_or_else(|| "Missing data.roles in JSON".to_string())?;
    let counterpicks = json.get("data").and_then(|d| d.get("counterpicks"));
    let synergies = json.get("data").and_then(|d| d.get("synergies"));

    let roles_map = roles
        .as_object()
        .ok_or_else(|| "roles is not an object".to_string())?;

    let display_aliases = json
        .get("data")
        .and_then(|d| d.get("display_aliases"))
        .and_then(|a| a.as_object());

    let mut alias_to_key = std::collections::HashMap::new();
    if let Some(aliases) = display_aliases {
        for (alias, value) in aliases {
            if let Some(key) = value.as_str() {
                alias_to_key.insert(key.to_string(), alias.to_string());
            }
        }
    }

    let mut inserted = 0;
    for (key, value) in roles_map {
        let champion_key = key.as_str();
        // Use display alias if available (e.g., "Dr. Mundo" for "DrMundo")
        let name = alias_to_key
            .get(champion_key)
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                champion_key.replace(
                    |c: char| {
                        c.is_uppercase() && !champion_key.starts_with(|c: char| c.is_lowercase())
                    },
                    ". ",
                )
            });

        let roles_vec = value
            .as_array()
            .ok_or_else(|| format!("roles for {} is not an array", champion_key))?;
        let roles_json = serde_json::to_string(roles_vec)
            .map_err(|e| format!("Failed to serialize roles for {}: {}", champion_key, e))?;

        // Filter counterpicks/synergies where this champion is "a" (the subject)
        let champ_counterpicks = counterpicks
            .map(|arr| {
                arr.as_array()
                    .map(|items| {
                        let filtered: Vec<_> = items
                            .iter()
                            .filter(|item| {
                                item.get("a").and_then(|v| v.as_str()) == Some(champion_key)
                            })
                            .cloned()
                            .collect();
                        serde_json::to_string(&filtered).unwrap_or_default()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let champ_synergies = synergies
            .map(|arr| {
                arr.as_array()
                    .map(|items| {
                        let filtered: Vec<_> = items
                            .iter()
                            .filter(|item| {
                                item.get("a").and_then(|v| v.as_str()) == Some(champion_key)
                            })
                            .cloned()
                            .collect();
                        serde_json::to_string(&filtered).unwrap_or_default()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let new_champ = NewChampion {
            name,
            champion_key: champion_key.to_string(),
            roles_json,
            counterpicks_json: if champ_counterpicks.is_empty() {
                None
            } else {
                Some(champ_counterpicks)
            },
            synergies_json: if champ_synergies.is_empty() {
                None
            } else {
                Some(champ_synergies)
            },
            image_tile_url: Some(format!(
                "/champion-tiles/{}.webp",
                champion_key
            )),
            image_splash_url: Some(format!(
                "/champion-splash/{}.webp",
                champion_key
            )),
            meta_tier: crate::domain::champion::MetaTier::B,
        };

        insert_champion(conn, &new_champ)?;
        inserted += 1;
    }

    Ok(inserted)
}

/// Get all champions from the database, ordered by name.
pub fn get_all_champions(conn: &Connection) -> Result<Vec<Champion>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, champion_key, roles_json, counterpicks_json, synergies_json, image_tile_url, image_splash_url
             FROM champions
             ORDER BY name ASC",
        )
        .map_err(|e| format!("Failed to prepare statement: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Champion {
                id: row.get(0)?,
                name: row.get(1)?,
                champion_key: row.get(2)?,
                roles_json: row.get(3)?,
                counterpicks_json: row.get(4)?,
                synergies_json: row.get(5)?,
                image_tile_url: row.get(6)?,
                image_splash_url: row.get(7)?,
                meta_tier: crate::domain::champion::MetaTier::B,
            })
        })
        .map_err(|e| format!("Failed to query champions: {}", e))?;

    let mut champions = Vec::new();
    for champion in rows {
        champions.push(champion.map_err(|e| format!("Failed to read champion row: {}", e))?);
    }

    Ok(champions)
}

/// Get a single champion by its numeric ID.
pub fn get_champion_by_id(conn: &Connection, id: i64) -> Result<Option<Champion>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, champion_key, roles_json, counterpicks_json, synergies_json, image_tile_url, image_splash_url
             FROM champions
             WHERE id = ?1",
        )
        .map_err(|e| format!("Failed to prepare statement: {}", e))?;

    let mut rows = stmt
        .query_map(params![id], |row| {
            Ok(Champion {
                id: row.get(0)?,
                name: row.get(1)?,
                champion_key: row.get(2)?,
                roles_json: row.get(3)?,
                counterpicks_json: row.get(4)?,
                synergies_json: row.get(5)?,
                image_tile_url: row.get(6)?,
                image_splash_url: row.get(7)?,
                meta_tier: crate::domain::champion::MetaTier::B,
            })
        })
        .map_err(|e| format!("Failed to query champion: {}", e))?;

    Ok(rows
        .next()
        .transpose()
        .map_err(|e| format!("Failed to read champion: {}", e))?)
}

/// Get a single champion by its champion_key (the JSON ID like "Aatrox").
pub fn get_champion_by_key(conn: &Connection, key: &str) -> Result<Option<Champion>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, champion_key, roles_json, counterpicks_json, synergies_json, image_tile_url, image_splash_url
             FROM champions
             WHERE champion_key = ?1",
        )
        .map_err(|e| format!("Failed to prepare statement: {}", e))?;

    let mut rows = stmt
        .query_map(params![key], |row| {
            Ok(Champion {
                id: row.get(0)?,
                name: row.get(1)?,
                champion_key: row.get(2)?,
                roles_json: row.get(3)?,
                counterpicks_json: row.get(4)?,
                synergies_json: row.get(5)?,
                image_tile_url: row.get(6)?,
                image_splash_url: row.get(7)?,
                meta_tier: crate::domain::champion::MetaTier::B,
            })
        })
        .map_err(|e| format!("Failed to query champion: {}", e))?;

    Ok(rows
        .next()
        .transpose()
        .map_err(|e| format!("Failed to read champion: {}", e))?)
}

/// Delete all champions (useful for reseeding).
pub fn delete_all_champions(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM champions", [])
        .map_err(|e| format!("Failed to delete champions: {}", e))?;
    Ok(())
}

