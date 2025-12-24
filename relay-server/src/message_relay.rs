//! WebSocket message routing for the relay server.
//!
//! This module handles bidirectional communication between game hosts and clients.
//! It spawns paired Tokio tasks for each connection that:
//! - Validate and filter messages by type (preventing illegal commands)
//! - Route host broadcasts to subscribed clients
//! - Forward client RPCs to the host with injected player IDs
//! - Manage sync state so clients only receive deltas after a full update
//!
//! The relay server never interprets game logic — it only validates message types
//! and routes bytes between endpoints.

use axum::extract::ws::{Message, WebSocket};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use protocol::*;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::broadcast;
use tokio::sync::broadcast::Sender;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::Receiver;

/// Spawns bidirectional message handlers for a game host connection.
///
/// Creates two concurrent tasks:
/// - **Send task**: Forwards client messages (joins, disconnects, RPCs) to the host
/// - **Receive task**: Broadcasts host messages (updates, kicks) to all clients
///
/// When either task completes (connection lost, protocol error, intentional disconnect),
/// the other is aborted and the room should be cleaned up by the caller.
///
/// # Returns
/// A static string describing why the connection ended (for logging/debugging).
pub async fn handle_server_logic(
    sender: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    receiver: SplitStream<WebSocket>,
    internal_receiver: Receiver<Bytes>,
    internal_sender: broadcast::Sender<Bytes>,
) -> &'static str {
    let mut send_task =
        tokio::spawn(async move { send_logic_server(sender, internal_receiver).await });

    let mut receive_task =
        tokio::spawn(async move { receive_logic_server(receiver, internal_sender).await });

    // If any one of the tasks run to completion, we abort the other.
    let result = tokio::select! {
        res_a = &mut send_task => {receive_task.abort(); res_a},
        res_b = &mut receive_task => {send_task.abort(); res_b},
    };

    result.unwrap_or_else(|err| {
        tracing::error!(?err, "Error while handling server logic.");
        "Internal panic in server side logic."
    })
}

/// Receives messages from the game host and broadcasts them to all clients.
///
/// Allowed message types from host:
/// - [`CLIENT_GETS_KICKED`]: Remove a specific player
/// - [`DELTA_UPDATE`]: Incremental game state change
/// - [`FULL_UPDATE`]: Complete game state (for new/desynced clients)
/// - [`RESET`]: Game restart signal
/// - [`SERVER_DISCONNECTS`]: Graceful shutdown (triggers cleanup)
///
/// Any other message type is rejected as a protocol violation.
async fn receive_logic_server(
    mut receiver: SplitStream<WebSocket>,
    internal_sender: Sender<Bytes>,
) -> &'static str {
    while let Some(state) = receiver.next().await {
        match state {
            Ok(Message::Binary(bytes)) => {
                if bytes.is_empty() {
                    tracing::error!("Illegal empty message in receive logic server.");
                    return "Illegal empty message received.";
                }

                if bytes[0] == SERVER_DISCONNECTS {
                    // This something normal to be expected.
                    return "Server disconnected intentionally";
                }

                if !matches!(
                    bytes[0],
                    CLIENT_GETS_KICKED | DELTA_UPDATE | FULL_UPDATE | RESET
                ) {
                    tracing::error!(
                        message_type = bytes[0],
                        "Illegal message type Server->Client."
                    );
                    return "Illegal Server -> Client command.";
                }

                // All messages are simply passed through.
                let res = internal_sender.send(bytes);
                // An error may occur, if there are no further clients available.
                // As a rule of a thumb the server should not send any messages, if he does not know of any clients.
                // Currently logged as a warning, as it is unclear, if this is strictly avoidable.
                if let Err(error) = res {
                    tracing::warn!(?error, "Sending to no clients.");
                }
            }
            Ok(_) => {} // Ignore other messages (ping/pong handled by axum)
            Err(_) => {
                return "Connection lost.";
            }
        }
    }
    "Connection lost."
}

/// Forwards aggregated client messages to the game host.
///
/// Allowed message types to host:
/// - [`NEW_CLIENT`]: Player joined notification
/// - [`CLIENT_DISCONNECTS`]: Player left notification
/// - [`SERVER_RPC`]: Game action from a client (with player ID prepended)
///
/// This task owns the WebSocket sender lock for its lifetime to ensure
/// sequential message delivery to the host.
async fn send_logic_server(
    sender: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    mut internal_receiver: Receiver<Bytes>,
) -> &'static str {
    let mut enclosed = sender.lock().await;

    while let Some(bytes) = internal_receiver.recv().await {
        if bytes.is_empty() {
            tracing::error!("Illegal internal empty message in send logic server.");
            return "Illegal empty message received.";
        }
        if !matches!(bytes[0], NEW_CLIENT | CLIENT_DISCONNECTS | SERVER_RPC) {
            tracing::error!(
                message_type = bytes[0],
                "Unknown internal Client->Server command"
            );
            return "Unknown internal Client->Server command";
        }
        // Simply pass on the message.
        let res = enclosed.send(Message::Binary(bytes)).await;
        if let Err(err) = res {
            tracing::error!(?err, "Error in communication with server endpoint.");
            return "Error in communication with server endpoint.";
        }
    }
    // In normal shutdown procedure that should not happen, because we are responsible for closing the channel.
    tracing::error!("Internal channel on server was unexpectedly closed.");
    "Internal channel closed."
}

/// Spawns bidirectional message handlers for a game client connection.
///
/// Creates two concurrent tasks:
/// - **Send task**: Delivers host broadcasts to this client (with sync state filtering)
/// - **Receive task**: Forwards client RPCs to the host (with player ID injection)
///
/// # Arguments
/// * `player_id` - Unique identifier assigned to this client for the session
///
/// # Returns
/// A static string describing why the connection ended.
pub async fn handle_client_logic(
    sender: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    receiver: SplitStream<WebSocket>,
    internal_receiver: tokio::sync::broadcast::Receiver<Bytes>,
    internal_sender: tokio::sync::mpsc::Sender<Bytes>,
    player_id: u16,
) -> &'static str {
    let mut send_task =
        tokio::spawn(async move { send_logic_client(sender, internal_receiver, player_id).await });

    let mut receive_task =
        tokio::spawn(
            async move { receive_logic_client(receiver, internal_sender, player_id).await },
        );

    // If any one of the tasks run to completion, we abort the other.
    let result = tokio::select! {
        res_a = &mut send_task => {receive_task.abort(); res_a},
        res_b = &mut receive_task => {send_task.abort(); res_b},
    };

    result.unwrap_or_else(|err| {
        tracing::error!(?err, "Internal panic in client side logic.");
        "Internal panic in client side logic."
    })
}

/// Receives messages from a client and forwards them to the host.
///
/// Allowed message types from client:
/// - [`SERVER_RPC`]: Game action — gets player ID injected before forwarding
/// - [`CLIENT_DISCONNECTS_SELF`]: Graceful disconnect (triggers cleanup)
///
/// # Player ID Injection
/// RPC messages are transformed from `[SERVER_RPC, payload...]` to
/// `[SERVER_RPC, player_id_high, player_id_low, payload...]` so the host
/// knows which player sent the action.
async fn receive_logic_client(
    mut receiver: SplitStream<WebSocket>,
    internal_sender: tokio::sync::mpsc::Sender<Bytes>,
    player_id: u16,
) -> &'static str {
    while let Some(state) = receiver.next().await {
        match state {
            Ok(Message::Binary(bytes)) => {
                if bytes.is_empty() {
                    tracing::error!("Illegal empty message received in receive logic client.");
                    return "Illegal empty message received.";
                }
                match bytes[0] {
                    SERVER_RPC => {
                        // Inject player ID after command byte
                        let mut msg = BytesMut::with_capacity(bytes.len() + CLIENT_ID_SIZE);
                        msg.put_u8(SERVER_RPC);
                        msg.put_u16(player_id);
                        msg.put_slice(&bytes[1..]);

                        let res = internal_sender.send(msg.into()).await;
                        if let Err(error) = res {
                            tracing::error!(?error, "Error in internal broadcast.");
                            return "Error in internal broadcast.";
                        }
                    }
                    CLIENT_DISCONNECTS_SELF => {
                        return "Client disconnected intentionally";
                    }
                    _ => {
                        tracing::error!(command = ?bytes[0], "Illegal command from client.");
                        return "Illegal Command from client";
                    }
                }
            }
            Ok(_) => {} // Ignore other messages
            Err(_) => {
                return "Connection lost.";
            }
        }
    }
    "Connection lost."
}

/// Delivers host broadcasts to a specific client with sync state management.
///
/// # Sync State Machine
/// Clients start unsynced and must receive a [`FULL_UPDATE`] or [`RESET`] before
/// processing [`DELTA_UPDATE`] messages. This prevents clients from applying
/// deltas to an unknown base state.
///
/// ```text
/// [Unsynced] --FULL_UPDATE--> [Synced] --DELTA_UPDATE--> [Synced]
/// [Unsynced] --RESET-------> [Synced]
/// [Synced]   --DELTA_UPDATE--> [Synced] (forwarded)
/// [Unsynced] --DELTA_UPDATE--> [Unsynced] (dropped)
/// ```
///
/// # Filtered Messages
/// - [`CLIENT_GETS_KICKED`]: Only terminates if `player_id` matches
/// - [`SERVER_DISCONNECTS`]: Always terminates
///
/// # Error Handling
/// Returns immediately if the broadcast channel lags (buffer overflow),
/// as the client cannot recover from missed messages.
async fn send_logic_client(
    sender: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    mut internal_receiver: tokio::sync::broadcast::Receiver<Bytes>,
    player_id: u16,
) -> &'static str {
    let mut enclosed = sender.lock().await;

    let mut is_synced = false;
    loop {
        let state = internal_receiver.recv().await;
        match state {
            Err(RecvError::Closed) => {
                tracing::error!("Internal channel closed.");
                return "Internal channel closed.";
            }
            Err(RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    skipped_messages = skipped,
                    "Lagging started on internal channel."
                );
                return "Lagging on internal channel - Computer too slow.";
            }
            Ok(mut bytes) => {
                if bytes.is_empty() {
                    tracing::error!("Illegal empty message received.");
                    return "Illegal empty message received.";
                }
                match bytes[0] {
                    SERVER_DISCONNECTS => {
                        return "Server has left the game.";
                    }
                    CLIENT_GETS_KICKED => {
                        if bytes.len() < 3 {
                            tracing::error!("Malformed CLIENT_GETS_KICKED message");
                            return "Malformed message received.";
                        }
                        bytes.get_u8(); // Skip command byte
                        let meant_client = bytes.get_u16();
                        // We have to see if  we are meant.
                        if meant_client == player_id {
                            return "We got rejected by server.";
                        }
                    }
                    DELTA_UPDATE => {
                        if is_synced {
                            let res = enclosed.send(Message::Binary(bytes)).await;
                            if let Err(error) = res {
                                tracing::error!(
                                    ?error,
                                    "Error in communication with client endpoint."
                                );
                                return "Error in communication with client endpoint.";
                            }
                        }
                        // Silently drop deltas for unsynced clients
                    }
                    FULL_UPDATE => {
                        if !is_synced {
                            is_synced = true;
                            let res = enclosed.send(Message::Binary(bytes)).await;
                            if let Err(error) = res {
                                tracing::error!(
                                    ?error,
                                    "Error in communication with client endpoint."
                                );
                                return "Error in communication with client endpoint.";
                            }
                        }
                        // Drop redundant full updates for already synced clients
                    }
                    RESET => {
                        // We simply forward the message and are definitively synced here.
                        is_synced = true;
                        let res = enclosed.send(Message::Binary(bytes)).await;
                        if let Err(error) = res {
                            tracing::error!(?error, "Error in communication with client endpoint.");
                            return "Error in communication with client endpoint.";
                        }
                    }
                    _ => {
                        tracing::error!(
                            message = bytes[0],
                            "Illegal message on client side received."
                        );
                        return "Illegal message on client side received.";
                    }
                }
            }
        }
    }
}
