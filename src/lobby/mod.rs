pub mod server;

pub use server::{ChannelID, LobbyServer, UserID};

use crate::channel::{Broadcast, Request};
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
    pub response: oneshot::Sender<JoinResponse>,
}

#[derive(Debug)]
pub enum JoinError {
    RecvFailed(oneshot::error::RecvError),
    SendFailed(mpsc::error::SendError<JoinRequest>),
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
        let (tx, rx) = oneshot::channel::<JoinResponse>();

        self.0
            .send(JoinRequest {
                path: path.into(),
                response: tx,
            })
            .await
            .map_err(|err| JoinError::SendFailed(err))?;

        rx.await.map_err(|err| JoinError::RecvFailed(err))
    }
}
