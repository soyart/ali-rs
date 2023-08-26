use thiserror::Error;

use crate::run::Action;

#[derive(Debug, Error)]
pub enum AliError {
    #[error("no such file")]
    NoSuchFile(std::io::Error, String),

    #[error("no such device")]
    NoSuchDevice(String),

    #[error("bad manifest")]
    BadManifest(String),

    #[error("shell command failed")]
    CmdFailed(Option<std::io::Error>, String),

    #[error("bad cli arguments")]
    BadArgs(String),

    #[error("not implemented")]
    NotImplemented,

    #[error("installation error")]
    InstallError {
        error: Box<AliError>,
        action_failed: Action,
        actions_performed: Vec<Action>,
    },

    #[error("ali-rs bug")]
    AliRsBug(String),
}
