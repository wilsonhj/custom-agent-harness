//! Identifier newtypes. Core never generates identifiers; they arrive as data.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(
    /// Identifies a task.
    TaskId
);
string_id!(
    /// Identifies a command.
    CommandId
);
string_id!(
    /// Identifies an effect (a unit of external work).
    EffectId
);
string_id!(
    /// Identifies an event.
    EventId
);
string_id!(
    /// Deduplicates effects across restarts.
    IdempotencyKey
);

/// A malformed hex identifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind} must be {len} lowercase hex characters, got {got:?}")]
pub struct IdError {
    pub kind: &'static str,
    pub len: usize,
    pub got: String,
}

fn check_hex(kind: &'static str, len: usize, s: &str) -> Result<(), IdError> {
    let ok = s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if ok {
        Ok(())
    } else {
        Err(IdError {
            kind,
            len,
            got: s.chars().take(80).collect(),
        })
    }
}

macro_rules! hex_id {
    ($(#[$doc:meta])* $name:ident, $len:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub const LEN: usize = $len;

            /// Accepts exactly `LEN` lowercase hex characters.
            pub fn parse(s: &str) -> Result<Self, IdError> {
                check_hex(stringify!($name), $len, s)?;
                Ok(Self(s.to_owned()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;
            fn try_from(s: String) -> Result<Self, IdError> {
                check_hex(stringify!($name), $len, &s)?;
                Ok(Self(s))
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

hex_id!(
    /// Content address: lowercase hex SHA-256 (64 chars).
    ArtifactId,
    64
);
hex_id!(
    /// A Git commit: 40 lowercase hex chars (SHA-1 object format).
    CommitSha,
    40
);

impl ArtifactId {
    /// The SHA-256 of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let digest = Sha256::digest(bytes);
        let mut s = String::with_capacity(64);
        for b in digest.iter() {
            s.push(char::from(HEX[usize::from(b >> 4)]));
            s.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
        Self(s)
    }
}

/// Position in the event log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Seq(pub u64);

/// Unix time in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub i64);
