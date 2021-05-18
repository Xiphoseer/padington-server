//! # Client distribution
//!
//! This module contains the methods to distribute incoming clients into appropriate
//! channels. The `LobbyServer` responds to `JoinRequest`s and spins up new channels when
//! necessary. It also keeps track of which channels are currently active.
mod server;

pub use server::{ChannelID, LobbyServer, UserID};

use crate::channel::{Broadcast, Request};
use displaydoc::Display;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot};

/// Response from the lobby server to a client
#[derive(Debug)]
pub struct JoinResponse {
    /// The ID that this client is assigned.
    pub id: UserID,
    /// The sender to pass requests into the channel
    pub msg_tx: mpsc::Sender<Request>,
    /// The receiver to listen to events in the channel
    pub bct_rx: broadcast::Receiver<Broadcast>,
}

/// Request to join a channel
#[derive(Debug)]
pub struct JoinRequest {
    /// The path that identifies the channel to join.
    pub path: String,
    /// The channel to send the response over.
    pub response: oneshot::Sender<Result<JoinResponse, JoinError>>,
}

/// Error when joining
#[derive(Debug, Error, Display)]
pub enum JoinError {
    /// Recieving JoinResponse failed
    RecvFailed(#[from] oneshot::error::RecvError),
    /// Sending JoinRequest failed
    SendFailed(#[from] mpsc::error::SendError<JoinRequest>),
    /// Invalid path {0:?}
    InvalidPath(String),
    /// Is folder {0:?}
    IsFolder(String),
}

/// A handle to a lobby server that can be used to send join requests
#[derive(Debug, Clone)]
pub struct LobbyClient(mpsc::Sender<JoinRequest>);

impl From<mpsc::Sender<JoinRequest>> for LobbyClient {
    fn from(inner: mpsc::Sender<JoinRequest>) -> Self {
        Self(inner)
    }
}

impl LobbyClient {
    /// Request to join the given channel
    pub async fn join_channel<S: Into<String>>(
        &mut self,
        path: S,
    ) -> Result<JoinResponse, JoinError> {
        let (tx, rx) = oneshot::channel::<Result<JoinResponse, JoinError>>();

        self.0
            .send(JoinRequest {
                path: path.into(),
                response: tx,
            })
            .await
            .map_err(JoinError::SendFailed)?;

        let recv_result = rx.await?;
        let join_response = recv_result?;
        Ok(join_response)
    }
}
