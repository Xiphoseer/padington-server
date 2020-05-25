use crate::command::Command;
use crate::lobby::{ChannelID, UserID};
use log::*;
use futures_util::future::{select, Either};
use tokio::stream::StreamExt;
use tokio::sync::{broadcast, mpsc, oneshot};

#[derive(Debug, Clone)]
pub struct Request {
    pub source: UserID,
    pub cmd: Command,
}

#[derive(Debug, Clone)]
pub enum Broadcast {
    NewUser(UserID),
    UserLeft(UserID),
    ChatMessage(UserID, String),
}

pub struct Channel {
    pub id: ChannelID,
    pub msg_rx: mpsc::Receiver<Request>,
    pub bct_tx: broadcast::Sender<Broadcast>,
    pub end_tx: mpsc::Sender<ChannelID>,
    pub ter_rx: oneshot::Receiver<()>,
}

impl Channel {
    pub async fn handle_messages(mut self) {

        let mut ter_fut = self.ter_rx;
        let mut msg_fut = self.msg_rx.next();
        loop {
            match select(ter_fut, msg_fut).await {
                Either::Left((ter, _msg_fut_continue)) => {
                    match ter {
                        Ok(()) => info!("No clients left, terminating"),
                        Err(_) => info!("Server shutdown, terminating"),
                    }
                    // TODO: shutdown
                    break;
                },
                Either::Right((req, ter_fut_continue)) => {
                    if let Some(request) = req {
                        let id = request.source;
                        match request.cmd {
                            Command::Init => {
                                info!("New user: {}", id);
                                self.bct_tx.send(Broadcast::NewUser(id)).unwrap();
                            }
                            Command::Chat(text) => {
                                info!("New message: {}", text);
                                self.bct_tx.send(Broadcast::ChatMessage(id, text)).unwrap();
                            }
                            Command::Close => {
                                info!("User left: {}", id);

                                if let Err(err) = self.bct_tx.send(Broadcast::UserLeft(id)) {
                                    info!("No client left, shutting down: {:?}", err);
                                }
                                if let Err(err) = self.end_tx.send(self.id).await {
                                    error!("Could not send quit message: {}", err);
                                }
                            }
                        }
                    } else {
                        info!("Terminated stream, what is this?");
                    }
                    ter_fut = ter_fut_continue;
                    msg_fut = self.msg_rx.next();
                }
            }
        }
    }
}
