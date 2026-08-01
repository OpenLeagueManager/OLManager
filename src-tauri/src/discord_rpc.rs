use discord_rich_presence::DiscordIpcClient;
use std::sync::Mutex;

/// Inner state held behind a `Mutex` inside `DiscordRpcState`.
pub struct DiscordRpcInner {
    /// Connected RPC client, or `None` when Discord is unavailable / not yet
    /// initialised.
    pub client: Option<DiscordIpcClient>,
    /// State key queued by `update_discord_presence` while the client was
    /// still `None`.  `init_discord_rpc` drains this after connecting so no
    /// presence update is lost during the startup race window.
    pub pending_key: Option<String>,
}

/// Tauri-managed wrapper around an optional Discord IPC client.
///
/// `client` is `None` when Discord is not running or initialisation failed —
/// the system degrades gracefully.  A `pending_key` slot prevents lost
/// presence updates that arrive while `init_discord_rpc` is still connecting.
pub struct DiscordRpcState(pub Mutex<DiscordRpcInner>);

impl DiscordRpcState {
    /// Creates a new state with no active client and no pending key.
    pub fn new() -> Self {
        Self(Mutex::new(DiscordRpcInner {
            client: None,
            pending_key: None,
        }))
    }
}

impl Default for DiscordRpcState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_client_is_none() {
        let state = DiscordRpcState::new();
        let guard = state.0.lock().unwrap();
        assert!(
            guard.client.is_none(),
            "fresh state should hold None client"
        );
    }

    #[test]
    fn test_initial_state_pending_key_is_none() {
        let state = DiscordRpcState::new();
        let guard = state.0.lock().unwrap();
        assert!(
            guard.pending_key.is_none(),
            "fresh state should have no pending key"
        );
    }

    #[test]
    fn test_lock_acquire_release_preserves_client() {
        let state = DiscordRpcState::new();

        {
            let mut guard = state.0.lock().unwrap();
            assert!(guard.client.is_none());
            guard.client = Some(DiscordIpcClient::new("1495804489943351398"));
            assert!(guard.client.is_some());
        }

        let guard = state.0.lock().unwrap();
        assert!(
            guard.client.is_some(),
            "client should persist after lock release"
        );
    }

    #[test]
    fn test_take_client_returns_none_when_inactive() {
        let state = DiscordRpcState::new();
        let mut guard = state.0.lock().unwrap();

        let client = guard.client.take();
        assert!(client.is_none(), "taking from None should return None");

        guard.client = client;
        assert!(guard.client.is_none());
    }

    #[test]
    fn test_pending_key_is_drained_after_connect() {
        let state = DiscordRpcState::new();

        // Simulate: update arrives before init
        {
            let mut guard = state.0.lock().unwrap();
            guard.pending_key = Some("squad".into());
        }

        // Simulate: init completes
        {
            let mut guard = state.0.lock().unwrap();
            guard.client = Some(DiscordIpcClient::new("1495804489943351398"));
            if let Some(key) = guard.pending_key.take() {
                assert_eq!(key, "squad");
            }
        }

        assert!(state.0.lock().unwrap().pending_key.is_none());
    }
}
