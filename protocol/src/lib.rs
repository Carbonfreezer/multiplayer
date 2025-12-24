//! The ids for messages that we use. They will be used consistent across the server and the client.
//! Also contains the protocol structure for joining a game.

use serde::{Deserialize, Serialize};

/// The buffer sizes for the channels for intra VPS communication.
pub const CHANNEL_BUFFER_SIZE: usize = 256;

// Client -> Server.

/// The message to announce a new client (Client->Server) followed by u16 client id.
pub const NEW_CLIENT: u8 = 0;
/// The message size for a new client (Header + Client Id) (u8 + u16)
pub const NEW_CLIENT_MSG_SIZE: usize = 3;

/// A client disconnects from the game. (Client->Server) and removes him from the room. followed by u16 client id.
pub const CLIENT_DISCONNECTS: u8 = 1;
/// The disconnect client message size (Header + Client Id) (u8 + u16)
pub const CLIENT_DISCONNECT_MSG_SIZE: usize = 3;

/// Client -> Server RPC followed by u16 Clientid, followed by payload from postcard or other coding.  (Client->Server)
pub const SERVER_RPC: u8 = 2;

/// The disconnection message that is used for disconnecting without any arguments, that gets passed through the web socket layer.
pub const CLIENT_DISCONNECTS_SELF : u8 = 3;

// Server -> Client

/// The server disconnects from the game and the room gets closed.
pub const SERVER_DISCONNECTS: u8 = 0;
/// The disconnection message is just the byte itself.
pub const SERVER_DISCONNECT_MSG_SIZE: usize = 1;

/// A client gets kicked, meant for the situation, when no more clients should get accepted. followed by u16 client id. The receiving tokio task has to act on its own. (Server -> Client)
pub const CLIENT_GETS_KICKED: u8 = 1;

/// Delta update. Followed by u16 amount of delta updates, followed by payload for every delta update.
pub const DELTA_UPDATE: u8 = 2;

/// Flagging a full update. Followed by payload for full update.
pub const FULL_UPDATE: u8 = 3;

/// The message to reset the game. This sets the internal status to not snyced.
pub const RESET: u8 = 4;

/// The error message we add.
pub const SERVER_ERROR: u8 = 5;

/// The response message for the handshake.
pub const HAND_SHAKE_RESPONSE: u8 = 6;

// Sizes of entries.
/// For the handshake we respond with player id and rule variation. (u16 + u16)
pub const HAND_SHAKE_RESPONSE_SIZE: usize = 5;

/// The size of a new client. (u16)
pub const CLIENT_ID_SIZE: usize = 2;

/// The join request. This struct is used on the server and on the client.
#[derive(Deserialize, Serialize)]
pub struct JoinRequest {
    /// Which game do we want to join.
    pub game_id: String,
    /// Which room do we want to join.
    pub room_id: String,
    /// The rule variation that is applied, this gets only interpreted if a room gets constructed.
    pub rule_variation: u16,
    /// Do we want to create a room and act as a server?
    pub create_room: bool,
}