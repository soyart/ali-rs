use std::collections::HashSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cli;
use crate::errors::NayiError;
use crate::manifest::{self, validation, Manifest};

#[derive(Debug)]
pub(super) struct Report {
    pub actions: Vec<Action>,
    pub duration: Duration,
}

impl Report {
    pub(super) fn to_json(&self) -> serde_json::Value {
        json!({
            "actions": self.actions,
            "elaspedTime": self.duration,
        })
    }

    pub(super) fn to_json_string(&self) -> String {
        self.to_json().to_string()
    }
}

pub(super) fn run(args: cli::Args) -> Result<Report, NayiError> {
    let start = std::time::Instant::now();

    let manifest_yaml = std::fs::read_to_string(&args.manifest)
        .map_err(|err| NayiError::NoSuchFile(err, args.manifest))?;

    // manifest is mutable because we might have to
    // help add packages such as lvm2 and btrfs-progs
    let mut manifest = Manifest::from_yaml(&manifest_yaml)?;

    validation::validate(&manifest)?;
    update_manifest(&mut manifest);

    // TODO: now nayi-rs just prints valid manifest to stdout
    println!("{:?}", manifest);

    Ok(Report {
        actions: vec![],
        duration: start.elapsed(),
    })
}

// Update manifest to suit the manifest
fn update_manifest(manifest: &mut Manifest) {
    let (lvm2, btrfs, btrfs_progs) = (
        "lvm2".to_string(),
        "btrfs".to_string(),
        "btrfs-progs".to_string(),
    );
    let (mut has_lvm, mut has_btrfs) = (false, false);

    if let Some(dms) = &manifest.dm {
        for dm in dms {
            match dm {
                manifest::Dm::Lvm(_) => {
                    has_lvm = true;
                    break;
                }
                _ => continue,
            }
        }
    }

    if manifest.rootfs.fs_type.as_str() == btrfs {
        has_btrfs = true;
    }

    if !has_btrfs {
        if let Some(filesystems) = &manifest.filesystems {
            for fs in filesystems {
                if fs.fs_type.as_str() == btrfs {
                    has_btrfs = true;
                    break;
                }
            }
        }
    }

    if has_lvm {
        match manifest.pacstraps {
            Some(ref mut pacstraps) => {
                pacstraps.insert(lvm2.clone());
            }
            None => {
                let pacstraps = HashSet::from([lvm2.clone()]);
                manifest.pacstraps = Some(pacstraps)
            }
        }
    }

    if has_btrfs {
        match manifest.pacstraps {
            Some(ref mut pacstraps) => {
                pacstraps.insert(lvm2);
            }
            None => {
                let pacstraps = HashSet::from([btrfs_progs.clone()]);
                manifest.pacstraps = Some(pacstraps)
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) enum Action {
    #[serde(rename = "createPartitionTable")]
    CreatePartitionTable {
        device: String,
        table: manifest::PartitionTable,
    },

    #[serde(rename = "createPartition")]
    CreatePartition {
        device: String,
        number: usize,
        size: String,
    },

    #[serde(rename = "createDmLuks")]
    CreateDmLuks { device: String },

    #[serde(rename = "createLvmPv")]
    CreateDmLvmPv(String),

    #[serde(rename = "createLvmVg")]
    CreateDmLvmVg { pv: String, vg: String },

    #[serde(rename = "createLvmLv")]
    CreateDmLvmLv { vg: String, lv: String },

    #[serde(rename = "createFilesystem")]
    CreateFs {
        device: String,
        fs_type: String,
        mountpoint: String,
    },

    #[serde(rename = "installPackages")]
    InstallPackages { packages: Vec<String> },

    #[serde(rename = "commandsChroot")]
    RunCommandsChroot { commands: Vec<String> },

    #[serde(rename = "commandsPostInstall")]
    RunCommandsPostInstall { commands: Vec<String> },
}

#[ignore = "Ignored because just dummy print JSON"]
#[test]
// Dummy function to see JSON result
fn test_json_actions() {
    use manifest::PartitionTable;

    let actions = vec![
        Action::CreatePartitionTable {
            device: "/dev/sda".into(),
            table: PartitionTable::Gpt,
        },
        Action::CreatePartition {
            device: "/dev/sda1".into(),
            number: 1,
            size: "8G".into(),
        },
        Action::CreateFs {
            device: "/dev/sda1".into(),
            fs_type: "btrfs".into(),
            mountpoint: "/".into(),
        },
    ];

    let report = Report {
        actions,
        duration: Duration::from_secs(20),
    };

    println!("{}", report.to_json_string());
}
