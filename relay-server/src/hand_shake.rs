//! This module does the whole initialization and handshake thing.
//! The general protocol of connecting is :
//! WASM Client -> Websocket: postcard serialized join request.
//! Websocket -> WASM Client: u16 player id, u16 rule variation.

use protocol::{CHANNEL_BUFFER_SIZE, CLIENT_DISCONNECT_MSG_SIZE, CLIENT_DISCONNECTS, HAND_SHAKE_RESPONSE_SIZE, NEW_CLIENT, NEW_CLIENT_MSG_SIZE, SERVER_DISCONNECT_MSG_SIZE, SERVER_DISCONNECTS, SERVER_ERROR, HAND_SHAKE_RESPONSE, JoinRequest};
use crate::hand_shake::ClientServerSpecificData::{Client, Server};
use crate::hand_shake::DisconnectEndpointSpecification::{DisconnectClient, DisconnectServer};
use crate::server_state::{AppState, Room};
use axum::extract::ws::Message::Binary;
use axum::extract::ws::{ Message, WebSocket};
use bytes::{BufMut, Bytes, BytesMut};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{sink::SinkExt, stream::StreamExt};
use postcard::from_bytes;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{broadcast, mpsc};

/// Is called on error, sends a text message because e-websocket can not interpret closing messages.
/// This text message is encoded as a binary message.
async fn send_closing_message( sender: &mut SplitSink<WebSocket, Message>, closing_message: String) {

    let raw_data = closing_message.as_bytes();
    let mut msg = BytesMut::with_capacity(1 + raw_data.len());
    msg.put_u8(SERVER_ERROR);
    msg.put_slice(raw_data);

    let _ = sender.send(Message::Binary(msg.into())).await;
    let _ =sender.send(Message::Close(None)).await;
}

/// The handshake result we get for the joining the room.
pub struct HandshakeResult {
    /// The id of the player we play.
    pub player_id: u16,
    /// The complete identifier of the room as stored in the hashmap.
    pub room_id: String,
    /// The rule variation we apply.
    pub rule_variation: u16,
    /// The internal connection information.
    pub specific_data: ClientServerSpecificData,
}

/// Contains all the channel information for internal communication.
pub enum ClientServerSpecificData {
    /// In this case we are servicing the server.
    Server(Receiver<Bytes>, broadcast::Sender<Bytes>),
    /// In this case we are servicing a client.
    Client(broadcast::Receiver<Bytes>, Sender<Bytes>),
}

/// This data is data we need to keep for the disconnect handling and cleanup.
pub struct DisconnectData {
    /// The id of the player we play.
    pub player_id: u16,
    /// The complete identifier of the room as stored in the hashmap.
    pub room_id: String,
    /// The sender we use.
    pub sender: DisconnectEndpointSpecification,
}

/// Contains the information where to send error data to in case of disconnection.
pub enum DisconnectEndpointSpecification {
    /// If we are servicing the server, we broadcast the info to all clients.
    DisconnectServer(broadcast::Sender<Bytes>),
    /// If we are servicing the client, we send data to the server.
    DisconnectClient(Sender<Bytes>),
}


/// Construction of DisconnectData from Handshake result.
impl From<&HandshakeResult> for DisconnectData {
    fn from(value: &HandshakeResult) -> Self {
        match &value.specific_data {
            Server(_, internal_sender) => DisconnectData {
                player_id: value.player_id,
                room_id: value.room_id.clone(),
                sender: DisconnectServer(internal_sender.clone()),
            },
            Client(_, internal_sender) => DisconnectData {
                player_id: value.player_id,
                room_id: value.room_id.clone(),
                sender: DisconnectClient(internal_sender.clone()),
            },
        }
    }
}

/// Gets an initial connection result, where a room is constructed
/// and game and existence / non existence of room is checked for legality.
struct InitialConnectionResult {
    /// Flags, if we are a server.
    is_server: bool,
    /// The complete room we have for internal administration.
    compound_room_id: String,
    /// Which game do we want to join.
    game_id: String,
    /// Which room do we want to join.
    room_id: String,
    /// The rule variation that is applied, this gets only interpreted if a room gets constructed.
    rule_variation: u16,
    /// The maximum amount of players a room allows (0 = infinite).
    max_players: u16,
}


/// Reads in the join request from the web socket, verifies if game exists and generates the final room name.
async fn get_initial_query(
    sender: &mut SplitSink<WebSocket, Message>,
    receiver: &mut SplitStream<WebSocket>,
    state: Arc<AppState>,
) -> Option<InitialConnectionResult> {
    // First we get a room opening and joining request. This is the first binary message we received.
    let my_data = loop {
        let Some(raw_data) = receiver.next().await else {
            tracing::warn!("WebSocket closed before handshake completed");
            send_closing_message(sender, "Initial error during handshake.".into()).await;
            return None;
        };
        match raw_data {
            Err(err) => {
                tracing::error!(?err, "Initial error during handshake.");
                send_closing_message(sender, "Initial error during handshake.".into()).await;
                return None;
            }
            Ok(Binary(data)) => {
                break data;
            }
            // We do not care about any other message like ping pong messages.
            Ok(_) => {}
        }
    };

    // Now we get some data and we try to convert it into the required format.
    let working_struct = match from_bytes::<JoinRequest>(&my_data) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!(error = ?e, "Failed to parse join request");
            send_closing_message(sender, "Failed to parse join request.".into()).await;
            return None;
        }
    };

    // Let us take a look, if the game exists.
    let games = state.configs.read().await;
    let game_exists = games.contains_key(&working_struct.game_id);
    let max_players = if game_exists {
        games[&working_struct.game_id]
    } else {
        0
    };
    drop(games);

    if !game_exists {
        tracing::error!(
            optional_game = working_struct.game_id,
            "Requested illegal game."
        );
        send_closing_message(sender, format!("Unknown game {}.", &working_struct.game_id)).await;
        return None;
    }

    // The final room id is the combination of game and room id.
    let room_id = format!(
        "{}#{}",
        working_struct.room_id.as_str(),
        working_struct.game_id.as_str()
    );
    let is_server = working_struct.create_room;

    Some(InitialConnectionResult {
        is_server,
        compound_room_id: room_id,
        game_id: working_struct.game_id,
        room_id: working_struct.room_id,
        rule_variation: working_struct.rule_variation,
        max_players,
    })
}

/// Connects and eventually establishes a room.
pub async fn init_and_connect(
    sender: &mut SplitSink<WebSocket, Message>,
    receiver: &mut SplitStream<WebSocket>,
    state: Arc<AppState>,
) -> Option<HandshakeResult> {

    let start_result = get_initial_query(sender, receiver, state.clone()).await?;

    if start_result.is_server {
        process_handshake_server(sender, state, start_result).await
    } else {
        process_handshake_client(sender, state, start_result).await
    }
}

/// Does the handshake, if we are connected to a client.
async fn process_handshake_client(
    sender: &mut SplitSink<WebSocket, Message>,
    state: Arc<AppState>,
    initial_result: InitialConnectionResult,
) -> Option<HandshakeResult> {
    let mut rooms = state.rooms.lock().await;
    let Some(local_room) = rooms.get_mut(&initial_result.compound_room_id) else {
        drop(rooms);
        send_closing_message(sender, format!(
                    "Room {} does not exist for game {}.",
                    &initial_result.room_id, &initial_result.game_id)).await;
        return None;
    };

    // Do we fit in? max_players == 0 means "infinite".
    // Use >= so we reject if the room is already at/over capacity (defensive if state was inconsistent).
    if initial_result.max_players != 0 && local_room.amount_of_players >= initial_result.max_players {
        drop(rooms);
        send_closing_message(sender,  format!(
            "Room  {} exceeded max amount of players {}.",
            &initial_result.room_id, initial_result.max_players
        )).await;
        return None;
    }
    
    // Save guard against the case, that we have run out of client ids.
    if local_room.next_client_id > 32700 {
        drop(rooms);
        send_closing_message(sender,  format!(
            "Room {} run out of client ids.",
            &initial_result.room_id
        )).await;
        tracing::error!( "Server run out of client ids.");
        return None;
    }

    local_room.amount_of_players += 1;
    let player_id = local_room.next_client_id;
    local_room.next_client_id += 1;

    let to_server_sender = local_room.to_host_sender.clone();
    let receiver = local_room.host_to_client_broadcaster.subscribe();
    let rule_variation = local_room.rule_variation;
    drop(rooms);

    // Here we send a message to the server, that a new client has joined.
    let mut msg = BytesMut::with_capacity(NEW_CLIENT_MSG_SIZE);
    msg.put_u8(NEW_CLIENT); // Message-Type
    msg.put_u16(player_id); // player id.

    let result = to_server_sender.send(msg.into()).await;
    if let Err(error) = result {
        // We have to leave the room again.
        let mut rooms = state.rooms.lock().await;
        if let Some(room) = rooms.get_mut(&initial_result.compound_room_id) {
            room.amount_of_players -= 1;
        }
        drop(rooms);
        tracing::error!(?error, "Server unexpectedly left during handshake");
        send_closing_message(sender,  "Server unexpectedly left during handshake".into()).await;
        return None;
    }

    let hand_shake_result = HandshakeResult {
        room_id: initial_result.compound_room_id,
        player_id,
        rule_variation,
        specific_data: Client(receiver, to_server_sender),
    };

    Some(hand_shake_result)
}

/// Opens a new room and generates the handshake result for the server.
async fn process_handshake_server(
    sender: &mut SplitSink<WebSocket, Message>,
    state: Arc<AppState>,
    initial_result: InitialConnectionResult,
) -> Option<HandshakeResult> {
    let mut rooms = state.rooms.lock().await;
    if rooms.contains_key(&initial_result.compound_room_id) {
        drop(rooms);
        send_closing_message(sender, format!(
            "Room {} already exists for game {}.",
            &initial_result.room_id, &initial_result.game_id
        )).await;
        // User error no need for error tracing.
        return None;
    }
    // Here we create a new room.
    let (to_server_sender, to_server_receiver) = mpsc::channel(CHANNEL_BUFFER_SIZE);
    let (to_client_sender, _) = broadcast::channel(CHANNEL_BUFFER_SIZE);
    let new_room = Room {
        next_client_id: 1,
        amount_of_players: 1,
        rule_variation: initial_result.rule_variation,
        to_host_sender: to_server_sender,
        host_to_client_broadcaster: to_client_sender.clone(),
    };
    rooms.insert(initial_result.compound_room_id.clone(), new_room);
    drop(rooms);
    let hand_shake_result = HandshakeResult {
        room_id: initial_result.compound_room_id,
        player_id: 0,
        rule_variation: initial_result.rule_variation,
        specific_data: Server(to_server_receiver, to_client_sender),
    };
    Some(hand_shake_result)
}

/// Informs the partner of the connection result, returns a bool as a success flag.
pub async fn inform_client_of_connection(
    sender: &mut SplitSink<WebSocket, Message>,
    status: &HandshakeResult,
) -> bool {
    let mut msg = BytesMut::with_capacity(HAND_SHAKE_RESPONSE_SIZE);
    msg.put_u8(HAND_SHAKE_RESPONSE);
    msg.put_u16(status.player_id);
    msg.put_u16(status.rule_variation);

    let result = sender.send(Message::Binary(msg.into())).await;
    result.is_ok()
}

/// Performs the shutdown of the system and sends a last message.
pub async fn shutdown_connection(
    wrapped_sender: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    disconnect_data: DisconnectData,
    app_state: Arc<AppState>,
    error_message: &'static str,
) {
    match disconnect_data.sender {
        DisconnectServer(sender) => {
            // Inform clients first.
            let mut msg = BytesMut::with_capacity(SERVER_DISCONNECT_MSG_SIZE);
            msg.put_u8(SERVER_DISCONNECTS);
            let _ = sender.send(msg.into());
            // Kill room.
            let mut rooms = app_state.rooms.lock().await;
            rooms.remove(&disconnect_data.room_id);
            drop(rooms);
        }
        DisconnectClient(sender) => {
            // Inform server first.
            let mut msg = BytesMut::with_capacity(CLIENT_DISCONNECT_MSG_SIZE);
            msg.put_u8(CLIENT_DISCONNECTS);
            msg.put_u16(disconnect_data.player_id);
            let _ = sender.send(msg.into()).await;
            // Subtract one client from the room.
            let mut rooms = app_state.rooms.lock().await;
            // Check if the room still exists.
            if let Some(room) = rooms.get_mut(&disconnect_data.room_id) {
                room.amount_of_players -= 1;
            }
            drop(rooms);
        }
    }

    let mut sender = wrapped_sender.lock().await;

    // Send the message to the WASM point.
    send_closing_message(&mut sender, error_message.into()).await;
}
