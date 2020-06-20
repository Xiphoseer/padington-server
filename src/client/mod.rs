use crate::channel::{Broadcast, InitReply, Request, RequestKind};
use crate::command::{Command, ParseCommandError};
use crate::lobby::{JoinError, LobbyClient, UserID};
use crate::ClientStream;
use color_eyre::Report;
use eyre::WrapErr;
use futures_util::future::{select, Either};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use log::*;
use prosemirror::markdown::MD;
use prosemirror::transform::Steps;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::WebSocketStream;
use tracing::error;
use tungstenite::http::{
    header::SEC_WEBSOCKET_PROTOCOL, response::Response as HttpResponse, status::StatusCode,
    uri::Uri,
};
use tungstenite::{handshake::server, Message, Result as TResult};

fn make_callback(tx: oneshot::Sender<Uri>) -> impl server::Callback {
    move |http_req: &server::Request, mut http_rep: server::Response| {
        let headers = http_req.headers();
        if let Some(value) = headers.get(SEC_WEBSOCKET_PROTOCOL) {
            if value == "padington" {
                http_rep
                    .headers_mut()
                    .append(SEC_WEBSOCKET_PROTOCOL, value.clone());
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
            let msg = format!("Missing Sec-WebSocket-Protocol header");
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
    msg_tx: &mut mpsc::Sender<Request>,
    ws_sender: &mut SplitSink<WebSocketStream<ClientStream>, Message>,
    cmd_res: Result<Command, ParseCommandError>,
) -> TResult<CommandRes> {
    match cmd_res {
        Ok(Command::Init(name)) => {
            let (tx, rx) = oneshot::channel::<InitReply>();
            let req = Request {
                source: id,
                kind: RequestKind::Init { response: tx, name },
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
                    /*let msg = format!("steps|{}", state.steps);
                    ws_sender.send(Message::text(msg)).await?;*/
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
        Ok(Command::Rename(new_name)) => {
            let req = Request {
                source: id,
                kind: RequestKind::Rename(new_name),
            };
            if let Err(e) = msg_tx.send(req).await {
                error!("{:?}", e);
                return Ok(CommandRes::Break);
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

async fn handle_broadcast(
    msg: Broadcast,
    ws_sender: &mut SplitSink<WebSocketStream<ClientStream>, Message>,
) -> TResult<()> {
    match msg {
        Broadcast::ChatMessage(id, text) => {
            let msg = format!("chat|{}|{}", id.int_val(), text);
            ws_sender.send(Message::text(msg)).await?;
        }
        Broadcast::NewUser { remote_id, data } => {
            let msg = format!("new-user|{}|{}", remote_id.int_val(), data);
            ws_sender.send(Message::text(msg)).await?;
        }
        Broadcast::Rename(id, new_name) => {
            let msg = format!("rename|{}|{}", id.int_val(), new_name);
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

async fn handle_message(
    id: UserID,
    msg: Message,
    msg_tx: &mut mpsc::Sender<Request>,
    ws_sender: &mut SplitSink<WebSocketStream<ClientStream>, Message>,
) -> Result<CommandRes, Report> {
    match msg {
        Message::Text(t) => {
            let cmd_res = t.parse();
            handle_command(id, msg_tx, ws_sender, cmd_res).await?;
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
    let mut bct_rx = join_response.bct_rx;
    let id: UserID = join_response.id;

    let mut interval = tokio::time::interval(Duration::from_millis(1000));
    // Echo incoming WebSocket messages and send a message periodically every second.

    let int_fut = interval.next();
    let msg_fut = ws_receiver.next();

    let mut int_or_msg_fut = select(msg_fut, int_fut);
    let mut bct_fut = bct_rx.next();
    loop {
        trace!("Loop iteration");
        match select(int_or_msg_fut, bct_fut).await {
            Either::Left((msg_or_int, bct_fut_continue)) => {
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

                                match handle_message(id, msg, &mut msg_tx, &mut ws_sender).await {
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
                bct_fut = bct_fut_continue; // Continue waiting for broadcasts
            }
            Either::Right((bct, int_or_msg_fut_continue)) => {
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

                int_or_msg_fut = int_or_msg_fut_continue; // Continue receiving the WebSocket message.
                bct_fut = bct_rx.next(); // Wait for next broadcast.
            }
        }
    }

    trace!("Leaving handle_connection");

    Ok(())
}
