//! # A channel/room where clients are connected
mod doc;

pub use doc::DocState;

use crate::lobby::{ChannelID, UserID};
use color_eyre::Report;
use futures_util::{future::{select, Either}, StreamExt};
use tracing::{trace, warn, error, debug};
use prosemirror::markdown::{from_markdown, to_markdown, MarkdownNode, MD};
use prosemirror::transform::{Step, StepResult, Steps};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use tokio::io::{AsyncReadExt, ErrorKind};
use tokio_stream::{wrappers::ReceiverStream};
use tokio::{
    fs::File,
    sync::{broadcast, mpsc, oneshot},
};
use tracing::info;

/// A batch of related steps by the same user. Roughly corresponds to a transaction
#[derive(Debug, Serialize)]
pub struct StepBatch {
    /// The user that send these steps
    pub src: UserID,
    /// The steps to update the editor
    pub steps: Steps<MD>,
}

/// The reply to an initialization message
#[derive(Debug)]
pub struct InitReply {
    /// The last complete state of the doc
    pub doc: String,
    /// The peers that are currently in the channel
    pub j_peers: String,
}

/// A request from a client task to the channel task
#[derive(Debug)]
pub struct Request {
    /// The ID of the client task
    pub source: UserID,
    /// The payload of this request
    pub kind: RequestKind,
}

/// Configuration for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    /// When the name changed, this is `Some(..)` with the new name
    name: Option<String>,
    /// When the audio changed, this is `Some(..)` with the new audio setting
    audio: Option<bool>,
}

/// A kind of request from a client task to the channel
#[derive(Debug)]
pub enum RequestKind {
    /// Send a chat message
    Chat(String),
    /// Send a version and some steps
    Steps(usize, Steps<MD>),
    /// Initialize the connection
    Init {
        /// The reponse channel
        response: oneshot::Sender<InitReply>,
        /// The name of the client if the user selected one
        name: Option<String>,
        /// The sender signal
        sig_tx: mpsc::Sender<Signal>,
    },
    /// Send a signal to another user
    Signal(Signal),
    /// Update the user data
    Update(UserConfig),
    /// Close the connection
    Close,
}

/// A message from the channel to all clients
#[derive(Debug, Clone)]
pub enum Broadcast {
    /// A new user joined the channel
    NewUser {
        /// The ID of the new user
        remote_id: UserID,
        /// The JSON payload for the new user
        data: String,
    },
    /// A user left the channel
    UserLeft(UserID),
    /// A user changed their name
    Update(UserID, UserConfig),
    /// The shared document has been updated with new steps
    Steps(String),
    /// A user sent a chat message
    ChatMessage(UserID, String),
}

/// A signal from one client to another
#[derive(Debug)]
pub struct Signal {
    /// The sender of this signal
    pub sender: UserID,
    /// The reciever of this signal
    pub reciever: UserID,
    /// The kind of this signal
    pub kind: SignalKind,
}

/// A kind of signal from one client to another
#[derive(Debug)]
pub enum SignalKind {
    // IDEA: private chat
    /// A WebRTC signal
    WebRTC(serde_json::Value),
}

/// The data that represents a user
struct UserData {
    /// The name of the user
    name: String,
    /// Whether the user has audio enabled
    audio: bool,
    /// The signal channel
    sig_tx: mpsc::Sender<Signal>,
}

impl UserData {
    /// Get the subset of that data that is public
    fn public(&self) -> PublicMemberData {
        PublicMemberData {
            name: &self.name,
            audio: self.audio,
        }
    }
}

/// Data for a client that is public
#[derive(Debug, Clone, Serialize)]
pub struct PublicMemberData<'a> {
    name: &'a str,
    audio: bool,
}

/// The channel
pub struct Channel {
    /// Communication in the channel
    pub comms: ChannelComms,
    /// The reciever for messages from the clients
    pub msg_rx: mpsc::Receiver<Request>,
    /// The reciever for the termination from the lobby
    pub ter_rx: oneshot::Receiver<()>,
}

/// The outgoing edges from the channel
pub struct ChannelComms {
    /// The channel ID
    pub id: ChannelID,
    /// The path of this channel
    pub path: PathBuf,
    /// The sender for broadcasts
    pub bct_tx: broadcast::Sender<Broadcast>,
    /// The sender to notify the lobby when the channel is empty
    pub end_tx: mpsc::Sender<ChannelID>,
}

impl ChannelComms {
    /// The function to handle an incoming request from a client
    async fn handle_request(&mut self, c_state: &mut ChannelState, request: Request) {
        let id = request.source;
        match request.kind {
            RequestKind::Init {
                response,
                name,
                sig_tx,
            } => {
                let doc = serde_json::to_string(&c_state.doc_state).unwrap();
                // let steps = serde_json::to_string(&c_state.step_buffer).unwrap();

                let new_name = name.unwrap_or_else(|| format!("Bear #{}", id.int_val()));
                let new_data = UserData {
                    name: new_name,
                    audio: false,
                    sig_tx,
                };
                let j_data = serde_json::to_string(&new_data.public()).unwrap();

                c_state.member_data.insert(id, new_data);

                let peers = c_state
                    .member_data
                    .iter()
                    .map(|(id, data)| (id, data.public()))
                    .collect::<HashMap<_, _>>();

                let j_peers = serde_json::to_string(&peers).unwrap();

                let reply = InitReply {
                    doc,
                    //steps,
                    j_peers,
                };

                if let Err(_e) = response.send(reply) {
                    error!("Client dropped while initializing");
                } else {
                    info!("New user: {}", id);
                    self.bct_tx
                        .send(Broadcast::NewUser {
                            remote_id: id,
                            data: j_data,
                        })
                        .unwrap();
                }
            }
            RequestKind::Chat(text) => {
                info!("New message: {}", text);
                self.bct_tx.send(Broadcast::ChatMessage(id, text)).unwrap();
            }
            RequestKind::Update(cfg) => {
                let member = c_state.member_data.get_mut(&id).unwrap();
                if let Some(new_name) = &cfg.name {
                    let old_name = &mut member.name;
                    info!({from = old_name.as_str(), to= new_name.as_str()}, "{} changed their name", id);
                    *old_name = new_name.clone();
                }
                if let Some(audio) = &cfg.audio {
                    member.audio = *audio;
                }
                if let Err(e) = self.bct_tx.send(Broadcast::Update(id, cfg)) {
                    error!("Error sending broadcast {:?}", e);
                }
            }
            RequestKind::Signal(signal) => {
                let member = c_state.member_data.get_mut(&signal.reciever).unwrap();
                trace!("{:?}", signal);
                if let Err(s) = member.sig_tx.send(signal).await {
                    warn!("Failed to send signal {:?}", s);
                }
            }
            RequestKind::Steps(version, steps) => {
                if version == c_state.doc_state.version {
                    info!("Received steps for version {}", version);

                    fn apply_steps(
                        doc: &MarkdownNode,
                        (first, rest): (&Step<MD>, &[Step<MD>]),
                    ) -> StepResult<MD> {
                        debug!("Step {:?}", first);
                        let mut new_doc = first.apply(doc)?;
                        for step in rest {
                            debug!("Step {:?}", step);
                            new_doc = step.apply(&new_doc)?;
                        }
                        Ok(new_doc)
                    }

                    if let Some(fr) = steps.split_first() {
                        match apply_steps(&c_state.doc_state.doc, fr) {
                            Ok(new_doc) => {
                                c_state.doc_state.doc = new_doc;
                                c_state.doc_state.version += steps.len();

                                let batch = StepBatch { src: id, steps };
                                let msg = [&batch];
                                let text = serde_json::to_string(&msg).unwrap();
                                //c_state.step_buffer.push(batch);
                                self.bct_tx.send(Broadcast::Steps(text)).unwrap();
                            }
                            Err(err) => {
                                warn!("Failed to apply some step: {:?}", err);
                            }
                        }
                    } else {
                        debug!("No steps, ignoring!");
                    }
                } else {
                    info!("Rejected steps for outdated version {}", version);
                }
            }
            RequestKind::Close => {
                info!("User left: {}", id);
                c_state.member_data.remove(&id);

                if let Err(err) = self.bct_tx.send(Broadcast::UserLeft(id)) {
                    info!("No client left, shutting down: {:?}", err);
                }
                if let Err(err) = self.end_tx.send(self.id).await {
                    error!("Could not send quit message: {}", err);
                }
            }
        }
    }
}

/// The state of the channel
#[derive(new)]
pub struct ChannelState {
    //step_buffer: Vec<StepBatch>,
    /// The data for each channel member
    #[new(default)]
    member_data: HashMap<UserID, UserData>,
    /// The state of the common document
    doc_state: DocState,
}

impl Channel {
    /// The main task for a channel
    pub async fn handle_messages(mut self) -> Result<(), Report> {
        let path = &self.comms.path;

        let doc_state = match File::open(path).await {
            Ok(mut file) => {
                let mut buf = String::new();
                file.read_to_string(&mut buf).await?;
                let md = from_markdown(&buf)?;
                DocState::new(md)
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                let doc = doc::initial_doc();
                let md = to_markdown(&doc)?;
                tokio::fs::write(path, md).await?;
                DocState::new(doc)
            }
            Err(e) => return Err(Report::from(e)),
        };

        let mut c_state = ChannelState::new(doc_state);

        let mut ter_fut = self.ter_rx;
        let mut msg_rx = ReceiverStream::new(self.msg_rx);
        //pin_mut!(msg_rx);

        let mut msg_fut = msg_rx.next();
        
        //let _ = msg_fut;
        //let mut msg_fut = msg_rx.next();

        loop {
            match select(ter_fut, msg_fut).await {
                Either::Left((Ok(()), _)) => {
                    info!("No clients left, terminating");
                    break;
                }
                Either::Left((Err(_), _)) => {
                    info!("Server shutdown, terminating");
                    break;
                }
                Either::Right((req, ter)) => {
                    if let Some(request) = req {
                        self.comms.handle_request(&mut c_state, request).await;
                    } else {
                        info!("Terminated stream, what is this?");
                    }
                    ter_fut = ter;
                    msg_fut = msg_rx.next();
                }
            }
        }
        let path = &self.comms.path;
        let md = to_markdown(&c_state.doc_state.doc)?;
        std::fs::write(path, md)?;
        Ok(())
    }
}
