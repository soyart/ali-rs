use thiserror::Error;

use crate::run::apply::Action;

#[derive(Debug, Error)]
pub enum AliError {
    #[error("no such file: {0}")]
    NoSuchFile(std::io::Error, String),

    #[error("no such device: {0}")]
    NoSuchDevice(String),

    #[error("bad manifest: {0}")]
    BadManifest(String),

    #[error("shell command (context: \"{context}\"): {error:?}")]
    CmdFailed {
        error: Option<std::io::Error>,
        context: String,
    },

    #[error("bad cli arguments: {0}")]
    BadArgs(String),

    #[error("not implemented")]
    NotImplemented,

    #[error("installation error")]
    InstallError {
        error: Box<AliError>,
        action_failed: Action,
        actions_performed: Vec<Action>,
    },

    #[error("ali-rs bug: {0}")]
    AliRsBug(String),
}
