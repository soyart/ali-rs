use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use super::action::*;

/// ALI stages
#[derive(Debug, Clone, PartialEq, Eq, Hash, ValueEnum)]

pub enum Stage {
    #[value(alias = "stage-mountpoints")]
    Mountpoints,

    #[value(alias = "stage-bootstrap")]
    Bootstrap,

    #[value(alias = "routine", alias = "stage-routines")]
    Routines,

    #[value(
        alias = "chroot_ali",
        alias = "stage-chrootali",
        alias = "stage-chroot_ali"
    )]
    ChrootAli,

    #[value(
        alias = "chroot_user",
        alias = "stage-chrootuser",
        alias = "stage-chroot_user"
    )]
    ChrootUser,

    #[value(
        alias = "postinstalluser",
        alias = "postinstall_user",
        alias = "stage-postinstall",
        alias = "stage-postinstalluser",
        alias = "stage-postinstall_user"
    )]
    PostInstallUser,
}

pub const STAGES: [Stage; 6] = [
    Stage::Mountpoints,
    Stage::Bootstrap,
    Stage::Routines,
    Stage::ChrootAli,
    Stage::ChrootUser,
    Stage::PostInstallUser,
];

/// StageActions groups closely related actions together
/// and can be used in error or success reports.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct StageActions {
    #[serde(rename = "stage-mountpoints")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mountpoints: Vec<ActionMountpoints>,

    #[serde(rename = "stage-bootstrap")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bootstrap: Vec<ActionBootstrap>,

    #[serde(rename = "stage-routines")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub routines: Vec<ActionRoutine>,

    #[serde(rename = "stage-chroot_ali")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chroot_ali: Vec<ActionChrootAli>,

    #[serde(rename = "stage-chroot_user")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chroot_user: Vec<ActionChrootUser>,

    #[serde(rename = "stage-postinstall_user")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub postinstall_user: Vec<ActionPostInstallUser>,
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mountpoints => write!(f, "stage-mountpoints"),
            Self::Bootstrap => write!(f, "stage-bootstrap"),
            Self::Routines => write!(f, "stage-routines"),
            Self::ChrootAli => write!(f, "stage-chroot_ali"),
            Self::ChrootUser => write!(f, "stage-chroot_user"),
            Self::PostInstallUser => write!(f, "stage-postinstall_user"),
        }
    }
}

impl From<Vec<Action>> for StageActions {
    fn from(value: Vec<Action>) -> Self {
        let mut s = Self::default();

        for v in value {
            match v {
                Action::Mountpoints(action) => s.mountpoints.push(action),
                Action::Bootstrap(action) => s.bootstrap.push(action),
                Action::Routines(action) => s.routines.push(action),
                Action::ChrootAli(action) => s.chroot_ali.push(action),
                Action::ChrootUser(action) => s.chroot_user.push(action),
                Action::UserPostInstall(action) => s.postinstall_user.push(action),
            }
        }

        s
    }
}
