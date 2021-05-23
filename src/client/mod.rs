//! # Connections to clients

use crate::channel::{Broadcast, InitReply, Request, RequestKind, Signal, SignalKind, UserConfig};
use crate::command::{Command, ParseCommandError};
use crate::lobby::{JoinError, LobbyClient, UserID};
use crate::ClientStream;
use color_eyre::{eyre::WrapErr, Report};
use futures_util::future::{select, Either};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use prosemirror::markdown::MD;
use prosemirror::transform::Steps;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::{
    sync::{mpsc, oneshot},
    time::interval,
};
use tokio_stream::wrappers::{IntervalStream, ReceiverStream, BroadcastStream};
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::WebSocketStream;
use tracing::{debug, error, info, trace, warn};
use tungstenite::http::{
    header::SEC_WEBSOCKET_PROTOCOL, response::Response as HttpResponse, status::StatusCode,
    uri::Uri, HeaderValue,
};
use tungstenite::{handshake::server, Message, Result as TResult};

type WsSender = SplitSink<WebSocketStream<ClientStream>, Message>;

fn make_callback(tx: oneshot::Sender<Uri>) -> impl server::Callback {
    move |http_req: &server::Request, mut http_rep: server::Response| {
        let headers = http_req.headers();
        if let Some(value) = headers.get(SEC_WEBSOCKET_PROTOCOL) {
            if value == "padington" {
                let headers = http_rep.headers_mut();
                headers.append(SEC_WEBSOCKET_PROTOCOL, value.clone());
                headers.append(
                    tungstenite::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
                    HeaderValue::from_static("*"),
                );
                match tx.send(http_req.uri().clone()) {
                    Ok(_) => Ok(http_rep),
                    Err(e) => todo!("{}", e),
                }
            } else {
                let msg = format!("Invalid protocol {:?}", value);
                error!("Invalid protocol {:?}", value);
                let mut rep = HttpResponse::new(Some(msg));
                *rep.status_mut() = StatusCode::NOT_ACCEPTABLE;
                Err(rep)
            }
        } else {
            let msg = "Missing Sec-WebSocket-Protocol header".to_string();
            error!("Missing Sec-WebSocket-Protocol header");
            let mut rep = HttpResponse::new(Some(msg));
            *rep.status_mut() = StatusCode::NOT_ACCEPTABLE;
            Err(rep)
        }
    }
}

enum CommandRes {
    Break,
    Continue,
}

async fn handle_command(
    id: UserID,
    sig_tx: &mut mpsc::Sender<Signal>,
    msg_tx: &mut mpsc::Sender<Request>,
    ws_sender: &mut WsSender,
    cmd_res: Result<Command, ParseCommandError>,
) -> TResult<CommandRes> {
    match cmd_res {
        Ok(Command::Init(name)) => {
            let (tx, rx) = oneshot::channel::<InitReply>();
            let req = Request {
                source: id,
                kind: RequestKind::Init {
                    response: tx,
                    name,
                    sig_tx: sig_tx.clone(),
                },
            };
            if let Err(e) = msg_tx.send(req).await {
                error!("{:?}", e);
                return Ok(CommandRes::Break);
            }
            match rx.await {
                Ok(state) => {
                    let msg = format!("init|{}|{}", id.int_val(), state.doc);
                    ws_sender.send(Message::text(msg)).await?;
                    let msg = format!("peers|{}", state.j_peers);
                    ws_sender.send(Message::text(msg)).await?;
                }
                Err(err) => {
                    error!("{}", err);
                }
            }
        }
        Ok(Command::Chat(msg)) => {
            let req = Request {
                source: id,
                kind: RequestKind::Chat(msg),
            };
            if let Err(e) = msg_tx.send(req).await {
                error!("{:?}", e);
                return Ok(CommandRes::Break);
            }
        }
        Ok(Command::Update(payload)) => {
            let update: Result<UserConfig, _> = serde_json::from_str(&payload);
            match update {
                Ok(cfg) => {
                    debug!("Recieved Update {:?} from {:?}", cfg, id);
                    let req = Request {
                        source: id,
                        kind: RequestKind::Update(cfg),
                    };
                    if let Err(e) = msg_tx.send(req).await {
                        error!("{:?}", e);
                        return Ok(CommandRes::Break);
                    }
                }
                Err(e) => {
                    error!("{:?}", e);
                    return Ok(CommandRes::Break);
                }
            }
        }
        Ok(Command::WebRTC(reciever, payload)) => {
            let value: Result<serde_json::Value, _> = serde_json::from_str(&payload);
            match value {
                Ok(value) => {
                    let req = Request {
                        source: id,
                        kind: RequestKind::Signal(Signal {
                            sender: id,
                            reciever: UserID::from(reciever),
                            kind: SignalKind::WebRTC(value),
                        }),
                    };
                    if let Err(e) = msg_tx.send(req).await {
                        error!("{:?}", e);
                        return Ok(CommandRes::Break);
                    }
                }
                Err(e) => {
                    error!("{:?}", e);
                    return Ok(CommandRes::Break);
                }
            }
        }
        Ok(Command::Steps(version, string)) => {
            debug!("Step Text: {:?}", string);
            let steps_res: Result<Steps<MD>, _> = serde_json::from_str(&string);

            match steps_res {
                Ok(steps) => {
                    let req = Request {
                        source: id,
                        kind: RequestKind::Steps(version, steps),
                    };
                    if let Err(e) = msg_tx.send(req).await {
                        error!("{:?}", e);
                        return Ok(CommandRes::Break);
                    }
                }
                Err(e) => {
                    error!("{:?}", e);
                    return Ok(CommandRes::Break);
                }
            }
        }
        Ok(Command::Close) => {
            let req = Request {
                source: id,
                kind: RequestKind::Close,
            };
            if let Err(e) = msg_tx.send(req).await {
                error!("{:?}", e);
            }
            return Ok(CommandRes::Break);
        }
        Err(err) => {
            ws_sender
                .send(Message::text(format!("error|{}", err)))
                .await?;
        }
    }
    Ok(CommandRes::Continue)
}

async fn submit_close(id: UserID, msg_tx: &mut mpsc::Sender<Request>) {
    let close_req = Request {
        source: id,
        kind: RequestKind::Close,
    };
    match msg_tx.send(close_req).await {
        Ok(()) => {
            debug!("Sent {:?}", Command::Close);
        }
        Err(e) => {
            error!("Failed to send {:?} ({:?})", Command::Close, e);
        }
    }
}

async fn handle_broadcast(msg: Broadcast, ws_sender: &mut WsSender) -> TResult<()> {
    match msg {
        Broadcast::ChatMessage(id, text) => {
            let msg = format!("chat|{}|{}", id.int_val(), text);
            ws_sender.send(Message::text(msg)).await?;
        }
        Broadcast::NewUser { remote_id, data } => {
            let msg = format!("new-user|{}|{}", remote_id.int_val(), data);
            ws_sender.send(Message::text(msg)).await?;
        }
        Broadcast::Update(id, cfg) => {
            debug!("Sending update {:?} for {:?}", cfg, id);
            let msg = format!(
                "update|{}|{}",
                id.int_val(),
                serde_json::to_string(&cfg).unwrap()
            );
            ws_sender.send(Message::text(msg)).await?;
        }
        Broadcast::UserLeft(id) => {
            let msg = format!("user-left|{}", id.int_val());
            ws_sender.send(Message::text(msg)).await?;
        }
        Broadcast::Steps(steps) => {
            let msg = format!("steps|{}", steps);
            ws_sender.send(Message::text(msg)).await?;
        }
    }
    Ok(())
}

async fn handle_signal(signal: Signal, ws_sender: &mut WsSender) -> TResult<()> {
    match signal.kind {
        SignalKind::WebRTC(payload) => {
            let msg = format!(
                "webrtc|{}|{}",
                signal.sender.int_val(),
                serde_json::to_string(&payload).unwrap()
            );
            ws_sender.send(Message::text(msg)).await?;
        }
    }
    Ok(())
}

async fn handle_message(
    id: UserID,
    msg: Message,
    sig_tx: &mut mpsc::Sender<Signal>,
    msg_tx: &mut mpsc::Sender<Request>,
    ws_sender: &mut WsSender,
) -> Result<CommandRes, Report> {
    match msg {
        Message::Text(t) => {
            let cmd_res = t.parse();
            handle_command(id, sig_tx, msg_tx, ws_sender, cmd_res).await?;
        }
        Message::Binary(b) => {
            ws_sender.send(Message::binary(b)).await?;
        }
        Message::Close(c) => {
            debug!("WebSocket closed ({:?})", c);
            submit_close(id, msg_tx).await;
            return Ok(CommandRes::Break);
        }
        Message::Ping(p) => {
            if let Err(err) = ws_sender.send(Message::Pong(p)).await {
                error!("Failed to send pong: {}", err);
                submit_close(id, msg_tx).await;
                return Ok(CommandRes::Break);
            }
        }
        Message::Pong(_) => {}
    }
    Ok(CommandRes::Continue)
}

/// Handle an incoming connection
pub async fn handle_connection(
    mut lc: LobbyClient,
    peer: SocketAddr,
    stream: ClientStream,
) -> Result<(), Report> {
    let (tx, rx) = oneshot::channel::<Uri>();
    let ws_stream: WebSocketStream<ClientStream> =
        accept_hdr_async(stream, make_callback(tx)).await?;
    let uri: Uri = rx.await.wrap_err("Callback dropped")?;
    let start_time = Instant::now();

    info!("New WebSocket connection: {} to {}", peer, uri);
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    let channel_path = urlencoding::decode(uri.path())?;
    let join_response = match lc.join_channel(channel_path).await {
        Ok(jr) => jr,
        Err(JoinError::IsFolder(c)) => {
            let msg = format!("folder|{}", c);
            ws_sender.send(Message::text(msg)).await?;
            ws_sender.send(Message::Close(None)).await?;
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    let mut msg_tx = join_response.msg_tx;
    let mut bct_rx = BroadcastStream::new(join_response.bct_rx);
    let id: UserID = join_response.id;

    let mut interval = IntervalStream::new(interval(Duration::from_millis(1000)));
    // Echo incoming WebSocket messages and send a message periodically every second.

    let (mut sig_tx, sig_rx) = mpsc::channel::<Signal>(20);
    let mut sig_rx = ReceiverStream::new(sig_rx);

    let int_fut = interval.next();
    let msg_fut = ws_receiver.next();

    let bct_fut = bct_rx.next();
    let sig_fut = sig_rx.next();

    let mut int_or_msg_fut = select(msg_fut, int_fut);
    let mut bct_or_sig_fut = select(bct_fut, sig_fut);
    loop {
        trace!("Loop iteration");
        match select(int_or_msg_fut, bct_or_sig_fut).await {
            Either::Left((msg_or_int, bct_or_sig_fut_continue)) => {
                match msg_or_int {
                    Either::Left((msg, int_fut_continue)) => {
                        trace!("Message");
                        match msg {
                            Some(msg) => {
                                let msg = match msg {
                                    Err(e) => {
                                        error!("Error on input stream: {}", e);
                                        submit_close(id, &mut msg_tx).await;
                                        break;
                                    }
                                    Ok(msg) => msg,
                                };

                                match handle_message(
                                    id,
                                    msg,
                                    &mut sig_tx,
                                    &mut msg_tx,
                                    &mut ws_sender,
                                )
                                .await
                                {
                                    Ok(CommandRes::Break) => break,
                                    Ok(CommandRes::Continue) => {}
                                    Err(err) => {
                                        error!("Could not handle message: {}", err);
                                        break;
                                    }
                                }
                            }
                            None => {
                                debug!("WebSocket stream was terminated unexpectedly");
                                submit_close(id, &mut msg_tx).await;
                                break;
                            }
                        };

                        int_or_msg_fut = select(ws_receiver.next(), int_fut_continue);
                    }
                    Either::Right((opt_instant, msg_fut_continue)) => {
                        trace!("Send ping to {}", id);
                        let time = opt_instant.unwrap();
                        let dur = time.into_std().duration_since(start_time);
                        let bytes: [u8; 16] = dur.as_micros().to_le_bytes();
                        let vec: Vec<u8> = Vec::from(&bytes[..]);
                        if let Err(err) = ws_sender.send(Message::Ping(vec)).await {
                            error!("Could not send ping: {}", err);
                            submit_close(id, &mut msg_tx).await;
                            break;
                        }

                        int_or_msg_fut = select(msg_fut_continue, interval.next());
                    }
                }
                bct_or_sig_fut = bct_or_sig_fut_continue; // Continue waiting for broadcasts
            }
            Either::Right((bct_or_sig, int_or_msg_fut_continue)) => {
                match bct_or_sig {
                    Either::Left((bct, sig_fut_continue)) => {
                        if let Some(msg) = bct {
                            match msg {
                                Ok(msg) => {
                                    if let Err(err) = handle_broadcast(msg, &mut ws_sender).await {
                                        error!("Could not send broadcast: {}", err);
                                        //submit_close(id, &mut msg_tx).await;
                                        //break;
                                    }
                                }
                                Err(err) => {
                                    error!("Could not recieve broadcast: {}", err);
                                    //submit_close(id, &mut msg_tx).await;
                                    //break;
                                }
                            }
                        } else {
                            // End of stream
                            info!("End of stream");
                        }

                        // Wait for next broadcast.
                        bct_or_sig_fut = select(bct_rx.next(), sig_fut_continue);
                    }
                    Either::Right((sig, bct_fut_continue)) => {
                        if let Some(signal) = sig {
                            if let Err(err) = handle_signal(signal, &mut ws_sender).await {
                                warn!("Could not handle signal {:?}", err);
                            }
                        } else {
                            // None signal, end of stream
                        }
                        bct_or_sig_fut = select(bct_fut_continue, sig_rx.next());
                    }
                }

                int_or_msg_fut = int_or_msg_fut_continue; // Continue receiving the WebSocket message.
            }
        }
    }

    trace!("Leaving handle_connection");

    Ok(())
}
