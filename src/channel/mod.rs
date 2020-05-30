use crate::lobby::{ChannelID, UserID};
use crate::model::{steps::Steps, CodeBlockAttrs, HeadingAttrs, Node};
use futures_util::future::{select, Either};
use log::*;
use serde::Serialize;
use tokio::stream::StreamExt;
use tokio::sync::{broadcast, mpsc, oneshot};

#[derive(Debug, Serialize)]
pub struct StepBatch {
    pub src: UserID,
    pub steps: Steps,
}

#[derive(Debug)]
pub struct InitReply {
    /// The last complete state of the doc
    pub doc: String,
    /// The steps that are not yet part of the doc
    pub steps: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocState {
    doc: Node,
    version: usize,
}

#[derive(Debug)]
pub struct Request {
    pub source: UserID,
    pub kind: RequestKind,
}

#[derive(Debug)]
pub enum RequestKind {
    Chat(String),
    Steps(usize, Steps),
    Init(oneshot::Sender<InitReply>),
    Close,
}

#[derive(Debug, Clone)]
pub enum Broadcast {
    NewUser(UserID),
    UserLeft(UserID),
    Steps(String),
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
        let mut step_buffer: Vec<StepBatch> = Vec::new();

        let mut doc_state = DocState {
            version: 0,
            doc: Node::Doc {
                content: vec![
                    Node::Heading {
                        attrs: HeadingAttrs {
                            level: 1,
                        },
                        content: vec![
                            Node::Text {
                                text: "Padington".to_string(),
                            }
                        ]
                    },
                    Node::CodeBlock {
                        attrs: CodeBlockAttrs {
                            params: String::new(),
                        },
                        content: vec![
                            Node::Text {
                                text: "fn foo(a: u32) -> u32 {\n  2 * a\n}".to_string(),
                            }
                        ]
                    },
                    Node::Heading {
                        attrs: HeadingAttrs {
                            level: 2,
                        },
                        content: vec![
                            Node::Text {
                                text: "Lorem Ipsum".to_string(),
                            }
                        ]
                    },
                    Node::Blockquote {
                        content: vec![
                            Node::Paragraph {
                                content: vec![
                                    Node::Text {
                                        text: String::from("Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet. Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet. Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet.")
                                    }
                                ]
                            }
                        ]
                    }
                ]
            }
        };

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
                }
                Either::Right((req, ter_fut_continue)) => {
                    if let Some(request) = req {
                        let id = request.source;
                        match request.kind {
                            RequestKind::Init(response) => {
                                let doc = serde_json::to_string(&doc_state).unwrap();
                                let steps = serde_json::to_string(&step_buffer).unwrap();
                                let reply = InitReply { doc, steps };

                                if let Err(_e) = response.send(reply) {
                                    error!("Client dropped while initializing");
                                } else {
                                    info!("New user: {}", id);
                                    self.bct_tx.send(Broadcast::NewUser(id)).unwrap();
                                }
                            }
                            RequestKind::Chat(text) => {
                                info!("New message: {}", text);
                                self.bct_tx.send(Broadcast::ChatMessage(id, text)).unwrap();
                            }
                            RequestKind::Steps(version, steps) => {
                                if version == doc_state.version {
                                    info!("Received steps v.{}:", version);
                                    for step in &steps {
                                        info!("Step {:?}", step);
                                    }
                                    doc_state.version += steps.len();
                                    let batch = StepBatch { src: id, steps };
                                    let msg = [&batch];
                                    let text = serde_json::to_string(&msg).unwrap();
                                    step_buffer.push(batch);
                                    self.bct_tx.send(Broadcast::Steps(text)).unwrap();
                                } else {
                                    info!("Rejected steps for outdated version {}", version);
                                }
                            }
                            RequestKind::Close => {
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
