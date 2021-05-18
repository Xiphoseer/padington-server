//! # Server configuration

mod folder;

pub use folder::{Folder, PathValidity};

use color_eyre::Report;
use color_eyre::Result;
use eyre::{eyre, WrapErr};
use serde::{de, Deserialize, Deserializer};
use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::str::FromStr;
use structopt::StructOpt;
use tokio::fs::read_to_string;
use tracing::instrument;
use tungstenite::http::Uri;

use tokio_rustls::rustls::internal::pemfile::{certs, pkcs8_private_keys};
use tokio_rustls::rustls::{Certificate, PrivateKey};

/// The commandline flags for the server
#[derive(Debug, StructOpt)]
pub struct Flags {
    /// Which config file to use
    #[structopt(long = "cfg", short = "c")]
    pub cfg: Option<PathBuf>,
    /// Which port to use (if cfg isn't present)
    #[structopt(long = "port", short = "p")]
    pub port: Option<u16>,
}

/// The type of connection we want
pub enum ConnSetup {
    /// A simple connection (localhost or behind a web-server)
    Basic,
    /// A TLS connection set up within this service
    Tls {
        /// The loaded keys
        keys: Vec<PrivateKey>,
        /// The loaded certificates
        certs: Vec<Certificate>,
    },
}

/// The setup that we are actually using
pub struct Setup {
    /// The address to bind to
    pub addr: String,
    /// The kind of connection we use
    pub conn: ConnSetup,
    /// The folder we use
    pub folder: Folder,
}

impl Flags {
    #[instrument]
    /// Load the configuration from a file
    pub async fn load_cfg(&self) -> Result<Setup, Report> {
        if let Some(cfg) = &self.cfg {
            let cfg_string: String = read_to_string(cfg)
                .await
                .wrap_err("Could not read config file")?;
            let config: Config =
                toml::from_str(&cfg_string).wrap_err("Could not parse config file")?;

            let addr = config.addr;
            if let Some(cfg_tls) = config.tls {
                if cfg_tls.enabled {
                    let certs = cfg_tls
                        .load_certs()
                        .wrap_err("Could not load certificate file")?;
                    let keys = cfg_tls.load_keys().wrap_err("Could not load key file")?;
                    return Ok(Setup {
                        addr: addr.to_string(),
                        conn: ConnSetup::Tls { certs, keys },
                        folder: config.folder,
                    });
                }
            }
            Ok(Setup {
                addr: addr.to_string(),
                conn: ConnSetup::Basic,
                folder: config.folder,
            })
        } else if let Some(port) = self.port {
            Ok(Setup {
                addr: format!("0.0.0.0:{}", port),
                conn: ConnSetup::Basic,
                folder: Folder::default(),
            })
        } else {
            Ok(Setup {
                addr: String::from("127.0.0.1:9002"),
                conn: ConnSetup::Basic,
                folder: Folder::default(),
            })
        }
    }
}

/// The TLS config options
#[derive(Debug, Deserialize)]
pub struct Tls {
    /// Whether the TLS config is actually used
    pub enabled: bool,
    /// Which certificate file to use
    pub cert: PathBuf,
    /// Which key file to use
    pub key: PathBuf,
}

impl Tls {
    #[instrument]
    /// Load the TLS certificates
    pub fn load_certs(&self) -> Result<Vec<Certificate>> {
        let path = &self.cert;
        let file = File::open(path)?;
        certs(&mut BufReader::new(file)).map_err(|()| eyre!("Invalid certificate"))
    }

    #[instrument]
    /// Load the TLS keys
    pub fn load_keys(&self) -> Result<Vec<PrivateKey>> {
        let path = &self.key;
        let file = File::open(path)?;
        pkcs8_private_keys(&mut BufReader::new(file)).map_err(|()| eyre!("Invalid key"))
    }
}

/// A configuration for the system
#[derive(Deserialize)]
pub struct Config {
    /// The address to bind the service to
    #[serde(deserialize_with = "deserialize_from_str")]
    pub addr: Uri,
    /// The TLS options
    pub tls: Option<Tls>,
    /// The folder options
    #[serde(default)]
    pub folder: Folder,
}

// You can use this deserializer for any type that implements FromStr
// and the FromStr::Err implements Display
fn deserialize_from_str<'de, S, D>(deserializer: D) -> Result<S, D::Error>
where
    S: FromStr,      // Required for S::from_str...
    S::Err: Display, // Required for .map_err(de::Error::custom)
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    S::from_str(&s).map_err(de::Error::custom)
}
