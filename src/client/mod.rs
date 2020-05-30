use crate::channel::{Broadcast, InitReply, Request, RequestKind};
use crate::command::{Command, ParseCommandError};
use crate::lobby::{JoinResponse, LobbyClient, UserID};
use crate::model::steps::Steps;
use crate::ClientStream;
use futures_util::future::{select, Either};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use log::*;
use std::net::SocketAddr;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::WebSocketStream;
use tungstenite::http::uri::Uri;
use tungstenite::{handshake::server, Message, Result as TResult};

fn make_callback(tx: oneshot::Sender<Uri>) -> impl server::Callback {
    move |http_req: &server::Request, http_rep: server::Response| match tx
        .send(http_req.uri().clone())
    {
        Ok(_) => Ok(http_rep),
        Err(e) => todo!("{}", e),
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
        Ok(Command::Init) => {
            let (tx, rx) = oneshot::channel::<InitReply>();
            let req = Request {
                source: id,
                kind: RequestKind::Init(tx),
            };
            if let Err(e) = msg_tx.send(req).await {
                error!("{:?}", e);
                return Ok(CommandRes::Break);
            }
            match rx.await {
                Ok(state) => {
                    let msg = format!("init|{}|{}", id.int_val(), state.doc);
                    ws_sender.send(Message::text(msg)).await?;
                    let msg = format!("steps|{}", state.steps);
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
        Ok(Command::Steps(version, string)) => {
            debug!("Step Text: {:?}", string);
            let steps_res: Result<Steps, _> = serde_json::from_str(&string);

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
                return Ok(CommandRes::Break);
            }
        }
        Err(err) => {
            ws_sender
                .send(Message::text(format!("error|{}", err)))
                .await?;
        }
    }
    Ok(CommandRes::Continue)
}

pub async fn handle_connection(
    mut lc: LobbyClient,
    peer: SocketAddr,
    stream: ClientStream,
) -> TResult<()> {
    let (tx, rx) = oneshot::channel::<Uri>();
    let ws_stream: WebSocketStream<ClientStream> = accept_hdr_async(stream, make_callback(tx))
        .await
        .expect("Failed to accept");
    let uri: Uri = rx.await.expect("Callback dropped");

    let channel_path = uri.path();
    let join_response: JoinResponse = lc.join_channel(channel_path).await.unwrap();
    let mut msg_tx = join_response.msg_tx;
    let mut bct_rx = join_response.bct_rx;
    let id: UserID = join_response.id;

    info!("New WebSocket connection: {} to {}", peer, uri);
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    //let mut interval = tokio::time::interval(Duration::from_millis(1000));
    // Echo incoming WebSocket messages and send a message periodically every second.

    let mut msg_fut = ws_receiver.next();
    let mut bct_fut = bct_rx.next();
    loop {
        match select(msg_fut, bct_fut).await {
            Either::Left((msg, bct_fut_continue)) => {
                match msg {
                    Some(msg) => {
                        let msg = msg?;
                        match msg {
                            Message::Text(t) => {
                                let cmd_res = t.parse();
                                match handle_command(id, &mut msg_tx, &mut ws_sender, cmd_res)
                                    .await?
                                {
                                    CommandRes::Break => break,
                                    CommandRes::Continue => {}
                                }
                            }
                            Message::Binary(b) => {
                                ws_sender.send(Message::binary(b)).await?;
                            }
                            Message::Close(c) => {
                                debug!("WebSocket closed ({:?})", c);
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
                                break;
                            }
                            _ => {}
                        }

                        bct_fut = bct_fut_continue; // Continue waiting for broadcasts
                        msg_fut = ws_receiver.next(); // Receive next WebSocket message.
                    }
                    None => {
                        debug!("WebSocket stream was terminated unexpectedly");
                        break;
                    }
                };
            }
            Either::Right((bct, msg_fut_continue)) => {
                if let Some(msg) = bct {
                    match msg {
                        Ok(msg) => match msg {
                            Broadcast::ChatMessage(id, text) => {
                                let msg = format!("chat|{}|{}", id.int_val(), text);
                                ws_sender.send(Message::text(msg)).await?;
                            }
                            Broadcast::NewUser(id) => {
                                let msg = format!("new-user|{}", id.int_val());
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
                        },
                        Err(err) => {
                            info!("An error occured: {}", err);
                        }
                    }
                }

                msg_fut = msg_fut_continue; // Continue receiving the WebSocket message.
                bct_fut = bct_rx.next(); // Wait for next broadcast.
            }
        }
    }

    Ok(())
}
