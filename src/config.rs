use serde::{de, Deserialize, Deserializer};
use std::fmt::Display;
use std::fs::File;
use std::io::{self, BufReader};
use std::path::PathBuf;
use std::str::FromStr;
use structopt::StructOpt;
use tokio::fs::read_to_string;
use tungstenite::http::Uri;

use tokio_rustls::rustls::internal::pemfile::{certs, pkcs8_private_keys};
use tokio_rustls::rustls::{Certificate, PrivateKey};

#[derive(StructOpt)]
pub struct Flags {
    #[structopt(long = "cfg", short = "c")]
    pub cfg: Option<PathBuf>,
}

pub enum ConnSetup {
    Basic,
    Tls {
        keys: Vec<PrivateKey>,
        certs: Vec<Certificate>,
    },
}

pub struct Setup {
    pub addr: String,
    pub conn: ConnSetup,
}

#[derive(Debug)]
pub enum SetupError {
    Io(io::Error),
    Toml(toml::de::Error),
}

impl From<io::Error> for SetupError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<toml::de::Error> for SetupError {
    fn from(err: toml::de::Error) -> Self {
        Self::Toml(err)
    }
}

impl Flags {
    pub async fn load_cfg(&self) -> Result<Setup, SetupError> {
        if let Some(cfg) = &self.cfg {
            let cfg_string: String = read_to_string(cfg).await?;
            let config: Config = toml::from_str(&cfg_string)?;

            let addr = config.addr;
            if let Some(cfg_tls) = config.tls {
                if cfg_tls.enabled {
                    let certs = cfg_tls.load_certs()?;
                    let keys = cfg_tls.load_keys()?;
                    return Ok(Setup {
                        addr: addr.to_string(),
                        conn: ConnSetup::Tls { certs, keys },
                    });
                }
            }
            Ok(Setup {
                addr: addr.to_string(),
                conn: ConnSetup::Basic,
            })
        } else {
            Ok(Setup {
                addr: String::from("127.0.0.1:9002"),
                conn: ConnSetup::Basic,
            })
        }
    }
}

#[derive(Deserialize)]
pub struct Tls {
    pub enabled: bool,
    pub cert: PathBuf,
    pub key: PathBuf,
}

impl Tls {
    pub fn load_certs(&self) -> io::Result<Vec<Certificate>> {
        let path = &self.cert;
        certs(&mut BufReader::new(File::open(path)?))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert"))
    }

    pub fn load_keys(&self) -> io::Result<Vec<PrivateKey>> {
        let path = &self.key;
        pkcs8_private_keys(&mut BufReader::new(File::open(path)?))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))
    }
}

#[derive(Deserialize)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_from_str")]
    pub addr: Uri,
    pub tls: Option<Tls>,
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
