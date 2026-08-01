pub mod academy;
pub mod bug_report;
pub mod club;
pub mod competitions;
pub mod contracts;
pub mod discord;
pub mod game;
pub mod import;
pub mod jobs;
pub mod live_match;
pub mod messages;
pub mod round_summary;
pub mod season;
pub mod settings;
pub mod sim_live;
pub mod social;
pub mod squad;
pub mod staff;
pub mod stats;
pub mod time;
pub mod transfers;
pub use academy::*;
pub use bug_report::*;
pub use club::*;
pub use competitions::*;
pub use contracts::*;
pub use discord::*;
pub use game::*;
pub use import::*;
pub use jobs::*;
pub use live_match::*;
pub use messages::*;
pub use season::*;
pub use settings::*;
pub use sim_live::*;
pub use social::*;
pub use squad::*;
pub use staff::*;
pub use stats::*;
pub use time::*;
pub use transfers::*;

#[tauri::command]
pub fn debug_log(message: String) {
    println!("[JS DEBUG] {}", message);
}

#[tauri::command]
pub fn update_manager_profile(
    state: tauri::State<'_, olm_core::state::StateManager>,
    nickname: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    dob: Option<String>,
    nationality: Option<String>,
    avatar_path: Option<String>,
) -> Result<serde_json::Value, String> {
    let mut game = state
        .get_game(|g| g.clone())
        .ok_or("No active game session".to_string())?;

    if let Some(v) = first_name {
        game.manager.first_name = v;
    }
    if let Some(v) = last_name {
        game.manager.last_name = v;
    }
    if let Some(v) = nickname {
        game.manager.nickname = v;
    }
    if let Some(v) = nationality {
        game.manager.nationality = v;
    }
    if let Some(v) = dob {
        game.manager.date_of_birth = v;
    }
    if let Some(v) = avatar_path {
        game.manager.avatar_path = Some(v);
    }

    state.set_game(game.clone());
    Ok(serde_json::to_value(&game).map_err(|e| e.to_string())?)
}
