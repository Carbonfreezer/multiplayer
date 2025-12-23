//! Does all communication related stuff with the web sockets.
//! Uses ewebsock for native builds and quad-net for WASM builds.

use protocol::{
    CLIENT_DISCONNECTS, CLIENT_DISCONNECTS_SELF, CLIENT_GETS_KICKED, CLIENT_ID_SIZE, DELTA_UPDATE,
    FULL_UPDATE, HAND_SHAKE_RESPONSE, NEW_CLIENT, RESET, SERVER_DISCONNECTS, SERVER_ERROR,
    SERVER_RPC,
};
use crate::middle_layer::ViewStateUpdate;
use crate::traits::SerializationCap;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use postcard::{from_bytes, take_from_bytes, to_stdvec};
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
use ewebsock::WsEvent::{Closed, Error, Message};
#[cfg(not(target_arch = "wasm32"))]
use ewebsock::{WsMessage, WsReceiver, WsSender};

// ============================================================================
// WASM FFI declarations
// ============================================================================

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    fn quad_ws_connect(url_ptr: *const u8, url_len: usize);
    fn quad_ws_connected() -> i32;
    fn quad_ws_send(data_ptr: *const u8, data_len: usize);
    fn quad_ws_next_message_len() -> usize;
    fn quad_ws_recv(buffer_ptr: *mut u8, buffer_len: usize) -> usize;
}

/// The join request we get from the server. This is the same info as in the rust server to remain compatibility.
#[derive(Deserialize, Serialize)]
struct JoinRequest {
    /// Which game do we want to join.
    game_id: String,
    /// Which room do we want to join.
    room_id: String,
    /// The rule variation that is applied, this gets only interpreted if a room gets constructed.
    rule_variation: u16,
    /// Do we want to create a room and act as a server?
    create_room: bool,
}

/// A local structure that gets completed by the synchronization.
pub struct GameSetting {
    pub player_id: u16,
    pub rule_variation: u16,
}

/// Contains the commands that go to the server.
pub enum ToServerCommands<ServerRpcPayload> {
    ClientJoin(u16),
    ClientLeft(u16),
    Rpc(u16, ServerRpcPayload),
}

/// This is a connection information setting that manages all receiving and sending
pub struct ConnectionInformation {
    #[cfg(not(target_arch = "wasm32"))]
    sender: WsSender,
    #[cfg(not(target_arch = "wasm32"))]
    receiver: WsReceiver,

    pending_join_request: JoinRequest,
}

impl ConnectionInformation {
    #[cfg(not(target_arch = "wasm32"))]
    fn new(sender: WsSender, receiver: WsReceiver, join_request: JoinRequest) -> Self {
        ConnectionInformation {
            sender,
            receiver,
            pending_join_request: join_request,
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn new(join_request: JoinRequest) -> Self {
        ConnectionInformation {
            pending_join_request: join_request,
        }
    }

    /// Queries from the inner state if we are a server or not.
    pub fn is_server(&self) -> bool {
        self.pending_join_request.create_room
    }

    // ===================================================================
    // NATIVE (ewebsock) implementations
    // ===================================================================

    #[cfg(not(target_arch = "wasm32"))]
    fn send_binary(&mut self, data: &[u8]) {
        self.sender.send(WsMessage::Binary(data.to_vec()));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn try_recv_binary(&mut self) -> Result<Option<Vec<u8>>, String> {
        loop {
            match self.receiver.try_recv() {
                Some(Message(WsMessage::Binary(msg))) => return Ok(Some(msg)),
                Some(Closed) => return Err("Connection closed by server".to_string()),
                Some(Error(context)) => return Err(context),
                Some(_) => continue, // Ignore other message types, keep checking
                None => return Ok(None),
            }
        }
    }

    // ===================================================================
    // WASM (quad-net) implementations
    // ===================================================================

    #[cfg(target_arch = "wasm32")]
    fn send_binary(&mut self, data: &[u8]) {
        unsafe {
            quad_ws_send(data.as_ptr(), data.len());
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn try_recv_binary(&mut self) -> Result<Option<Vec<u8>>, String> {
        unsafe {
            // First check for incoming messages.
            let len = quad_ws_next_message_len();
            if len > 0 {
                let mut buffer = vec![0u8; len];
                quad_ws_recv(buffer.as_mut_ptr(), buffer.len());
                return Ok(Some(buffer));
            }

            // No more messages, generate error.
            if quad_ws_connected() == 0 {
                return Err("Connection lost".to_string());
            }

            Ok(None)
        }
    }

    // ===================================================================
    // Shared implementation (platform-agnostic)
    // ===================================================================

    // -----------------------------------
    // All server related.
    // -----------------------------------

    /// Sends out the information to kick a player.
    pub fn server_kick_player(&mut self, player_id: u16) {
        let mut msg_builder = BytesMut::with_capacity(1 + CLIENT_ID_SIZE);
        msg_builder.put_u8(CLIENT_GETS_KICKED);
        msg_builder.put_u16(player_id);
        self.send_binary(&msg_builder);
    }

    /// Sends the sequence with the accumulated delta infos.
    pub fn server_send_delta_info<DeltaInformation: SerializationCap>(
        &mut self,
        delta_vec: &[DeltaInformation],
    ) {
        let serialized: Vec<_> = delta_vec
            .iter()
            .flat_map(|d| to_stdvec(d).expect("Could not serialize delta information."))
            .collect();
        let mut msg_builder = BytesMut::with_capacity(1 + serialized.len());
        msg_builder.put_u8(DELTA_UPDATE);
        msg_builder.put_slice(&serialized);
        self.send_binary(&msg_builder);
    }

    /// Sends a full synchronization command.
    pub fn server_send_full_sync<ViewState: SerializationCap>(&mut self, state: &ViewState) {
        let serialized = to_stdvec(state).expect("Could not serialize state");
        let mut msg_builder = BytesMut::with_capacity(1 + serialized.len());
        msg_builder.put_u8(FULL_UPDATE);
        msg_builder.put_slice(&serialized);
        self.send_binary(&msg_builder);
    }

    /// Same as full_sync only that it gets interpreted by all clients.
    pub fn server_send_reset<ViewState: SerializationCap>(&mut self, state: &ViewState) {
        let serialized = to_stdvec(state).expect("Could not serialize state");
        let mut msg_builder = BytesMut::with_capacity(1 + serialized.len());
        msg_builder.put_u8(RESET);
        msg_builder.put_slice(&serialized);
        self.send_binary(&msg_builder);
    }

    /// Reads in all the commands that come from the diverse clients to the server.
    pub fn server_receive_commands_for<ServerRpcPayload: SerializationCap>(
        &mut self,
    ) -> Result<Vec<ToServerCommands<ServerRpcPayload>>, String> {
        let mut result: Vec<ToServerCommands<ServerRpcPayload>> = Vec::new();

        while let Some(data) = self.try_recv_binary()? {
            let mut bytes = Bytes::from(data);
            let msg = bytes.get_u8();

            match msg {
                SERVER_ERROR => {
                    let error_text = String::from_utf8_lossy(&bytes).to_string();
                    return Err(error_text);
                }
                NEW_CLIENT => {
                    let client_id = bytes.get_u16();
                    result.push(ToServerCommands::ClientJoin(client_id));
                }
                CLIENT_DISCONNECTS => {
                    let client_id = bytes.get_u16();
                    result.push(ToServerCommands::ClientLeft(client_id));
                }
                SERVER_RPC => {
                    let client_id = bytes.get_u16();
                    let payload: ServerRpcPayload = from_bytes(bytes.chunk())
                        .expect("Failed to deserialize server rpc payload");
                    result.push(ToServerCommands::Rpc(client_id, payload));
                }
                _ => return Err(format!("Unknown message received: {:?}", msg)),
            }
        }
        Ok(result)
    }

    // -----------------------------------
    // All client related.
    // -----------------------------------

    /// Sends an rpc server over the next.
    pub fn client_send_rpc_from<ServerRpcPayload: SerializationCap>(
        &mut self,
        server_payload: ServerRpcPayload,
    ) {
        let raw_bytes = to_stdvec(&server_payload).expect("Failed to serialize server rpc payload");
        let mut msg_builder = BytesMut::with_capacity(1 + raw_bytes.len());
        msg_builder.put_u8(SERVER_RPC);
        msg_builder.put_slice(&raw_bytes);
        self.send_binary(&msg_builder);
    }

    /// Gets all the updates that were sent from the server to the client side.
    pub fn client_receive_update<
        ViewState: SerializationCap,
        DeltaInformation: SerializationCap,
    >(
        &mut self,
    ) -> Result<Vec<ViewStateUpdate<ViewState, DeltaInformation>>, String> {
        let mut result: Vec<ViewStateUpdate<ViewState, DeltaInformation>> = Vec::new();

        while let Some(data) = self.try_recv_binary()? {
            let mut bytes = Bytes::from(data);
            let msg = bytes.get_u8();

            match msg {
                SERVER_ERROR => {
                    let error_text = String::from_utf8_lossy(&bytes).to_string();
                    return Err(error_text);
                }
                DELTA_UPDATE => {
                    let mut remaining: &[u8] = &bytes;
                    while !remaining.is_empty() {
                        let (delta, rest): (DeltaInformation, &[u8]) =
                            take_from_bytes(remaining).expect("Failed to decode delta payload");
                        remaining = rest;

                        result.push(ViewStateUpdate::Incremental(delta));
                    }
                }
                FULL_UPDATE | RESET => {
                    let message: ViewState =
                        from_bytes(&bytes).expect("Failed to decode full payload");
                    result.push(ViewStateUpdate::Full(message));
                }
                _ => return Err(format!("Unknown message received: {:?}", msg)),
            }
        }
        Ok(result)
    }

    // -----------------------------------
    // All connection logic related.
    // -----------------------------------

    /// Sends the disconnect message
    pub fn disconnect(&mut self, as_server: bool) {
        let msg = if as_server {
            vec![SERVER_DISCONNECTS]
        } else {
            vec![CLIENT_DISCONNECTS_SELF]
        };
        self.send_binary(&msg);
    }

    /// Initiates the connection phase (native version).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_connecting(
        base_url: String,
        game_id: String,
        room_id: String,
        rule_variation: u16,
        is_server: bool,
    ) -> Result<ConnectionInformation, String> {
        let options = ewebsock::Options::default();
        let (sender, receiver) = ewebsock::connect(&base_url, options)
            .map_err(|_| "Could not reach websocket api".to_string())?;

        let req = JoinRequest {
            game_id,
            room_id,
            rule_variation,
            create_room: is_server,
        };

        Ok(ConnectionInformation::new(sender, receiver, req))
    }

    /// Initiates the connection phase (WASM version).
    #[cfg(target_arch = "wasm32")]
    pub fn start_connecting(
        base_url: String,
        game_id: String,
        room_id: String,
        rule_variation: u16,
        is_server: bool,
    ) -> Result<ConnectionInformation, String> {
        unsafe {
            quad_ws_connect(base_url.as_ptr(), base_url.len());
        }

        let req = JoinRequest {
            game_id,
            room_id,
            rule_variation,
            create_room: is_server,
        };

        Ok(ConnectionInformation::new(req))
    }

    /// Here we update the awaiting readiness state.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn update_awaiting_readiness(
        connection: &mut ConnectionInformation,
    ) -> Result<bool, String> {
        let msg = to_stdvec(&connection.pending_join_request)
            .map_err(|_| "Problem in serialization".to_string())?;
        connection.sender.send(WsMessage::Binary(msg));
        Ok(true)
    }

    /// Here we update the awaiting readiness state. WASM version.
    #[cfg(target_arch = "wasm32")]
    pub fn update_awaiting_readiness(
        connection: &mut ConnectionInformation,
    ) -> Result<bool, String> {
        unsafe {
            if quad_ws_connected() == 0 {
                return Ok(false);
            }
            let msg = to_stdvec(&connection.pending_join_request)
                .map_err(|_| "Problem in serialization".to_string())?;
            quad_ws_send(msg.as_ptr(), msg.len());
        }
        Ok(true)
    }

    /// Updates the connection in the state machine.
    pub fn update_connecting(
        connection_info: &mut ConnectionInformation,
    ) -> Option<Result<GameSetting, String>> {
        let data = match connection_info.try_recv_binary() {
            Ok(Some(data)) => data,
            Ok(None) => return None,
            Err(e) => return Some(Err(e)),
        };

        let mut bytes = Bytes::from(data);
        let msg = bytes.get_u8();

        match msg {
            SERVER_ERROR => {
                let error_text = String::from_utf8_lossy(&bytes).to_string();
                Some(Err(error_text))
            }
            HAND_SHAKE_RESPONSE => {
                let player_id = bytes.get_u16();
                let rule_variation = bytes.get_u16();

                Some(Ok(GameSetting {
                    player_id,
                    rule_variation,
                }))
            }
            _ => Some(Err(format!(
                "Unknown message received in handshake: {:?}",
                msg
            ))),
        }
    }
}
