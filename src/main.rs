pub mod channel;
pub mod client;
pub mod command;
pub mod config;
pub mod lobby;
pub mod util;

#[macro_use]
extern crate derive_new;

use crate::client::handle_connection;
use crate::config::{ConnSetup, Flags, Setup};
use crate::lobby::{JoinRequest, LobbyClient, LobbyServer};
use color_eyre::Report;
use eyre::{eyre, WrapErr};
use futures_util::future::ready;
//use log::*;
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
use tracing::{error, info, instrument};

async fn accept_connection(lc: LobbyClient, peer: SocketAddr, stream: ClientStream) {
    if let Err(e) = handle_connection(lc, peer, stream).await {
        match e {
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

#[cfg(feature = "capture-spantrace")]
fn install_tracing() {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let fmt_layer = fmt::layer(); //.with_target(false);
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| {
            EnvFilter::try_new(
                #[cfg(debug_assertions)]
                "warn,padington_server=debug",
                #[cfg(not(debug_assertions))]
                "warn,padington_server=info",
            )
        })
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(ErrorLayer::default())
        .init();
}

#[instrument]
#[tokio::main]
async fn main() -> Result<(), Report> {
    #[cfg(feature = "capture-spantrace")]
    install_tracing();

    let flags: Flags = Flags::from_args();
    let cfg: Setup = flags.load_cfg().await.wrap_err("loading config")?;

    let addr = cfg.addr.as_str().to_socket_addrs().unwrap().next().unwrap();

    let (lobby_sender, lobby_receiver) = mpsc::channel(100);

    tokio::spawn(LobbyServer::new(lobby_receiver, cfg.folder).run());

    let listener = TcpListener::bind(&addr).await.wrap_err("Can't listen")?;
    info!("Listening on: {}", addr);

    match cfg.conn {
        ConnSetup::Basic => {
            wait_for_connections(listener, lobby_sender, |stream| {
                ready(Ok(Stream::Plain(stream)))
            })
            .await;
        }
        ConnSetup::Tls { certs, mut keys } => {
            info!("Setting up TLS ...");
            let mut config = ServerConfig::new(NoClientAuth::new());
            let key = keys
                .drain(..1)
                .next()
                .ok_or_else(|| eyre!("Key-File contains no keys"))?;
            config
                .set_single_cert(certs, key)
                .wrap_err("setting certificate")?;
            let acceptor = TlsAcceptor::from(Arc::new(config));
            wait_for_connections(listener, lobby_sender, |stream: TcpStream| async {
                let acceptor = acceptor.clone();
                let stream = acceptor.accept(stream).await?;
                Ok(Stream::Tls(stream))
            })
            .await;
        }
    }

    Ok(())
}
