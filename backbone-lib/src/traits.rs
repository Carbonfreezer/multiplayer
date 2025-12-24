//! Core abstractions for the client-hosted game architecture.
//!
//! This module defines the contract between game-specific logic and the
//! networking middleware. Games implement [`BackEndArchitecture`] to handle
//! player events and produce state updates, while the middle layer handles
//! serialization and network transport.
//!
//! # Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        Host Client                          │
//! │  ┌───────────────┐    ┌───────────────┐    ┌─────────────┐  │
//! │  │   Frontend    │───►│  MiddleLayer  │───►│   Backend   │  │
//! │  │  (Rendering)  │    │  (Transport)  │    │ (Game Logic)│  │
//! │  └───────────────┘    └───────────────┘    └─────────────┘  │
//! └─────────────────────────────────────────────────────────────┘
//!                               ▲
//!                               │ WebSocket (via Relay Server)
//!                               ▼
//!                    ┌─────────────────────┐
//!                    │   Remote Clients    │
//!                    │ (Frontend + Middle) │
//!                    └─────────────────────┘
//! ```
//!
//! # Data Flow
//!
//! - **Inbound**: Player RPCs arrive via [`BackEndArchitecture::inform_rpc`]
//! - **Outbound**: Game produces [`BackendCommand`]s (deltas, kicks, timers)
//! - **Sync**: New clients receive [`BackEndArchitecture::get_view_state`] for full state
//!
//! # Implementing a Game
//!
//! ```ignore
//! impl BackEndArchitecture<MyRpc, MyDelta, MyViewState> for MyGame {
//!     fn new(rule_variation: u16) -> Self { /* ... */ }
//!     fn player_arrival(&mut self, player: u16) { /* ... */ }
//!     fn inform_rpc(&mut self, player: u16, payload: MyRpc) { /* ... */ }
//!     // ...
//! }
//! ```

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Marker trait for types that can be serialized with postcard.
///
/// This combines [`Serialize`] and [`DeserializeOwned`] into a single bound,
/// reducing boilerplate in generic signatures. All types implementing both
/// traits automatically implement `SerializationCap`.
///
/// # Example
///
/// ```ignore
/// // Instead of writing:
/// fn send<T: Serialize + DeserializeOwned>(msg: T) { /* ... */ }
///
/// // You can write:
/// fn send<T: SerializationCap>(msg: T) { /* ... */ }
/// ```
pub trait SerializationCap: Serialize + DeserializeOwned {}
impl<T> SerializationCap for T where T: Serialize + DeserializeOwned {}

/// Commands emitted by the game backend to control the session.
///
/// The middle layer polls these via [`BackEndArchitecture::drain_commands`]
/// and translates them into network messages or local actions.
///
/// # Command Types
///
/// | Command | Network Effect | Use Case |
/// |---------|----------------|----------|
/// | [`Delta`](Self::Delta) | Broadcast to all clients | Incremental state change |
/// | [`ResetViewState`](Self::ResetViewState) | Broadcast + clear client state | New game/round |
/// | [`KickPlayer`](Self::KickPlayer) | Targeted disconnect | Rule enforcement |
/// | [`SetTimer`](Self::SetTimer) | None (local only) | Turn limits, animations |
/// | [`CancelTimer`](Self::CancelTimer) | None (local only) | Player acted in time |
/// | [`TerminateRoom`](Self::TerminateRoom) | Disconnect everyone | Important player left, fatal error |
pub enum BackendCommand<DeltaInformation>
where
    DeltaInformation: SerializationCap,
{
    /// Incremental state change to be applied by all frontends.
    ///
    /// Deltas should be minimal and designed to trigger animations or
    /// visual feedback. The frontend applies them on top of its current
    /// view state.
    ///
    /// # Example Deltas
    /// - Piece moved from A to B
    /// - Card revealed
    /// - Score updated
    Delta(DeltaInformation),

    /// Signals a complete reset of the game state.
    ///
    /// Clients discard their current view state and request a fresh
    /// [`BackEndArchitecture::get_view_state`]. Typically used when
    /// starting a new game or round.
    ResetViewState,

    /// Forcibly removes a player from the session.
    ///
    /// The relay server will close the player's WebSocket connection.
    /// Common reasons:
    /// - Player joined during an invalid game phase
    /// - Rule violation detected
    /// - Room is full (based on `rule_variation`)
    KickPlayer {
        /// The player ID to disconnect.
        player: u16,
    },

    /// Schedules a callback after the specified duration.
    ///
    /// When the timer fires, [`BackEndArchitecture::timer_triggered`] is
    /// called with the corresponding `timer_id`. Setting a timer with an
    /// existing ID overwrites the previous timer.
    ///
    /// # Use Cases
    /// - Turn time limits
    /// - Delayed animations
    /// - AI thinking time simulation
    SetTimer {
        /// Unique identifier for this timer (allows cancellation).
        timer_id: u16,
        /// Duration in seconds until the timer fires.
        duration: f32,
    },

    /// Cancels a previously scheduled timer.
    ///
    /// No-op if the timer already fired or was never set.
    CancelTimer {
        /// The timer ID to cancel.
        timer_id: u16,
    },

    /// Shuts down the entire room and disconnects all players.
    ///
    /// This is a terminal state — no further commands are processed.
    /// Triggered when:
    /// - The host player disconnects
    /// - An unrecoverable error occurs
    /// - The game ends and the room should close
    TerminateRoom,
}

/// The core trait for implementing game-specific server logic.
///
/// A game backend is a purely event-driven state machine. It receives events
/// (player joins, RPCs, timer callbacks) and produces commands that the middle
/// layer translates into network messages.
///
/// # Type Parameters
///
/// * `ServerRpcPayload` — The game-specific action type sent by players
///   (e.g., `PlacePiece { x: u8, y: u8 }`)
/// * `DeltaInformation` — Incremental state changes for animations
///   (e.g., `PieceMoved { from: Pos, to: Pos }`)
/// * `ViewState` — Complete game state for syncing new clients
///   (e.g., board positions, scores, current player)
///
/// # Lifecycle
///
/// ```text
/// new(rule_variation)
///       │
///       ▼
/// ┌─────────────────────────────────────────────────┐
/// │              Event Loop                         │
/// │  ┌──────────────┐    ┌──────────────────────┐   │
/// │  │player_arrival│    │    drain_commands    │──►│──► Network
/// │  │player_depart.│    │    get_view_state    │   │
/// │  │inform_rpc    │───►│                      │   │
/// │  │timer_trigger │    └──────────────────────┘   │
/// │  └──────────────┘                               │
/// └─────────────────────────────────────────────────┘
///       │
///       ▼ (TerminateRoom)
///     [End]
/// ```
///
/// # Implementation Notes
///
/// - Update `ViewState` alongside every `Delta` to maintain consistency
/// - Use `rule_variation` to configure game modes (e.g., coop vs. competitive)
pub trait BackEndArchitecture<ServerRpcPayload, DeltaInformation, ViewState>
where
    ServerRpcPayload: SerializationCap,
    DeltaInformation: SerializationCap,
    ViewState: SerializationCap + Clone,
{
    /// Creates a new game instance with the specified rule configuration.
    ///
    /// The `rule_variation` parameter allows the same backend to support
    /// multiple game modes without separate implementations.
    ///
    /// # Examples
    /// - `0` = Standard rules
    /// - `1` = Cooperative mode
    /// - `2` = Timed mode
    fn new(rule_variation: u16) -> Self;

    /// Called when a new player connects to the room.
    ///
    /// The backend should:
    /// - Add the player to its internal tracking
    /// - Optionally emit a [`BackendCommand::Delta`] announcing the join
    /// - Optionally emit [`BackendCommand::KickPlayer`] if joining is not allowed
    ///
    /// Note: The player will receive a full **ViewState** automatically after
    /// this method returns.
    fn player_arrival(&mut self, player: u16);

    /// Called when a player disconnects (intentionally or due to connection loss).
    ///
    /// The backend should:
    /// - Remove the player from internal tracking
    /// - Handle game-over conditions if a critical player left
    /// - Optionally emit [`BackendCommand::TerminateRoom`] if the game cannot continue
    fn player_departure(&mut self, player: u16);

    /// Called when a player sends a game action.
    ///
    /// This is the main entry point for game logic. The backend should:
    /// - Validate the action (ignore or kick if invalid)
    /// - Update internal state
    /// - Emit appropriate [`BackendCommand::Delta`] messages
    /// - Update the **ViewState** to match
    ///
    /// # Arguments
    /// * `player` — The player ID who sent this action
    /// * `payload` — The deserialized game-specific action
    fn inform_rpc(&mut self, player: u16, payload: ServerRpcPayload);

    /// Called when a previously scheduled timer fires.
    ///
    /// Common responses:
    /// - Force a default action for a timed-out player
    /// - Transition to the next game phase
    /// - Emit a delta for animation completion
    fn timer_triggered(&mut self, timer_id: u16);

    /// Returns the complete current game state for client synchronization.
    ///
    /// This is called when:
    /// - A new player joins and needs the full state
    /// - A complete reset has been requested by the backend
    ///
    /// The returned state must reflect all deltas that have been emitted.
    fn get_view_state(&self) -> &ViewState;

    /// Collects and clears all pending commands since the last drain.
    ///
    /// The middle layer calls this periodically (typically every frame on the
    /// host client) to process outbound messages.
    ///
    /// # Implementation
    ///
    /// Use `std::mem::take` to efficiently drain the command queue:
    ///
    /// ```ignore
    /// fn drain_commands(&mut self) -> Vec<BackendCommand<DeltaInformation>> {
    ///     std::mem::take(&mut self.command_list)
    /// }
    /// ```
    fn drain_commands(&mut self) -> Vec<BackendCommand<DeltaInformation>>;
}
