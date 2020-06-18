pub mod server;

pub use server::{ChannelID, LobbyServer, UserID};

use crate::channel::{Broadcast, Request};
use displaydoc::Display;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot};

#[derive(Debug)]
pub struct JoinResponse {
    pub id: UserID,
    pub msg_tx: mpsc::Sender<Request>,
    pub bct_rx: broadcast::Receiver<Broadcast>,
}

#[derive(Debug)]
pub struct JoinRequest {
    pub path: String,
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

#[derive(Debug, Clone)]
pub struct LobbyClient(mpsc::Sender<JoinRequest>);

impl From<mpsc::Sender<JoinRequest>> for LobbyClient {
    fn from(inner: mpsc::Sender<JoinRequest>) -> Self {
        Self(inner)
    }
}

impl LobbyClient {
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
            .map_err(|err| JoinError::SendFailed(err))?;

        let recv_result = rx.await?;
        let join_response = recv_result?;
        Ok(join_response)
    }
}
