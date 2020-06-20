mod doc;

pub use doc::DocState;

use crate::lobby::{ChannelID, UserID};
use color_eyre::Report;
use futures_util::future::{select, Either};
use log::*;
use prosemirror::markdown::{from_markdown, to_markdown, MarkdownNode, MD};
use prosemirror::transform::{Step, StepResult, Steps};
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf};
use tokio::io::{AsyncReadExt, ErrorKind};
use tokio::stream::StreamExt;
use tokio::{
    fs::File,
    sync::{broadcast, mpsc, oneshot},
};
use tracing::info;

#[derive(Debug, Serialize)]
pub struct StepBatch {
    pub src: UserID,
    pub steps: Steps<MD>,
}

#[derive(Debug)]
pub struct InitReply {
    /// The last complete state of the doc
    pub doc: String,
    // /// The steps that are not yet part of the doc
    // pub steps: String,
    /// The peers that are currently in the channel
    pub j_peers: String,
}

#[derive(Debug)]
pub struct Request {
    pub source: UserID,
    pub kind: RequestKind,
}

#[derive(Debug)]
pub enum RequestKind {
    Chat(String),
    Rename(String),
    Steps(usize, Steps<MD>),
    Init {
        response: oneshot::Sender<InitReply>,
        name: Option<String>,
    },
    Close,
}

#[derive(Debug, Clone)]
pub enum Broadcast {
    NewUser { remote_id: UserID, data: String },
    UserLeft(UserID),
    Rename(UserID, String),
    Steps(String),
    ChatMessage(UserID, String),
}

struct UserData {
    name: String,
}

impl UserData {
    fn public<'a>(&'a self) -> PublicMemberData<'a> {
        PublicMemberData { name: &self.name }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicMemberData<'a> {
    name: &'a str,
}

pub struct Channel {
    pub comms: ChannelComms,
    pub msg_rx: mpsc::Receiver<Request>,
    pub ter_rx: oneshot::Receiver<()>,
}

pub struct ChannelComms {
    pub id: ChannelID,
    pub path: PathBuf,
    pub bct_tx: broadcast::Sender<Broadcast>,
    pub end_tx: mpsc::Sender<ChannelID>,
}

impl ChannelComms {
    async fn handle_request(&mut self, c_state: &mut ChannelState, request: Request) {
        let id = request.source;
        match request.kind {
            RequestKind::Init { response, name } => {
                let doc = serde_json::to_string(&c_state.doc_state).unwrap();
                // let steps = serde_json::to_string(&c_state.step_buffer).unwrap();

                let new_name = name.unwrap_or_else(|| format!("Bear #{}", id.int_val()));
                let new_data = UserData { name: new_name };
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
            RequestKind::Rename(new_name) => {
                let old_name = &mut c_state.member_data.get_mut(&id).unwrap().name;
                info!({from = old_name.as_str(), to= new_name.as_str()}, "{} changed their name", id);
                std::mem::replace(old_name, new_name.clone());
                self.bct_tx.send(Broadcast::Rename(id, new_name)).unwrap();
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

#[derive(new)]
pub struct ChannelState {
    //step_buffer: Vec<StepBatch>,
    #[new(default)]
    member_data: HashMap<UserID, UserData>,
    doc_state: DocState,
}

impl Channel {
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
        let mut msg_fut = self.msg_rx.next();
        loop {
            match select(ter_fut, msg_fut).await {
                Either::Left((ter, _msg_fut_continue)) => {
                    match ter {
                        Ok(()) => info!("No clients left, terminating"),
                        Err(_) => info!("Server shutdown, terminating"),
                    }

                    let path = &self.comms.path;
                    let md = to_markdown(&c_state.doc_state.doc)?;
                    std::fs::write(path, md)?;

                    break Ok(());
                }
                Either::Right((req, ter_fut_continue)) => {
                    if let Some(request) = req {
                        self.comms.handle_request(&mut c_state, request).await
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
