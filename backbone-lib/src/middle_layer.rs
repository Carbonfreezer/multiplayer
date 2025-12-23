//! The middle layer takes care of the communication with the relay service.
//! This is te core entry point of the system.
//!
//! The overall architecture is like this:
//! Frontend->MiddleLayer->Backend
//! The backend only exists on the client hosted server side. Otherwise all other communications are channeled
//! over the network. From the control flow it is commands and polling requests are sent from left to right.
//! The frontend works macroquad game like heartbeat driven. The backend is purely event driven. The middle layer has
//! to be created and updated from the frontend.
//!
//! To use the system, the following components have to be implemented:
//! * ServerRpcPayload: This entity (probably an enum) contains the payload send by the front end to the game logic.
//!   Typically, this boils down to user interaction commands, like making a move. To limit network traffic a smart preselection should already been done on the client side.
//! * DeltaInformation: This is the corresponding downstream part. This contains change information for the front end, like drawing a card or  making a move (probably also en enum). This can be polled step by step
//!   from the front end also for animation transitions. Do not be afraid to spilt complex transitions into several DeltaInformations. They get coalesced in network transmission.
//! * ViewState: This is snapshot for the frontend. It contains all the information for the frontend to demonstrate the game as is. This information gets used, when a new client joins.
//!   When the frontend polls such a message, it should not animate to something but simply set the current visualization state. The ViewState is administrated by the backend, it usually contains all relevant information.
//! * BackendArchitecture: This is the module that contains the real board game logic and only sits on the server side.
//! * Frontend: Before entering the game loop the middle layer should be created. At the beginning of the core loop an update should be invoked.
//!
//! As long as the Middlelayer is disconnected, a UI for creating or joining a room should be shown. Also the error string should be displayed.
//! As soon as the middlelayer is connected, the real game gets executed. All game logic relevant user interactions are sent to the middle layer via register_server_rpc. Updating the
//! visualization is done by polling game state updates via get_next_update. A full update means a hard set of the view state. This happens on server start or  client connect or after a reset command
//! or if an explicit hard setting is required by the game logic.
//! Simply hard set your frontend to the indicated information. The incremental update flags a local small update, that may eventually be executed by an animation (like moving a figure or dealing a card.).
//!
//! A rough usage example looks like this:
//! ```text
//!
//!    let mut net_architecture: MiddleLayer<MoveCommand, MoveCommand, TicTacToeLogic, GameBoard> =
//!         MiddleLayer::generate_middle_layer(
//!             "ws://127.0.0.1:8080/ws".to_string(),
//!             "my_fat_game".to_string(),
//!        );
//!       loop {
//!         let delta_time = get_frame_time();
//!         net_architecture.update(delta_time);
//!
//!         let state = net_architecture.connection_state().clone();
//!         match state {
//!             ConnectionState::Disconnected { error_string } => {
//!                     // Process startup and connectin GUI here. and start server or client eventually.
//!                      net_architecture
//!                         .start_game_server(room, 0),
//!             }
//!             ConnectionState::Connected {
//!                 is_server: _,
//!                 player_id,
//!                 rule_set,
//!             } => {
//!                 if let Some(update) = middle_layer.get_next_update() {
//!                     match update {
//!                         ViewStateUpdate::Full(state) => {
//!                             // Process hard setting of view state
//!                         }
//!                         ViewStateUpdate::Incremental(delta) => {
//!                             // Process any incremental information, resulting in animation.
//!                         }
//!                     }
//!                 }
//!                 // In the logic we eventually create commands to be sent to the server.
//!                 middle_layer.register_server_rpc(command);
//!
//!             }
//!             _ => {}
//!         }
//!
//!         next_frame().await
//!     }
//!
//! ```

use crate::timer::Timer;
use crate::traits::BackendCommand::{CancelTimer, KickPlayer, SetTimer, TerminateRoom};
use crate::traits::{BackEndArchitecture, BackendCommand, SerializationCap};
use crate::web_socket_interface::{ConnectionInformation, ToServerCommands};
use std::collections::VecDeque;

/// The game state updates we get. We always get a full sync after connection or during a game reset.
pub enum ViewStateUpdate<ViewState, DeltaInformation> {
    /// The complete front end side representation of the game gets set. Happens on connect and after a reset. Is also
    /// invoked on the server side on start up and when a reset got requested.
    Full(ViewState),
    /// Incremental information is transmitted for eventual animation.
    Incremental(DeltaInformation),
}

/// All information that only exists on the server side.
struct ServerContext<BackendArchitecture> {
    /// The backend that runs the game logic.
    back_end: BackendArchitecture,
    /// The timer to generate timing events for the backend.
    timer: Timer,
    /// The amount of players, that are currently subscribed (not including the local player).
    amount_of_remote_players: u16,
}

/// The different phases we may be in concerning the connection.
#[derive(Clone, PartialEq, Debug)]
pub enum ConnectionState {
    /// When we are disconnected we may have an error string, that tells the reason why we went to disconnection.
    Disconnected { error_string: Option<String> },
    /// We are awaiting a handshake, introduced because of WASM client connecting. Probably not interesting in game logic.
    AwaitingHandshake,
    /// We are awaiting a server response for client id, and rule set. Probably not interesting in game logic.
    ExecutingHandshake,
    /// We are connected now, we know, if we are a server, the player_id and the rule set. When we are and only when we are the server the
    /// player_id is always 0.
    Connected {
        is_server: bool,
        player_id: u16,
        rule_set: u16,
    },
}

/// The core entry point to the networking architecture.
pub struct MiddleLayer<ServerRpcPayload, DeltaInformation, Backend, ViewState>
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
    MiddleLayer<ServerRpcPayload, DeltaInformation, BackendArchitecture, ViewState>
where
    ServerRpcPayload: SerializationCap,
    BackendArchitecture: BackEndArchitecture<ServerRpcPayload, DeltaInformation, ViewState>,
    DeltaInformation: SerializationCap + Clone,
    ViewState: SerializationCap + Clone,
{
    /// Creates the middle layer needs the connection string (which is server specific) and
    /// the name of the game, which is game specific. Should be done before entering the game loop.
    pub fn generate_middle_layer(connection_string: String, game_name: String) -> Self {
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

    /// The update should be called once a frame from the main program. Typically that should be done at the beginning of the frame.
    /// Afterwards the state information can be polled, frontend logic and rendering done.
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

    /// Starts the game with an indicated game, room and rule variation as a server.
    /// Should only be done in disconnected state.
    pub fn start_game_server(&mut self, room_name: String, rule_variation: u16) {
        self.connection_initialize(room_name, rule_variation, true);
    }

    /// Starts the game as a client with a room name.
    /// Should only be done in disconnected state.
    pub fn start_game_client(&mut self, room_name: String) {
        self.connection_initialize(room_name, 0, false);
    }

    ///  Asks explicitly for a disconnection. Should be placed on a leave room button.
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

    /// Registers an rpc command that goes to the backend.
    pub fn register_server_rpc(&mut self, payload: ServerRpcPayload) {
        self.rpc_que.push_back(payload);
    }

    /// Gets the next update information if existent to be processed by the frontend.They can be polled once at a time
    /// to process animation information over several heartbeats.
    pub fn get_next_update(&mut self) -> Option<ViewStateUpdate<ViewState, DeltaInformation>> {
        self.state_info_que.pop_front()
    }
    /// Probes the current connection state. Especially interesting for dripping back to disconnected state for error handling.
    /// Should be called once a frame on the client logic side after the heartbeat of the middle layer.
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
        // We do the full sync right at the end, because the front end state is the final state that is left by the backend.
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
