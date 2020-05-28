pub mod channel;
pub mod client;
pub mod command;
pub mod config;
pub mod lobby;
pub mod model;
pub mod util;

use crate::client::handle_connection;
use crate::config::{ConnSetup, Flags};
use crate::lobby::{JoinRequest, LobbyClient, LobbyServer};
use futures_util::future::ready;
use log::*;
use std::future::Future;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use structopt::StructOpt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::rustls::{NoClientAuth, ServerConfig};
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use tokio_tungstenite::stream::Stream;
use tokio_tungstenite::tungstenite::Error;

async fn accept_connection(lc: LobbyClient, peer: SocketAddr, stream: ClientStream) {
    if let Err(e) = handle_connection(lc, peer, stream).await {
        match e {
            Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
            err => error!("Error processing connection: {}", err),
        }
    }
}

type ClientStream = Stream<TcpStream, TlsStream<TcpStream>>;

async fn wait_for_connections<F, R>(
    mut listener: TcpListener,
    lobby_sender: mpsc::Sender<JoinRequest>,
    map: F,
) where
    F: Fn(TcpStream) -> R,
    R: Future<Output = Result<ClientStream, io::Error>>,
{
    while let Ok((stream, peer)) = listener.accept().await {
        let lc = LobbyClient::from(lobby_sender.clone());
        match map(stream).await {
            Ok(stream) => {
                tokio::spawn(accept_connection(lc, peer, stream));
            }
            Err(e) => error!("Invalid connection request: {:?}", e),
        }
    }
}

#[tokio::main]
async fn main() {
    if std::env::var(env_logger::DEFAULT_FILTER_ENV).is_err() {
        std::env::set_var(
            env_logger::DEFAULT_FILTER_ENV,
            #[cfg(debug_assertions)]
            "warn,padington_server=trace",
            #[cfg(not(debug_assertions))]
            "warn,padington_server=info",
        );
    }
    env_logger::init();

    let flags = Flags::from_args();
    let cfg = flags.load_cfg().await.unwrap();

    let addr = cfg.addr.as_str().to_socket_addrs().unwrap().next().unwrap();

    let (lobby_sender, lobby_receiver) = mpsc::channel(100);

    tokio::spawn(LobbyServer::from(lobby_receiver).run());

    let listener = TcpListener::bind(&addr).await.expect("Can't listen");
    info!("Listening on: {}", addr);

    match cfg.conn {
        ConnSetup::Basic => {
            wait_for_connections(listener, lobby_sender, |stream| {
                ready(Ok(Stream::Plain(stream)))
            })
            .await;
        }
        ConnSetup::Tls { certs, mut keys } => {
            let mut config = ServerConfig::new(NoClientAuth::new());
            if keys.len() < 1 {
                panic!("Key-File contains no keys");
            }
            if let Err(e) = config.set_single_cert(certs, keys.remove(0)) {
                error!("{}", e);
            }
            let acceptor = TlsAcceptor::from(Arc::new(config));
            wait_for_connections(listener, lobby_sender, |stream: TcpStream| async {
                let acceptor = acceptor.clone();
                let stream = acceptor.accept(stream).await?;
                Ok(Stream::Tls(stream))
            })
            .await;
        }
    }
}
