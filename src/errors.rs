use thiserror::Error;

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

    #[error("ali-rs bug")]
    AliRsBug(String),
}
