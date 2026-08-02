use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use uuid::Uuid;

use crate::discord_rpc::DiscordRpcState;

/// Discord application ID.
///
/// ## Imágenes en el Rich Presence
///
/// 1. Andá a https://discord.com/developers/applications/1514763311646900295/rich-presence/assets
/// 2. Subí las imágenes (logo, iconos por estado, etc.)
/// 3. Discord te asigna un **asset key** a cada imagen (ej: "logo", "squad_icon")
/// 4. Poné ese key en `large_image` o `small_image` en `state_key_to_payload()`
const APP_ID: &str = "1514763311646900295";

/// Serializable payload for Discord Rich Presence activity data.
///
/// Used internally to build Discord activities from state-key mappings.
/// Exists as a first-class type so serde round-trips can be verified in tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordActivityPayload {
    pub state: String,
    pub details: String,
    pub large_image: Option<String>,
    pub large_text: Option<String>,
    pub small_image: Option<String>,
    pub small_text: Option<String>,
    pub start_timestamp: Option<i64>,
}

/// Maps a frontend state key to a `DiscordActivityPayload`.
fn state_key_to_payload(key: &str) -> DiscordActivityPayload {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let base = |state: &str, details: &str| DiscordActivityPayload {
        state: state.into(),
        details: details.into(),
        large_image: Some("logo".into()),
        large_text: Some("Open League Manager".into()),
        small_image: None,
        small_text: None,
        start_timestamp: Some(now),
    };

    match key {
        "dashboard" => base("Browsing Dashboard", "Managing the team"),
        "squad" => base("Managing Squad", "Setting lineups and roles"),
        "tactics" => base("Setting Tactics", "Planning match strategy"),
        "training" => base("In Training", "Improving player skills"),
        "scouting" => base("Scouting Players", "Searching for talent"),
        "finances" => base("Managing Finances", "Balancing the books"),
        "match" => base("In a Match", "Playing live!"),
        "transfers" => base("Making Transfers", "Negotiating deals"),
        "market" => base("In the Transfer Market", "Browsing available players"),
        "players" => base("Viewing Players", "Reviewing player profiles"),
        "teams" => base("Viewing Teams", "Scouting the competition"),
        "staff" => base("Managing Staff", "Hiring and releasing"),
        "competitions" => base("Viewing Competitions", "Checking standings and fixtures"),
        "scrims" => base("Setting Up Scrims", "Organising practice matches"),
        "youth" => base("Youth Academy", "Developing future stars"),
        "inbox" => base("Checking Messages", "Reading the inbox"),
        "social" => base("On Social Media", "Engaging with fans"),
        "news" => base("Reading News", "Staying updated"),
        "settings" => base("Configuring Settings", "Adjusting preferences"),
        "main_menu" => base("In the Main Menu", "OLManager"),
        _ => base("Playing", "Open League Manager"),
    }
}

/// Builds the JSON activity payload for Discord's SET_ACTIVITY IPC command.
///
/// Discord's local RPC protocol does NOT support custom buttons — only the
/// `state`, `details`, `timestamps`, and `assets` fields are honoured.
fn build_activity_json(payload: &DiscordActivityPayload) -> serde_json::Value {
    let mut activity = serde_json::json!({
        "state": payload.state,
        "details": payload.details,
        "timestamps": { "start": payload.start_timestamp.unwrap_or(0) },
    });

    if let Some(ref img) = payload.large_image {
        let mut assets = serde_json::json!({
            "large_image": img,
        });
        if let Some(ref txt) = payload.large_text {
            assets["large_text"] = serde_json::json!(txt);
        }
        activity["assets"] = assets;
    }

    activity
}

/// Initializes the Discord RPC client and connects to Discord.
///
/// Returns `true` if the client was successfully connected, `false` if Discord
/// is not available (graceful degradation). Subsequent calls are no-ops if the
/// client is already active.
///
/// After connecting, drains any `pending_key` that was queued by
/// `update_discord_presence` while the client was still `None`, ensuring no
/// presence update is lost during the startup race window.
#[tauri::command]
pub async fn init_discord_rpc(state: State<'_, DiscordRpcState>) -> Result<bool, String> {
    let mut guard = state.0.lock().map_err(|e| format!("Lock error: {}", e))?;

    if guard.client.is_some() {
        return Ok(true); // Already initialized
    }

    let mut client = DiscordIpcClient::new(APP_ID);

    match client.connect() {
        Ok(()) => {
            log::info!("[discord] RPC client connected");

            // Drain any presence update that arrived before init completed.
            if let Some(pending) = guard.pending_key.take() {
                log::debug!("[discord] Applying pending presence key: {pending}");
                let payload = state_key_to_payload(&pending);
                let activity_json = build_activity_json(&payload);
                let data = serde_json::json!({
                    "cmd": "SET_ACTIVITY",
                    "args": {
                        "pid": std::process::id(),
                        "activity": activity_json
                    },
                    "nonce": Uuid::new_v4().to_string()
                });
                if let Err(e) = client.send(data, 1) {
                    log::warn!("[discord] Failed to apply pending activity: {e}");
                }
            }

            guard.client = Some(client);
            Ok(true)
        }
        Err(e) => {
            log::warn!("[discord] Failed to connect to Discord IPC: {}", e);
            Ok(false)
        }
    }
}

/// Updates the Discord Rich Presence with the activity mapped from a state key.
///
/// Uses raw IPC send instead of `client.set_activity()`.
///
/// If the RPC client is not yet connected (`None`), the key is queued in
/// `pending_key` so that `init_discord_rpc` can apply it as soon as the IPC
/// connection is established.
#[tauri::command]
pub async fn update_discord_presence(
    state: State<'_, DiscordRpcState>,
    state_key: String,
) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| format!("Lock error: {}", e))?;

    let Some(client) = guard.client.as_mut() else {
        guard.pending_key = Some(state_key);
        return Ok(());
    };

    let payload = state_key_to_payload(&state_key);
    let activity_json = build_activity_json(&payload);

    let data = serde_json::json!({
        "cmd": "SET_ACTIVITY",
        "args": {
            "pid": std::process::id(),
            "activity": activity_json
        },
        "nonce": Uuid::new_v4().to_string()
    });

    client
        .send(data, 1)
        .map_err(|e| format!("Failed to send Discord activity: {}", e))?;

    Ok(())
}

/// Shuts down the Discord RPC client gracefully.
///
/// Also clears any queued `pending_key`.  Safe to call when no client is
/// active — it's a no-op.
#[tauri::command]
pub async fn shutdown_discord_rpc(state: State<'_, DiscordRpcState>) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| format!("Lock error: {}", e))?;

    guard.pending_key = None;

    if let Some(mut client) = guard.client.take() {
        if let Err(e) = client.close() {
            log::warn!("[discord] Error closing RPC client: {}", e);
        } else {
            log::info!("[discord] RPC client disconnected");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // DiscordActivityPayload serde round-trips (task 4.1)
    // -----------------------------------------------------------------------

    #[test]
    fn test_payload_serde_round_trip_dashboard() {
        let payload = state_key_to_payload("dashboard");
        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: DiscordActivityPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload.state, deserialized.state);
        assert_eq!(payload.details, deserialized.details);
        assert!(deserialized.start_timestamp.is_some());
    }

    #[test]
    fn test_payload_serde_round_trip_squad() {
        let payload = state_key_to_payload("squad");
        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: DiscordActivityPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.state, "Managing Squad");
        assert_eq!(deserialized.details, "Setting lineups and roles");
    }

    #[test]
    fn test_payload_serde_round_trip_match() {
        let payload = state_key_to_payload("match");
        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: DiscordActivityPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.state, "In a Match");
        assert!(deserialized.start_timestamp.is_some());
    }

    #[test]
    fn test_payload_serde_round_trip_transfers() {
        let payload = state_key_to_payload("transfers");
        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: DiscordActivityPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.state, "Making Transfers");
    }

    #[test]
    fn test_payload_serde_round_trip_settings() {
        let payload = state_key_to_payload("settings");
        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: DiscordActivityPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.state, "Configuring Settings");
    }

    #[test]
    fn test_payload_unknown_key_falls_back_to_playing() {
        let payload = state_key_to_payload("nonexistent_route");
        assert_eq!(payload.state, "Playing");
        assert_eq!(payload.details, "Open League Manager");
    }

    #[test]
    fn test_payload_serializes_all_fields() {
        let payload = state_key_to_payload("dashboard");
        let json = serde_json::to_value(&payload).unwrap();
        assert!(json.get("state").is_some());
        assert!(json.get("details").is_some());
        assert!(json.get("start_timestamp").is_some());
        assert!(json.get("large_image").is_some());
    }

    // -----------------------------------------------------------------------
    // State-key mapping tests (task 4.2 coverage)
    // -----------------------------------------------------------------------

    #[test]
    fn test_state_key_mapping_returns_correct_state_text() {
        let cases = [
            ("dashboard", "Browsing Dashboard"),
            ("squad", "Managing Squad"),
            ("tactics", "Setting Tactics"),
            ("training", "In Training"),
            ("scouting", "Scouting Players"),
            ("finances", "Managing Finances"),
            ("match", "In a Match"),
            ("transfers", "Making Transfers"),
            ("market", "In the Transfer Market"),
            ("players", "Viewing Players"),
            ("teams", "Viewing Teams"),
            ("staff", "Managing Staff"),
            ("competitions", "Viewing Competitions"),
            ("scrims", "Setting Up Scrims"),
            ("youth", "Youth Academy"),
            ("inbox", "Checking Messages"),
            ("social", "On Social Media"),
            ("news", "Reading News"),
            ("settings", "Configuring Settings"),
            ("main_menu", "In the Main Menu"),
        ];
        for (key, expected) in &cases {
            let payload = state_key_to_payload(key);
            assert_eq!(
                &payload.state, expected,
                "key '{}' should map to '{}'",
                key, expected
            );
        }
    }

    #[test]
    fn test_state_key_timestamp_is_current_time() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let payload = state_key_to_payload("dashboard");
        let ts = payload.start_timestamp.unwrap_or(0);
        assert!(
            (now - ts).abs() <= 5,
            "timestamp should be close to current time (now={}, ts={})",
            now,
            ts
        );
    }

    // -----------------------------------------------------------------------
    // Activity construction tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_activity_json_includes_correct_fields() {
        let payload = state_key_to_payload("squad");
        let json = build_activity_json(&payload);
        assert_eq!(
            json.get("state").and_then(|v| v.as_str()),
            Some("Managing Squad")
        );
        assert_eq!(
            json.get("details").and_then(|v| v.as_str()),
            Some("Setting lineups and roles")
        );
        assert!(json.get("timestamps").is_some());
        assert!(json.get("assets").is_some());
        // Buttons are not supported via local RPC protocol
        assert!(
            json.get("buttons").is_none(),
            "buttons are not supported via RPC"
        );
    }
}
