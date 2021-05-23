//! # Server for the `padington` protocol
#![warn(missing_docs)]

pub mod channel;
pub mod client;
pub mod command;
pub mod config;
pub mod lobby;
pub mod util;

#[macro_use]
extern crate derive_new;

use crate::{
    client::handle_connection,
    config::{ConnSetup, Flags, Setup},
    lobby::{JoinRequest, LobbyClient, LobbyServer},
};
use color_eyre::{eyre::WrapErr, Report};
use futures_util::future::ready;
use std::{future::Future, io, net::{SocketAddr, ToSocketAddrs}, process};
use structopt::StructOpt;
use tokio::{net::{TcpListener, TcpStream}, sync::mpsc};
use tracing::{error, info, instrument, warn};

#[cfg(feature = "tls")]
use {
    color_eyre::eyre::eyre,
    std::sync::Arc,
    tokio_rustls::{
        rustls::{NoClientAuth, ServerConfig},
        TlsAcceptor,
    },
};

async fn accept_connection(lc: LobbyClient, peer: SocketAddr, stream: ClientStream) {
    if let Err(e) = handle_connection(lc, peer, stream).await {
        error!("Error processing connection: {}", e)
    }
}

#[cfg(not(feature = "tls"))]
type ClientStream = TcpStream;
#[cfg(feature = "tls")]
mod stream;
#[cfg(feature = "tls")]
pub use stream::ClientStream;

async fn wait_for_connections<F, R>(
    listener: TcpListener,
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

async fn signal_handler() -> Result<(), Report> {
    tokio::signal::ctrl_c().await?;
    warn!("ctrl-c received!");
    process::exit(1);
}

#[instrument]
#[tokio::main]
async fn main() -> Result<(), Report> {
    install_tracing();

    let flags: Flags = Flags::from_args();
    let cfg: Setup = flags.load_cfg().await.wrap_err("loading config")?;

    let addr = cfg.addr.as_str().to_socket_addrs().unwrap().next().unwrap();

    let (lobby_sender, lobby_receiver) = mpsc::channel(100);

    tokio::spawn(LobbyServer::new(lobby_receiver, cfg.folder).run());
    tokio::spawn(signal_handler());

    let listener = TcpListener::bind(&addr).await.wrap_err("Can't listen")?;
    info!("Listening on: {}", addr);

    match cfg.conn {
        ConnSetup::Basic => {
            wait_for_connections(listener, lobby_sender, |stream| {
                #[cfg(feature = "tls")]
                let stream = ClientStream::Plain(stream);
                ready(Ok(stream))
            })
            .await;
        }
        #[cfg(feature = "tls")]
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
                Ok(ClientStream::Rustls(Box::new(stream)))
            })
            .await;
        }
    }

    Ok(())
}
