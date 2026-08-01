use crate::domain::player::{LolRole, Player};
use crate::game::Game;

/// Upgrades player identities to use LolRole positions.
/// Now that all players already use LolRole, this is a no-op.
pub fn upgrade_game_player_identities(_game: &mut Game) -> bool {
    false
}

/// Upgrades a single player's identity.
/// Now that players already use LolRole, this is a no-op.
pub fn upgrade_player_identity(_player: &mut Player, _assigned_slot: Option<&LolRole>) -> bool {
    false
}

/// Determines if a player needs identity upgrade.
/// With LolRole, all players are already in the correct format.
#[allow(dead_code)]
fn needs_identity_upgrade(_player: &Player) -> bool {
    false
}
