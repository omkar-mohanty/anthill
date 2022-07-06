use std::{error::Error, fmt::Display, path::PathBuf, str::FromStr};

use clap::{Parser, Subcommand};
use libp2p::{gossipsub::IdentTopic, PeerId};
use serde::{Deserialize, Serialize};

#[derive(Subcommand)]
pub enum NetworkMode {
    Ipfs {
        /// Path for IPFS config file
        #[clap(short, long, value_name = "path")]
        path: Option<PathBuf>,
    },
    Dial {
        /// PeerID's to dial
        id: Vec<PeerId>,
        /// Topics to subscribe to
        topic: ConfigTopic,
    },
}

pub struct ConfigTopic {
    name: String,
}

impl FromStr for ConfigTopic {
    type Err = ValidationParseErr;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(name) = String::from_str(s) {
            Ok(Self { name })
        } else {
            Err(ValidationParseErr)
        }
    }
}

impl Into<IdentTopic> for ConfigTopic {
    fn into(self) -> IdentTopic {
        IdentTopic::new(self.name)
    }
}

#[derive(Serialize, Deserialize, Parser)]
pub struct Config {
    /// Topics to subscribe to
    topics: Vec<String>,
    /// Peer Ids to connect
    peers: Vec<String>,
    /// Validation mode for gossipsub
    validation: GossipSubValidationMode,
}

#[derive(Serialize, Deserialize)]
pub enum GossipSubValidationMode {
    Strict,
    Signed,
    Author,
}

#[derive(Debug, Clone)]
pub struct ValidationParseErr;

impl Error for ValidationParseErr {}

impl Display for ValidationParseErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "could not parse validation mode")
    }
}
impl FromStr for GossipSubValidationMode {
    type Err = ValidationParseErr;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "strict" => Ok(Self::Strict),
            "Signed" => Ok(Self::Signed),
            "author" => Ok(Self::Author),
            _ => Err(ValidationParseErr),
        }
    }
}
