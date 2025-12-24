//! This module handles game rooms where players connect and exchange messages.
//! It provides:
//! - [`Room`]: A game session with host-to-client broadcast channels
//! - [`AppState`]: Global state holding all active rooms and game configurations
//! - [`reload_config`]: Hot-reloading of game settings from `GameConfig.json`


use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use bytes::Bytes;
use tokio::fs;
use tokio::sync::{Mutex, RwLock};
use tokio::sync::{broadcast, mpsc};

/// The game entry we have for one game.
#[derive(Serialize, Deserialize)]
pub struct GameEntry {
    /// The name of the game.
    pub name: String,
    /// The maximum amount of players (0 = no limit)
    pub max_players: u16,
}

type EntryList = Vec<GameEntry>;

/// The description of the room, the players play in
pub struct Room {
    /// The next id a client gets, this is consecutively counted.
    pub next_client_id: u16, // Needs Mutex
    /// The amount of players currently in the room.
    pub amount_of_players: u16, // Needs mutex.
    /// This is a status counter for rule variation in a game (like coop vs semi-coop).
    pub rule_variation: u16,
    /// The sender to send messages to the host.
    pub to_host_sender: mpsc::Sender<Bytes>, // Clone-able no Mutex!
    /// The broad case sender needed to subscribe for the clients.
    pub host_to_client_broadcaster: broadcast::Sender<Bytes>, // Clone-able -> no Mutex!
}

/// The application state.
#[derive(Default)]
pub struct AppState {
    /// The rooms we associate with several sessions.
    pub rooms: Mutex<HashMap<String, Room>>,
    /// Contains a mapping from game name to the maximum amount of players allowed.
    pub configs: RwLock<HashMap<String, u16>>,
}

/// Reloads the configuration file, that lists the games with the maximum number of players per room.
pub async fn reload_config(state: &Arc<AppState>) -> Result<(), String> {
    let json_content = fs::read_to_string("GameConfig.json")
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;
    let raw_data: EntryList =
        serde_json::from_str(&json_content).map_err(|e| format!("Failed to parse JSON: {}", e))?;
    let new_configs: HashMap<String, u16> = raw_data
        .into_iter()
        .map(|entry| (entry.name, entry.max_players))
        .collect();

    {
        let mut configs = state.configs.write().await;
        *configs = new_configs; // Replace all.
    }
    Ok(())
}
