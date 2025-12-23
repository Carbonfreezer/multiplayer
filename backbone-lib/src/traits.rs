//! This module contains the relevant traits for the system.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Helper structure to summarize serialization and deserialization capabilities.
pub trait SerializationCap: Serialize + DeserializeOwned {}
impl<T> SerializationCap for T where T: Serialize + DeserializeOwned {}

/// The commands that may get from the backend to the middle layer.
/// All communication with the frontend is done with these commands.
/// The DeltaInformation is an incremental change for the front end state.
pub enum BackendCommand<DeltaInformation>
where
    DeltaInformation: SerializationCap,
{
    /// This is the command to change something about the view state. This is an incremental change, which can be used by front end to trigger an animation
    /// or some other action. The change is encoded in the DeltaInformation.
    Delta(DeltaInformation),
    /// This is a total reset of the view state. Typically done, if a new game should get started.
    ResetViewState,
    /// A player should get removed / rejected. Can be done based on the game phase (eg. after setup, or based on rule variation.)
    KickPlayer { player: u16 },
    /// Activates a timer, that triggers a callback. Introduced to keep the Backend purely event driven. Existing timers get overwritten.
    SetTimer { timer_id: u16, duration: f32 },
    /// Cancels an already set timer.
    CancelTimer { timer_id: u16 },
    /// This is typically called when a critical player has left the game. The server gets shut down and everyone is back to state disconnected.
    TerminateRoom,
}

/// The backend architecture, that is used in client hosted server mode.
/// The backend works purely event driven. The events being invoked are
/// player_arrival, player_departure, inform_rpc and timer_triggered. The implementation
/// generates its response by keeping and updating an internal vector of BackEnd commands,
/// that gets drained during certain intervals from the middle layer.
pub trait BackEndArchitecture<ServerRpcPayload, DeltaInformation, ViewState>
where
    ServerRpcPayload: SerializationCap,
    DeltaInformation: SerializationCap,
    ViewState: SerializationCap + Clone,
{
    /// Creates a new backend, with the indicated rule variation.
    fn new(rule_variation: u16) -> Self;
    /// A new player just registered.
    fn player_arrival(&mut self, player: u16);
    /// A player has left the game.
    fn player_departure(&mut self, player: u16);
    /// This is a rpc coming from a player. The player is the player who sent it, the payload is the
    /// specific payload for the rpc.
    fn inform_rpc(&mut self, player: u16, payload: ServerRpcPayload);
    /// Gets invoked when a timer got triggered.
    fn timer_triggered(&mut self, timer_id: u16);

    /// Asks for the current view state that contains all the accumulated changes. This structure has to be updated by the backend architecture itself.
    /// Every delta information that will get drained by drain commands also has to be inserted into the view state.
    fn get_view_state(&self) -> &ViewState;

    /// Gets all the commands generated during the last heartbeat. The vector with the backend commands has to be build up
    /// and maintained by the event handlers from above.  
    /// Return it with
    /// ```
    /// std::mem::take(&mut self.command_queue)
    /// ```
    fn drain_commands(&mut self) -> Vec<BackendCommand<DeltaInformation>>;
}
