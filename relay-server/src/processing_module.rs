//! Here we handle the core communication

use protocol::*;
use axum::extract::ws::{Message, WebSocket};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::broadcast;
use tokio::sync::broadcast::Sender;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::Receiver;

/// Spawns two tokio tasks for the web-socket, that is connected with the game server.
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

/// We take care of messages, that are coming from the outer point.
async fn receive_logic_server(
    mut receiver: SplitStream<WebSocket>,
    internal_sender: Sender<Bytes>,
) -> &'static str {
    while let Some(state) = receiver.next().await {
        match state
        {
            Ok(Message::Binary(bytes) ) => {
                if bytes.is_empty() {
                    tracing::error!("Illegal empty message in receive logic server.");
                    return "Illegal empty message received.";
                }
                // Message is sent in the clean up phase anyway.
                if bytes[0] == SERVER_DISCONNECTS {
                    // This something normal to be expected.
                    return "Server disconnected intentionally";
                }

                if (bytes[0] != CLIENT_GETS_KICKED)
                    && (bytes[0] != DELTA_UPDATE)
                    && (bytes[0] != FULL_UPDATE)
                    && (bytes[0] != RESET)
                {
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
            Ok(_) => {} // We simply ignore other messages.
            Err(_) => {
                return "Connection lost.";
            }
        }
    }
    "Connection lost."
}

/// We take care of messages that are coming from inside.
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
        if (bytes[0] != NEW_CLIENT)
            && (bytes[0] != CLIENT_DISCONNECTS)
            && (bytes[0] != SERVER_RPC)
        {
            tracing::error!(
                message_type = bytes[0],
                "Unknown internal Client->Server command"
            );
            return "Unknown internal Client->Server command";
        }
        // Simply pass on the messsage.
        let res = enclosed.send(Message::Binary(bytes)).await;
        if let Err(err) = res {
            tracing::error!(?err, "Error in communication with server endpoint.");
            return "Error in communication with server endpoint.";
        }
    }

    // In normal shutdown procedure that should not happen, because we are responsible for closing the channel.
    tracing::error!("Internal channel onm server was unexpectedly closed.");
    "Internal channel closed."
}

/// Spawns the two tokio tasks for the client and does all the handling.
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

/// Takes care of the messages that are coming from the outside.
async fn receive_logic_client(
    mut receiver: SplitStream<WebSocket>,
    internal_sender: tokio::sync::mpsc::Sender<Bytes>,
    player_id: u16,
) -> &'static str {
    while let Some(state) = receiver.next().await {
        match state
        {
            Ok(Message::Binary(bytes)) => {
                if bytes.is_empty() {
                    tracing::error!("Illegal empty message received in receive logic client.");
                    return "Illegal empty message received.";
                }
                match bytes[0]
                {
                    SERVER_RPC => {
                        // This is the RPC command, we need to add our client id.
                        let mut msg = BytesMut::with_capacity(bytes.len() + CLIENT_ID_SIZE);
                        msg.put_u8(SERVER_RPC);
                        msg.put_u16(player_id);
                        // Skip the first byte
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

/// This is the client logic for commands coming from the inside.
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
                        // We have to see if  we are meant.
                        if bytes.len() < 3 {
                            tracing::error!("Malformed CLIENT_GETS_KICKED message");
                            return "Malformed message received.";
                        }
                        bytes.get_u8();
                        let meant_client = bytes.get_u16();
                        if meant_client == player_id {
                            return "We got rejected by server.";
                        }
                    }
                    DELTA_UPDATE => {
                        // Only pass deltas through. if we are synced.
                        if is_synced {
                            let test = enclosed.send(Message::Binary(bytes)).await;
                            if let Err(error) = test {
                                tracing::error!(
                                    ?error,
                                    "Error in communication with client endpoint."
                                );
                                return "Error in communication with client endpoint.";
                            }
                        }
                    }

                    FULL_UPDATE => {
                        // Only pass full updates through if we are not synced and flag as sync.
                        if !is_synced {
                            is_synced = true;
                            let test = enclosed.send(Message::Binary(bytes)).await;
                            if let Err(error) = test {
                                tracing::error!(
                                    ?error,
                                    "Error in communication with client endpoint."
                                );
                                return "Error in communication with client endpoint.";
                            }
                        }
                    }

                    RESET => {
                        // We simply forward the message and are definitively synced here.
                        is_synced = true;
                        let test = enclosed.send(Message::Binary(bytes)).await;
                        if let Err(error) = test {
                            tracing::error!(
                                    ?error,
                                    "Error in communication with client endpoint."
                                );
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
