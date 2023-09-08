use serde::{Deserialize, Serialize};

use super::action::*;

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
