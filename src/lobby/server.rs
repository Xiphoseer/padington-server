use super::{JoinError, JoinRequest, JoinResponse};
use crate::channel::{Broadcast, Channel, ChannelComms, Request};
use crate::util::{Counter, LoopState};
use displaydoc::Display;
use futures_util::future::{select, Either};
use log::*;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt;
use tokio::stream::StreamExt;
use tokio::sync::{broadcast, mpsc, oneshot};

macro_rules! make_id {
    ($name:ident, $key:literal) => {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize)]
        #[serde(into = "u64", from = "u64")]
        pub struct $name(u64);

        impl $name {
            pub fn int_val(&self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, $key, self.0)
            }
        }

        impl From<$name> for u64 {
            fn from(u_id: $name) -> u64 {
                u_id.0
            }
        }

        impl From<u64> for $name {
            fn from(id: u64) -> $name {
                UserID(id)
            }
        }
    };
}

make_id!(UserID, "user#{0}");

#[derive(Copy, Clone, Debug, Display, PartialEq, Eq, Hash)]
/// channel#{0}
pub struct ChannelID(u64);
impl From<ChannelID> for u64 {
    fn from(c_id: ChannelID) -> u64 {
        c_id.0
    }
}

impl From<u64> for ChannelID {
    fn from(id: u64) -> ChannelID {
        ChannelID(id)
    }
}

#[derive(Debug, new)]
pub struct LobbyChannel {
    next_id: Counter<UserID>,
    count: u64,
    path: String,
    bct_tx: broadcast::Sender<Broadcast>,
    req_tx: mpsc::Sender<Request>,
    terminate: oneshot::Sender<()>,
}

#[derive(Debug, Default)]
pub struct LobbyState {
    next_id: Counter<ChannelID>,
    channels: HashMap<ChannelID, LobbyChannel>,
    channel_names: HashMap<String, ChannelID>,
}

impl LobbyState {
    async fn handle_end(&mut self, sig: ChannelID) -> LoopState<()> {
        match self.channels.entry(sig) {
            Entry::Vacant(_v) => {
                error!("Channel entry vanished");
                LoopState::Break(())
            }
            Entry::Occupied(mut o) => {
                let channel = o.get_mut();
                match channel.count.cmp(&1) {
                    Ordering::Less => {
                        error!("Channel {} not cleaned up correctly", sig);
                        LoopState::Break(())
                    }
                    Ordering::Equal => {
                        let channel = o.remove();
                        self.channel_names.remove(&channel.path);
                        if let Err(()) = channel.terminate.send(()) {
                            error!("Error terminating channel {}", sig);
                        }
                        LoopState::Continue
                    }
                    Ordering::Greater => {
                        channel.count -= 1;
                        LoopState::Continue
                    }
                }
            }
        }
    }

    pub async fn handle_join_request(
        &mut self,
        msg: JoinRequest,
        end_tx: &mpsc::Sender<ChannelID>,
    ) {
        match self.channel_names.entry(msg.path.clone()) {
            Entry::Vacant(v) => {
                /// The type of file
                enum PathValidity<'a> {
                    Invalid,
                    File(Vec<&'a str>, &'a str),
                    Folder(Vec<&'a str>),
                }
                /// Checks the name for validity
                fn check_name(path: &str) -> PathValidity {
                    let mut iter = path.split('/');
                    if let Some("") = iter.next() {
                        let mut components: Vec<&str> = iter.collect();
                        if let Some(file) = components.pop() {
                            if components.iter().all(|c| {
                                c.len() > 0
                                    && c.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                            }) {
                                if file == "" {
                                    PathValidity::Folder(components)
                                } else {
                                    PathValidity::File(components, file)
                                }
                            } else {
                                PathValidity::Invalid
                            }
                        } else {
                            PathValidity::Invalid
                        }
                    } else {
                        PathValidity::Invalid
                    }
                }

                let response = msg.response;
                let send_join_response =
                    |rep: Result<JoinResponse, JoinError>| match response.send(rep) {
                        Ok(()) => {}
                        Err(_) => error!("Client connection dropped while joining"),
                    };

                match check_name(&msg.path) {
                    PathValidity::Invalid => {
                        send_join_response(Err(JoinError::InvalidPath(msg.path)));
                        return;
                    }
                    PathValidity::Folder(components) => {
                        send_join_response(Err(JoinError::IsFolder(format!("{:?}", components))));
                        return;
                    }
                    PathValidity::File(components, file) => {
                        info!("loading file {:?} {:?}", components, file);
                    }
                }

                let (req_tx, req_rx) = mpsc::channel(100);
                let (bct_tx, bct_rx) = broadcast::channel(100);
                let (ter_tx, ter_rx) = oneshot::channel::<()>();
                let channel_id = self.next_id.next();

                tokio::spawn(
                    Channel {
                        msg_rx: req_rx,
                        ter_rx,
                        comms: ChannelComms {
                            id: channel_id,
                            path: msg.path.clone(),
                            bct_tx: bct_tx.clone(),
                            end_tx: end_tx.clone(),
                        },
                    }
                    .handle_messages(),
                );

                let mut next_id = Counter::default();

                send_join_response(Ok(JoinResponse {
                    id: next_id.next(),
                    msg_tx: req_tx.clone(),
                    bct_rx,
                }));

                self.channels.insert(
                    channel_id,
                    LobbyChannel::new(next_id, 1, msg.path, bct_tx, req_tx, ter_tx),
                );
                v.insert(channel_id);
            }
            Entry::Occupied(o_id) => {
                let channel_id = o_id.get();
                let channel = self.channels.get_mut(channel_id).unwrap();
                channel.count += 1;

                let id = channel.next_id.next();
                match msg.response.send(Ok(JoinResponse {
                    id,
                    msg_tx: channel.req_tx.clone(),
                    bct_rx: channel.bct_tx.subscribe(),
                })) {
                    Ok(()) => {
                        info!("Accepted client {} into channel {}", id, channel_id);
                    }
                    Err(_) => {
                        error!("Client connection {} dropped while joining", id);
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct LobbyServer {
    inner: mpsc::Receiver<JoinRequest>,
    state: LobbyState,
}

impl From<mpsc::Receiver<JoinRequest>> for LobbyServer {
    fn from(inner: mpsc::Receiver<JoinRequest>) -> Self {
        Self {
            inner,
            state: LobbyState::default(),
        }
    }
}

impl LobbyServer {
    pub async fn run(mut self) {
        let (end_tx, mut end_rx) = mpsc::channel::<ChannelID>(5);

        let mut sig_fut = end_rx.next();
        let mut jrq_fut = self.inner.next();
        loop {
            let fut = select(sig_fut, jrq_fut);
            match fut.await {
                Either::Left((sig, jrq_fut_continue)) => {
                    if let Some(sig) = sig {
                        if let LoopState::Break(()) = self.state.handle_end(sig).await {
                            break;
                        }
                    }
                    jrq_fut = jrq_fut_continue;
                    sig_fut = end_rx.next();
                }
                Either::Right((msg, sig_fut_continue)) => {
                    if let Some(msg) = msg {
                        self.state.handle_join_request(msg, &end_tx).await;
                    } else {
                        trace!("JoinRequest stream broke!");
                    }
                    sig_fut = sig_fut_continue;
                    jrq_fut = self.inner.next();
                }
            }
        }
    }
}
