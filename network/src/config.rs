use std::{error::Error, fmt::Display, str::FromStr};

use clap::Subcommand;
use libp2p::{
    gossipsub::{IdentTopic, ValidationMode},
    Multiaddr, PeerId,
};
use serde::{Deserialize, Serialize};

#[derive(Subcommand)]
pub enum NetworkMode {
    Dial {
        /// PeerID's to dial
        #[clap(value_parser, short, long, value_name = "id")]
        id: Vec<PeerId>,
        /// Topics to subscribe to
        #[clap(value_parser, short, long, value_name = "topic")]
        topic: Vec<String>,
    },

    Test {
        /// PeerId of the test peer to dial
        #[clap(value_parser, short, long, value_name = "id")]
        id: PeerId,
        /// Address of the peer to dial
        #[clap(value_parser, short, long, value_name = "addr")]
        addr: Multiaddr,
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

/// Imoteph network configuration
pub struct Config {
    /// Topics to subscribe to
    pub topics: Vec<IdentTopic>,
    /// Peer Ids to connect
    pub peers: Vec<PeerId>,
    /// Validation mode for gossipsub
    pub validation: GossipSubValidationMode,
    /// Buffer size
    pub buffer: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct ConfigJson {
    /// Topics to subscribe to
    pub topics: Vec<String>,
    /// Peer Ids to connect
    pub peers: Vec<String>,
    /// Validation mode for gossipsub
    pub validation: GossipSubValidationMode,
    /// Buffer size
    pub buffer: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub enum GossipSubValidationMode {
    /// Gossipsub strict mode
    Strict,
    /// Gossipsub permissive mode
    Permissive,
    /// Gossipsub Anonymous mode
    Anonymous,
    None,
}

impl Into<ValidationMode> for GossipSubValidationMode {
    fn into(self) -> ValidationMode {
        match self {
            Self::Strict => ValidationMode::Strict,
            Self::Permissive => ValidationMode::Permissive,
            Self::Anonymous => ValidationMode::Anonymous,
            Self::None => ValidationMode::None,
        }
    }
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
            "permissive" => Ok(Self::Permissive),
            "anonymous" => Ok(Self::Permissive),
            "none" => Ok(Self::None),
            _ => Err(ValidationParseErr),
        }
    }
}
