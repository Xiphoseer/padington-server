use super::{JoinRequest, JoinResponse};
use crate::channel::{Broadcast, Channel, ChannelComms, Request};
use crate::util::Counter;
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

#[derive(Debug)]
pub struct LobbyChannel {
    next_id: Counter<UserID>,
    count: u64,
    path: String,
    bct_tx: broadcast::Sender<Broadcast>,
    req_tx: mpsc::Sender<Request>,
    terminate: oneshot::Sender<()>,
}

#[derive(Debug)]
pub struct LobbyServer {
    inner: mpsc::Receiver<JoinRequest>,
    next_id: Counter<ChannelID>,
    channels: HashMap<ChannelID, LobbyChannel>,
    channel_names: HashMap<String, ChannelID>,
}

impl From<mpsc::Receiver<JoinRequest>> for LobbyServer {
    fn from(inner: mpsc::Receiver<JoinRequest>) -> Self {
        Self {
            inner,
            next_id: Counter::default(),
            channels: HashMap::new(),
            channel_names: HashMap::new(),
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
                        match self.channels.entry(sig) {
                            Entry::Vacant(_v) => {
                                error!("Channel entry vanished");
                                break;
                            }
                            Entry::Occupied(mut o) => {
                                let channel = o.get_mut();
                                match channel.count.cmp(&1) {
                                    Ordering::Less => {
                                        error!("Channel {} not cleaned up correctly", sig);
                                        break;
                                    }
                                    Ordering::Equal => {
                                        let channel = o.remove();
                                        self.channel_names.remove(&channel.path);
                                        if let Err(()) = channel.terminate.send(()) {
                                            error!("Error terminating channel {}", sig);
                                        }
                                    }
                                    Ordering::Greater => {
                                        channel.count -= 1;
                                    }
                                }
                            }
                        }
                    }
                    jrq_fut = jrq_fut_continue;
                    sig_fut = end_rx.next();
                }
                Either::Right((msg, sig_fut_continue)) => {
                    if let Some(msg) = msg {
                        let msg: JoinRequest = msg;

                        match self.channel_names.entry(msg.path.clone()) {
                            Entry::Vacant(v) => {
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
                                            bct_tx: bct_tx.clone(),
                                            end_tx: end_tx.clone(),
                                        },
                                    }
                                    .handle_messages(),
                                );

                                let mut next_id = Counter::default();

                                match msg.response.send(JoinResponse {
                                    id: next_id.next(),
                                    msg_tx: req_tx.clone(),
                                    bct_rx,
                                }) {
                                    Ok(()) => {}
                                    Err(_) => error!("Client connection dropped while joining"),
                                }

                                self.channels.insert(
                                    channel_id,
                                    LobbyChannel {
                                        bct_tx,
                                        req_tx,
                                        path: msg.path,
                                        count: 1,
                                        next_id,
                                        terminate: ter_tx,
                                    },
                                );
                                v.insert(channel_id);
                            }
                            Entry::Occupied(o_id) => {
                                let channel_id = o_id.get();
                                let channel = self.channels.get_mut(channel_id).unwrap();
                                channel.count += 1;

                                let id = channel.next_id.next();
                                match msg.response.send(JoinResponse {
                                    id,
                                    msg_tx: channel.req_tx.clone(),
                                    bct_rx: channel.bct_tx.subscribe(),
                                }) {
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
                    sig_fut = sig_fut_continue;
                    jrq_fut = self.inner.next();
                }
            }
        }
    }
}
