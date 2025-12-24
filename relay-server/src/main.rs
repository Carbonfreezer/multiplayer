mod hand_shake;
mod processing_module;
mod lobby;

use crate::hand_shake::{
    ClientServerSpecificData, DisconnectData, inform_client_of_connection, init_and_connect,
    shutdown_connection,
};
use crate::processing_module::{handle_client_logic, handle_server_logic};
use crate::lobby::{AppState, reload_config};
use axum::Router;
use axum::extract::ws::WebSocket;
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use futures_util::stream::StreamExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
/// Activates error tracing, spawns a watch dog task to eliminate eventual  dead rooms, then it sets up the roting system to serve the
/// web sockets and listen for the pages enlist and reload. The server listens on port 8080.
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=trace", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_file(true)
                .with_line_number(true)
                .with_target(true) // Modul-Path (e.g. relay_server::processing_module)
                .with_thread_ids(true) // Thread-ID (helpful for Tokio)
                .with_thread_names(true), // Thread-Name
        )
        .init();

    let app_state = Arc::new(AppState::default());
    let watchdog_state = app_state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1200)); // 20 Min
        loop {
            interval.tick().await;
            cleanup_dead_rooms(&watchdog_state).await;
        }
    });

    let initial = reload_config(&app_state).await;
    if let Err(message) = initial {
        tracing::error!(message, "Initial load error.");
        panic!("Initial load error: {}", message);
    }


    let app = Router::new()
        .route("/reload", get(reload_handler))
        .route("/enlist", get(enlist_handler))
        .route("/ws", get(websocket_handler))
        .with_state(app_state)
        .fallback_service(ServeDir::new(".").not_found_service(ServeFile::new("index.html")));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}

/// Runs over all rooms and checks if they are diconnected from the server.
/// If so, it cleans them up. This is a fallback solution things should be handled internally otherwise.
async fn cleanup_dead_rooms(state: &Arc<AppState>) {
    let mut rooms = state.rooms.lock().await;
    rooms.retain(|room_id, room| {
        let is_alive = !room.to_host_sender.is_closed();
        if !is_alive {
            tracing::info!("Removing dead room: {}", room_id);
        }
        is_alive
    });
}

/// Generates a list with the current rooms, the amount of players and info if this is a dead room.
async fn enlist_handler(State(state): State<Arc<AppState>>) -> String {
    let rooms = state.rooms.lock().await;
    rooms
        .iter()
        .map(|(name, room)| {
            format!(
                "Room: {:<30}  Variation: {:03} Players: {:03} is alive: {}",
                name,
                room.rule_variation,
                room.amount_of_players,
                !room.to_host_sender.is_closed()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Forces the reload of the config file and lists the content. This enables the adding of new games
/// without restarting the service.
async fn reload_handler(State(state): State<Arc<AppState>>) -> String {
    let error = reload_config(&state).await;
    match error {
        Ok(_) => state
            .configs
            .read()
            .await
            .iter()
            .map(|(key, players)| {
                format!("Game: {:<40} Maximum Amount of Players: {}", key, players)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Err(e) => {
            format!("Config reload failed: {}", e)
        }
    }
}

/// This function gets immediately called and upgrades the web response to a web socket.
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| websocket(socket, state))
}

/// Does the whole handling from start to finish: Handshake -> Handling of logic depending on if we are connected to
/// the server or client -> Shut down processing.
async fn websocket(stream: WebSocket, state: Arc<AppState>) {
    // By splitting, we can send and receive at the same time.
    let (mut sender, mut receiver) = stream.split();

    let handshake_result = init_and_connect(&mut sender, &mut receiver, state.clone()).await;
    if handshake_result.is_none() {
        // We quit here, as the handshake did not work out.
        return;
    }
    let base_data = handshake_result.unwrap();

    let disconnect_data = DisconnectData::from(&base_data);
    let success = inform_client_of_connection(&mut sender, &base_data).await;
    let wrapped_sender = Arc::new(Mutex::new(sender));
    let mut error_message = "Connection to server lost";
    if success {
        match base_data.specific_data {
            ClientServerSpecificData::Server(internal_receiver, internal_sender) => {
                error_message = handle_server_logic(
                    wrapped_sender.clone(),
                    receiver,
                    internal_receiver,
                    internal_sender,
                )
                .await;
            }
            ClientServerSpecificData::Client(internal_receiver, internal_sender) => {
                error_message = handle_client_logic(
                    wrapped_sender.clone(),
                    receiver,
                    internal_receiver,
                    internal_sender,
                    base_data.player_id,
                )
                .await;
            }
        }
    }

    shutdown_connection(wrapped_sender, disconnect_data, state, error_message).await;
}
