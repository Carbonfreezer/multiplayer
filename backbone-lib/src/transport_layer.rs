//! The transport layer takes care of the communication with the relay service.
//! This is the core entry point of the system.
//!
//! # Architecture Overview
//!
//! ```text
//! Frontend -> TransportLayer -> Backend
//! ```
//!
//! The backend only exists on the client-hosted server side. All other
//! communication is channeled over the network. Commands and polling requests
//! flow from left to right.
//!
//! - **Frontend**: Macroquad-style heartbeat-driven game loop
//! - **TransportLayer**: Created and updated by the frontend each frame
//! - **Backend**: Purely event-driven, only exists on the host
//!
//! # Required Components
//!
//! To use the system, implement the following types:
//!
//! * **ServerRpcPayload**: Commands sent from frontend to game logic (e.g., `MakeMove`, `PlayCard`).
//!   Pre-filter on the client side to minimize network traffic.
//! * **DeltaInformation**: Incremental changes for the frontend (e.g., `CardDrawn`, `PieceMoved`).
//!   Can be polled step-by-step for animation transitions. Complex transitions can be split
//!   into multiple deltas — they get coalesced during network transmission.
//! * **ViewState**: Complete snapshot for the frontend containing all visualization data.
//!   Used when a new client joins. On receipt, set the visualization state immediately
//!   without animation.
//! * **BackendArchitecture**: The game logic module, only present on the server side.
//!
//! # Frontend Integration
//!
//! Before entering the game loop, create the transport layer. At the beginning of each frame,
//! call `update()`.
//!
//! - While **Disconnected**: Show room creation/joining UI and display any error string
//! - While **Connected**: Execute game logic, send RPCs via `register_server_rpc()`,
//!   and poll state updates via `get_next_update()`
//!
//! # Example Usage
//!
//! ```text
//! let mut net_architecture: TransportLayer<RpcPayload, DeltaInformation, Backend, ViewState> =
//!     TransportLayer::generate_transport_layer(
//!         "ws://127.0.0.1:8080/ws".to_string(),
//!         "my_fat_game".to_string(),
//!     );
//!
//! loop {
//!     let delta_time = get_frame_time();
//!     net_architecture.update(delta_time);
//!
//!     let state = net_architecture.connection_state().clone();
//!     match state {
//!         ConnectionState::Disconnected { error_string } => {
//!             // Process startup and connection GUI here
//!             net_architecture.start_game_server(room, 0);
//!         }
//!         ConnectionState::Connected { is_server: _, player_id, rule_set } => {
//!             if let Some(update) = net_architecture.get_next_update() {
//!                 match update {
//!                     ViewStateUpdate::Full(state) => {
//!                         // Hard-set view state (no animation)
//!                     }
//!                     ViewStateUpdate::Incremental(delta) => {
//!                         // Process with animation
//!                     }
//!                 }
//!             }
//!             // Send player actions to the server
//!             net_architecture.register_server_rpc(command);
//!         }
//!         _ => {}
//!     }
//!
//!     next_frame().await
//! }
//! ```

use crate::timer::Timer;
use crate::traits::BackendCommand::{CancelTimer, KickPlayer, SetTimer, TerminateRoom};
use crate::traits::{BackEndArchitecture, BackendCommand, SerializationCap};
use crate::web_socket_interface::{ConnectionInformation, ToServerCommands};
use std::collections::VecDeque;

/// State updates delivered to the frontend for rendering.
///
/// The frontend should handle these differently:
/// - [`Full`](Self::Full): Immediately set all visual state (no animation)
/// - [`Incremental`](Self::Incremental): Apply with animation/transition effects
pub enum ViewStateUpdate<ViewState, DeltaInformation> {
    /// Complete game state snapshot.
    ///
    /// Received when:
    /// - Initially connecting to a game
    /// - After a game reset
    /// - On server startup (for the host)
    ///
    /// The frontend should immediately synchronize all visuals to match
    /// this state without animations.
    Full(ViewState),

    /// Incremental state change for animated transitions.
    ///
    /// These arrive in order and can be polled one at a time to pace
    /// animations across multiple frames. Examples:
    /// - A piece moving from A to B
    /// - A card being revealed
    /// - A score incrementing
    Incremental(DeltaInformation),
}

/// Server-only state container.
///
/// This struct exists only on the host client and manages the game backend,
/// timers, and remote player tracking. It is created when `start_game_server()`
/// succeeds and destroyed on disconnect or room termination.
struct ServerContext<BackendArchitecture> {
    /// The backend that runs the game logic.
    back_end: BackendArchitecture,
    /// The timer to generate timing events for the backend.
    timer: Timer,
    /// The amount of players, that are currently subscribed (not including the local player).
    amount_of_remote_players: u16,
}

/// Connection lifecycle states.
///
/// The transport layer progresses through these states:
///
/// ```text
/// Disconnected -> AwaitingHandshake -> ExecutingHandshake -> Connected
///      ^                                                         |
///      |___________________ (on error or disconnect) ____________|
/// ```
///
/// Frontend code typically only needs to distinguish between `Disconnected`
/// (show lobby UI) and `Connected` (run game loop).
#[derive(Clone, PartialEq, Debug)]
pub enum ConnectionState {
    /// Not connected to any game room.
    ///
    /// The `error_string` contains the reason for disconnection if this state
    /// was reached due to an error (connection lost, kicked, room terminated).
    /// It's `None` on initial startup.
    Disconnected { error_string: Option<String> },

    /// WebSocket connection initiated, waiting for transport readiness.
    ///
    /// This intermediate state exists primarily for WASM clients where
    /// WebSocket connection is asynchronous. Frontend can show a
    /// "Connecting..." indicator.
    AwaitingHandshake,

    /// Transport ready, waiting for server response with player ID and rules.
    ///
    /// The handshake message has been sent; waiting for confirmation.
    /// Frontend can continue showing "Connecting..." indicator.
    ExecutingHandshake,

    /// Successfully connected and ready for gameplay.
    ///
    /// At this point:
    /// - `player_id` is assigned (always `0` for the host)
    /// - `rule_set` contains the game variant configuration
    /// - State updates can be polled via `get_next_update()`
    /// - RPCs can be sent via `register_server_rpc()`
    Connected {
        /// `true` if this client is hosting the game (runs the backend).
        is_server: bool,
        /// Unique player identifier for this session. Host is always `0`.
        player_id: u16,
        /// Game variant/mode as configured by the host.
        rule_set: u16,
    },
}

/// The central coordinator between frontend, backend, and network transport.
///
/// `TransportLayer` abstracts away the differences between hosting and joining a game.
/// The frontend interacts with it identically regardless of role:
///
/// ```text
/// ┌─────────────────────────────────────────────────────────────────────┐
/// │                         Host Client                                 │
/// │  ┌──────────┐     ┌─────────────────┐     ┌──────────────────────┐  │
/// │  │ Frontend │ ───►│ Transport Layer │ ───►│ Backend (game logic) │  │
/// │  └──────────┘     └──────┬──────────┘     └──────────────────────┘  │
/// └──────────────────────────┼──────────────────────────────────────────┘
///                            │ WebSocket
///                            ▼
///                     ┌─────────────┐
///                     │Relay Server │
///                     └──────┬──────┘
///                            │
/// ┌──────────────────────────┼──────────────────────────────────────────┐
/// │                          ▼                                          │
/// │  ┌──────────┐     ┌─────────────────┐                               │
/// │  │ Frontend │ ───►│ Transport Layer │  (no backend on clients)      │
/// │  └──────────┘     └─────────────────┘                               │
/// │                      Remote Client                                  │
/// └─────────────────────────────────────────────────────────────────────┘
/// ```
///
/// # Type Parameters
///
/// * `ServerRpcPayload` — Game actions sent by players (e.g., `PlacePiece { x, y }`)
/// * `DeltaInformation` — Incremental state changes for animations
/// * `Backend` — The [`BackEndArchitecture`] implementation for this game
/// * `ViewState` — Complete game state for synchronizing new clients
///
/// # Lifecycle
///
/// 1. Create with [`generate_transport_layer()`](Self::generate_transport_layer)
/// 2. Call [`update()`](Self::update) every frame
/// 3. Check [`connection_state()`](Self::connection_state) to determine UI mode
/// 4. When disconnected: call [`start_game_server()`](Self::start_game_server) or
///    [`start_game_client()`](Self::start_game_client)
/// 5. When connected: poll [`get_next_update()`](Self::get_next_update) and send
///    actions via [`register_server_rpc()`](Self::register_server_rpc)
pub struct TransportLayer<ServerRpcPayload, DeltaInformation, Backend, ViewState>
where
    ServerRpcPayload: SerializationCap,
    Backend: BackEndArchitecture<ServerRpcPayload, DeltaInformation, ViewState>,
    DeltaInformation: SerializationCap + Clone,
    ViewState: SerializationCap + Clone,
{
    /// The things we have only on the server.
    server_context: Option<ServerContext<Backend>>,

    /// The delta information and eventual full updates we enqueue for handing to the front end.
    state_info_que: VecDeque<ViewStateUpdate<ViewState, DeltaInformation>>,

    /// The list with rpc server payloads, that get either transmitted to the backend
    /// in server mode or transmitted to the network in the next heartbeat.
    rpc_que: VecDeque<ServerRpcPayload>,

    /// The core connection.
    core_connection: Option<ConnectionInformation>,

    /// The current state we have.
    connection_state: ConnectionState,

    /// The URI we use for connection.
    connection_string: String,

    /// The name of the game.
    game_name: String,
}

impl<ServerRpcPayload, DeltaInformation, BackendArchitecture, ViewState>
    TransportLayer<ServerRpcPayload, DeltaInformation, BackendArchitecture, ViewState>
where
    ServerRpcPayload: SerializationCap,
    BackendArchitecture: BackEndArchitecture<ServerRpcPayload, DeltaInformation, ViewState>,
    DeltaInformation: SerializationCap + Clone,
    ViewState: SerializationCap + Clone,
{
    /// Creates a new transport layer instance in disconnected state.
    ///
    /// Call this once before entering the game loop. The instance starts
    /// disconnected and ready to host or join a game.
    ///
    /// # Arguments
    ///
    /// * `connection_string` — WebSocket URL of the relay server
    ///   (e.g., `"wss://board-game-hub.de/ws"`)
    /// * `game_name` — Identifier for this game type, must match the relay
    ///   server's `GameConfig.json` entry
    ///
    /// # Example
    ///
    /// ```ignore
    /// let transport_layer = TransportLayer::<MyRpc, MyDelta, MyBackend, MyState>::generate_transport_layer(
    ///     "wss://board-game-hub.de/ws".to_string(),
    ///     "reversi".to_string(),
    /// );
    /// ```
    pub fn generate_transport_layer(connection_string: String, game_name: String) -> Self {
        Self {
            server_context: None,
            state_info_que: VecDeque::new(),
            rpc_que: VecDeque::new(),
            core_connection: None,
            connection_state: ConnectionState::Disconnected { error_string: None },
            connection_string,
            game_name,
        }
    }

    /// Advances the transport layer state machine by one frame.
    ///
    /// This method must be called once per frame, typically at the beginning
    /// of the game loop. It handles:
    ///
    /// - **Disconnected**: No-op, waiting for `start_game_server/client()`
    /// - **AwaitingHandshake**: Polls WebSocket connection readiness
    /// - **ExecutingHandshake**: Waits for server response with player ID
    /// - **Connected (host)**: Processes timers, RPCs, network messages,
    ///   drains backend commands, broadcasts updates
    /// - **Connected (client)**: Sends queued RPCs, receives state updates
    ///
    /// After calling `update()`, poll state updates via `get_next_update()`
    /// and check `connection_state()` for disconnection errors.
    ///
    /// # Arguments
    ///
    /// * `delta_time` — Seconds since last frame (used for timer updates on host)
    pub fn update(&mut self, delta_time: f32) {
        match self.connection_state {
            ConnectionState::Disconnected { error_string: _ } => {} // Nothing to do here.
            ConnectionState::AwaitingHandshake => {
                self.connection_update_awaiting();
            }
            ConnectionState::ExecutingHandshake => {
                self.connection_update_handshake();
            }
            ConnectionState::Connected {
                is_server: true,
                player_id: _,
                rule_set: _,
            } => {
                self.update_server(delta_time);
            }
            ConnectionState::Connected {
                is_server: false,
                player_id: _,
                rule_set: _,
            } => {
                self.update_client();
            }
        }
    }

    /// Initiates hosting a new game room.
    ///
    /// Creates a room on the relay server and starts the local backend.
    /// The connection progresses through `AwaitingHandshake` → `ExecutingHandshake`
    /// → `Connected { is_server: true, player_id: 0, ... }`.
    ///
    /// # Arguments
    ///
    /// * `room_name` — Unique identifier for the room (shareable with other players)
    /// * `rule_variation` — Game mode/variant passed to `BackEndArchitecture::new()`
    ///
    /// # Panics
    ///
    /// Panics if called while not in `Disconnected` state.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if ui.button("Host Game").clicked() {
    ///     transport_layer.start_game_server("my-room-123".to_string(), 0);
    /// }
    /// ```
    pub fn start_game_server(&mut self, room_name: String, rule_variation: u16) {
        self.connection_initialize(room_name, rule_variation, true);
    }

    /// Initiates joining an existing game room.
    ///
    /// Connects to a room hosted by another player. The connection progresses
    /// through `AwaitingHandshake` → `ExecutingHandshake` → `Connected { is_server: false, ... }`.
    ///
    /// # Arguments
    ///
    /// * `room_name` — The room identifier (as shared by the host)
    ///
    /// # Panics
    ///
    /// Panics if called while not in `Disconnected` state.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if ui.button("Join Game").clicked() {
    ///     transport_layer.start_game_client(room_code_input.clone());
    /// }
    /// ```
    pub fn start_game_client(&mut self, room_name: String) {
        self.connection_initialize(room_name, 0, false);
    }

    /// Gracefully disconnects from the current game.
    ///
    /// Notifies the relay server (so other players see the departure),
    /// cleans up local state, and transitions to `Disconnected` state.
    /// No-op if already disconnected.
    ///
    /// Typically bound to a "Leave Room" button in the UI.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if ui.button("Leave Game").clicked() {
    ///     transport_layer.disconnect();
    /// }
    /// ```
    pub fn disconnect(&mut self) {
        if let Some(connection) = self.core_connection.as_mut()
            && let ConnectionState::Connected {
                is_server,
                player_id: _,
                rule_set: _,
            } = self.connection_state
        {
            connection.disconnect(is_server);
            self.mark_error("Disconnected from server".to_string());
            self.server_context = None;
        }
    }

    /// Queues a game action to be sent to the backend.
    ///
    /// The RPC is processed during the next `update()` call:
    /// - **Host**: Delivered directly to the local backend
    /// - **Client**: Serialized and sent over the network to the host
    ///
    /// RPCs are processed in order. Pre-validate actions on the frontend
    /// to minimize invalid requests and network traffic.
    ///
    /// # Arguments
    ///
    /// * `payload` — The game-specific action (e.g., `MakeMove { x: 3, y: 4 }`)
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(cell) = clicked_board_cell {
    ///     transport_layer.register_server_rpc(GameRpc::PlacePiece { position: cell });
    /// }
    /// ```
    pub fn register_server_rpc(&mut self, payload: ServerRpcPayload) {
        self.rpc_que.push_back(payload);
    }

    /// Retrieves the next pending state update for the frontend.
    ///
    /// Returns `None` if no updates are queued. Updates are delivered in order
    /// and should be processed one at a time to enable frame-by-frame animation.
    ///
    /// # Update Types
    ///
    /// - [`ViewStateUpdate::Full`]: Hard-set all visuals immediately (no animation)
    /// - [`ViewStateUpdate::Incremental`]: Apply with animation/transition
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Process one update per frame for smooth animations
    /// if let Some(update) = transport_layer.get_next_update() {
    ///     match update {
    ///         ViewStateUpdate::Full(state) => {
    ///             game_renderer.set_state(&state);
    ///         }
    ///         ViewStateUpdate::Incremental(delta) => {
    ///             game_renderer.animate_delta(&delta);
    ///         }
    ///     }
    /// }
    /// ```
    pub fn get_next_update(&mut self) -> Option<ViewStateUpdate<ViewState, DeltaInformation>> {
        self.state_info_que.pop_front()
    }

    /// Returns the current connection state.
    ///
    /// Check this after each `update()` call to:
    /// - Detect disconnection errors and display them to the user
    /// - Determine whether to show lobby UI or game UI
    /// - Access `player_id` and `rule_set` when connected
    ///
    /// # Example
    ///
    /// ```ignore
    /// match transport_layer.connection_state() {
    ///     ConnectionState::Disconnected { error_string } => {
    ///         if let Some(err) = error_string {
    ///             ui.label(format!("Error: {}", err));
    ///         }
    ///         show_lobby_ui();
    ///     }
    ///     ConnectionState::AwaitingHandshake | ConnectionState::ExecutingHandshake => {
    ///         ui.label("Connecting...");
    ///     }
    ///     ConnectionState::Connected { player_id, .. } => {
    ///         ui.label(format!("Playing as Player {}", player_id));
    ///         show_game_ui();
    ///     }
    /// }
    /// ```
    pub fn connection_state(&self) -> &ConnectionState {
        &self.connection_state
    }

    /// Global function to mark error and drop the connection.
    fn mark_error(&mut self, error: String) {
        self.connection_state = ConnectionState::Disconnected {
            error_string: Some(error),
        };
        self.core_connection = None; // Drops sender + receiver, closes connection
    }

    /// Helper function for connection initialization.
    fn connection_initialize(&mut self, room_name: String, rule_variation: u16, is_server: bool) {
        debug_assert!(
            self.server_context.is_none(),
            "We should have no server context at that point"
        );
        self.server_context = None;
        assert!(
            matches!(
                self.connection_state,
                ConnectionState::Disconnected { error_string: _ }
            ),
            "Only in disconnected stata is a connect allowed."
        );
        let start = ConnectionInformation::start_connecting(
            self.connection_string.clone(),
            self.game_name.clone(),
            room_name,
            rule_variation,
            is_server,
        );

        match start {
            Ok(connection) => {
                self.connection_state = ConnectionState::AwaitingHandshake;
                self.core_connection = Some(connection);
            }
            Err(e) => {
                self.mark_error(e);
            }
        }
    }

    /// We are waiting for the base connection to be established.
    fn connection_update_awaiting(&mut self) {
        debug_assert!(matches!(
            self.connection_state,
            ConnectionState::AwaitingHandshake
        ));
        let Some(connection) = self.core_connection.as_mut() else {
            debug_assert!(false, "No connection in awaiting handshake state");
            return;
        };
        let query = ConnectionInformation::update_awaiting_readiness(connection);
        match query {
            Ok(true) => {
                self.connection_state = ConnectionState::ExecutingHandshake;
            }
            Err(e) => {
                self.mark_error(e);
            }
            _ => {} // Nothing to do here.
        }
    }

    /// The update during the connection phase. We are just waiting from the response for the player id and the rule variation.
    fn connection_update_handshake(&mut self) {
        debug_assert!(matches!(
            self.connection_state,
            ConnectionState::ExecutingHandshake
        ));
        let Some(connection) = self.core_connection.as_mut() else {
            debug_assert!(false, "No connection in executing handshake state");
            return;
        };
        let query = ConnectionInformation::update_connecting(connection);
        let is_server = connection.is_server();

        match query {
            Some(Ok(result)) => {
                self.connection_state = ConnectionState::Connected {
                    is_server,
                    player_id: result.player_id,
                    rule_set: result.rule_variation,
                };
                if is_server {
                    let mut server_context: ServerContext<BackendArchitecture> = ServerContext {
                        back_end: BackEndArchitecture::new(result.rule_variation),
                        timer: Timer::new(),
                        amount_of_remote_players: 0,
                    };
                    // We also flag ourselves that we arrived.
                    server_context.back_end.player_arrival(0);
                    debug_assert_eq!(
                        result.player_id, 0,
                        "The host player should always bew player 0."
                    );
                    self.state_info_que.push_back(ViewStateUpdate::Full(
                        server_context.back_end.get_view_state().clone(),
                    ));
                    self.server_context = Some(server_context);
                }
            }
            Some(Err(e)) => {
                self.mark_error(e);
            }
            None => {} // Do nothing here.
        }
    }

    /// Updates logic for the case that we are a client hosted server.
    fn update_server(&mut self, delta_time: f32) {
        let server_context = self
            .server_context
            .as_mut()
            .expect("No server context at that point");
        let communicator = self.core_connection.as_mut().unwrap();

        // 1. Eventual timer run outs are send to the backend.
        let running_out = server_context.timer.update_and_get_list(delta_time);
        for timer_id in running_out {
            server_context.back_end.timer_triggered(timer_id);
        }

        // 2. Process rpc_que and send the data to the backend, on the server the local player is always player 0.
        while let Some(rpc) = self.rpc_que.pop_front() {
            server_context.back_end.inform_rpc(0, rpc)
        }

        // 3. Collect data from ws_socket (RPC calls) and send the data to the backend.
        let mut client_joined = false;
        let vec = communicator.server_receive_commands_for();
        match vec {
            Ok(core) => {
                for command in core {
                    match command {
                        ToServerCommands::ClientJoin(client) => {
                            client_joined = true;
                            server_context.back_end.player_arrival(client);
                            server_context.amount_of_remote_players += 1;
                        }
                        ToServerCommands::ClientLeft(client) => {
                            server_context.back_end.player_departure(client);
                            server_context.amount_of_remote_players -= 1;
                        }
                        ToServerCommands::Rpc(client, payload) => {
                            server_context.back_end.inform_rpc(client, payload)
                        }
                    }
                }
            }
            Err(e) => {
                self.mark_error(e);
                return;
            }
        }

        // 4. Collect the data from the backend.
        let status_updates = server_context.back_end.drain_commands();
        let mut new_status = Vec::with_capacity(status_updates.len());
        // 5. Process all timer and kicking commands.
        for command in status_updates {
            match command {
                TerminateRoom => {
                    communicator.disconnect(true);
                    self.mark_error("Critical player left.".to_string());
                    self.server_context = None;
                    // We are done here.
                    return;
                }
                SetTimer { timer_id, duration } => {
                    server_context.timer.start_timer(timer_id, duration);
                }
                CancelTimer { timer_id } => {
                    server_context.timer.cancel_timer(timer_id);
                }
                KickPlayer { player } => {
                    // Safeguard for the case that a single player has already left.
                    if server_context.amount_of_remote_players > 0 {
                        communicator.server_kick_player(player);
                    }
                }
                rest => new_status.push(rest), // Keep all other commands.
            }
        }
        let status_updates = new_status;

        // 6. Check if there is a reset view state included, if we so we simply broadcast the final result and can skip all the delta information.
        if status_updates
            .iter()
            .any(|x| matches!(x, BackendCommand::ResetViewState))
        {
            let view_state = (server_context.back_end.get_view_state()).clone();

            // Reset the view state.
            if server_context.amount_of_remote_players > 0 {
                communicator.server_send_reset(&view_state);
            }
            self.state_info_que
                .push_back(ViewStateUpdate::Full(view_state));
            // With the reset everyone is up to date anyway, because the queried view state is the situation right after the update.
            return;
        }

        // 7. We collect all the remaining delta information.
        let delta_collector: Vec<DeltaInformation> = status_updates
            .into_iter()
            .map(|command| match command {
                BackendCommand::Delta(delta) => {
                    self.state_info_que
                        .push_back(ViewStateUpdate::Incremental(delta.clone()));
                    delta
                }
                _ => panic!("Unknown command"),
            })
            .collect();

        // If there are no remote players, we do not need to send update information.
        if server_context.amount_of_remote_players == 0 {
            return;
        }

        // 6.  Now all is left are the status updates methods.
        if !delta_collector.is_empty() {
            communicator.server_send_delta_info(&delta_collector);
        }

        // If we have a client joined we sent a full state broadcast.
        // We do not have to send this information to the local player, as he has always been present.
        // We do the full sync right at the end, because the view state is the final state that is left by the backend.
        if client_joined {
            communicator.server_send_full_sync(server_context.back_end.get_view_state());
        }
    }

    /// The update on the client side only communicates with the socket interface.
    fn update_client(&mut self) {
        let communicator = self.core_connection.as_mut().unwrap();
        // 1. Send out data from rpc_que.
        while let Some(rpc) = self.rpc_que.pop_front() {
            communicator.client_send_rpc_from(rpc);
        }
        // 2. Collect information from the socket and fill the data que.
        let update = communicator.client_receive_update();
        match update {
            Ok(core) => self.state_info_que.extend(core),
            Err(e) => {
                self.mark_error(e);
            }
        }
    }
}
