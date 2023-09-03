use serde_json::json;
use thiserror::Error;

use crate::entity::report;

#[derive(Debug, Error)]
pub enum AliError {
    #[error("no such file {1}: {0}")]
    NoSuchFile(std::io::Error, String),

    #[error("file error {1}: {0}")]
    FileError(std::io::Error, String),

    #[error("no such device: {0}")]
    NoSuchDevice(String),

    #[error("bad manifest: {0}")]
    BadManifest(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("shell command (context: \"{context}\"), embeddedError: {error:?}")]
    CmdFailed {
        error: Option<std::io::Error>,
        context: String,
    },

    #[error("bad cli arguments: {0}")]
    BadArgs(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),

    #[error("manifest application error: {error}")]
    ApplyError {
        error: Box<AliError>,
        action_failed: Box<report::Action>,
        actions_performed: Vec<report::Action>,
    },

    #[error("installation error")]
    InstallError {
        error: Box<AliError>,
        stages_performed: Box<report::Stages>,
    },

    #[error("ali-rs bug: {0}")]
    AliRsBug(String),
}

impl AliError {
    pub fn to_json_string(&self) -> String {
        let json_value = match self {
            Self::InstallError {
                error,
                stages_performed,
            } => {
                json!({
                    "error": error.to_json_string(),
                    "stagesPerformed": stages_performed,
                })
            }
            Self::ApplyError {
                error,
                action_failed,
                actions_performed,
            } => {
                json!({
                    "error": error.to_string(),
                    "actionFailed": action_failed,
                    "actionsPerformed": actions_performed,
                })
            }
            _ => {
                json!({
                    "error": self.to_string(),
                })
            }
        };

        json_value.to_string()
    }
}

#[test]
fn test_json_error() {
    use std::collections::HashSet;

    use crate::ali::PartitionTable;
    use crate::entity::report::*;

    let actions_mountpoints = vec![
        Action::Mountpoints(ActionMountpoints::CreatePartitionTable {
            device: "/dev/sda".to_string(),
            table: PartitionTable::Gpt,
        }),
        Action::Mountpoints(ActionMountpoints::CreatePartition {
            device: "/dev/sda".to_string(),
            number: 1,
            size: "500M".into(),
        }),
        Action::Mountpoints(ActionMountpoints::CreatePartition {
            device: "/dev/sda".to_string(),
            number: 2,
            size: "1G".into(),
        }),
        Action::Mountpoints(ActionMountpoints::ApplyDisk {
            device: "/dev/sda".to_string(),
        }),
        Action::Mountpoints(ActionMountpoints::CreatePartitionTable {
            device: "/dev/sdb".to_string(),
            table: PartitionTable::Gpt,
        }),
        Action::Mountpoints(ActionMountpoints::CreatePartition {
            device: "/dev/sdb".to_string(),
            number: 1,
            size: "3G".into(),
        }),
        Action::Mountpoints(ActionMountpoints::ApplyDisk {
            device: "/dev/sdb".to_string(),
        }),
        Action::Mountpoints(ActionMountpoints::ApplyDisks),
        Action::Mountpoints(ActionMountpoints::MkdirRootFs),
        Action::Mountpoints(ActionMountpoints::ApplyFilesystems),
    ];

    let actions_bootstrap = vec![Action::Bootstrap(ActionBootstrap::InstallBase)];

    // Failed during bootstrap user packages
    let err_pkg = AliError::ApplyError {
        error: Box::new(AliError::CmdFailed {
            error: None,
            context: "no such command foobar".to_string(),
        }),
        action_failed: Box::new(Action::Bootstrap(ActionBootstrap::InstallPackages {
            packages: HashSet::from(["badpkg".to_string()]),
        })),
        actions_performed: actions_bootstrap,
    };

    println!("ApplyError:");
    println!("{}", err_pkg.to_json_string());

    let err_install = AliError::InstallError {
        error: Box::new(err_pkg),
        stages_performed: Box::new(actions_mountpoints.into()),
    };

    println!("InstallError:");
    println!("{}", err_install.to_json_string());
}
