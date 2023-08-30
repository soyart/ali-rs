use std::collections::HashSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cli;
use crate::errors::AliError;
use crate::manifest::{self, validation, Dm, Manifest};

#[derive(Debug)]
pub struct Report {
    pub actions: Vec<Action>,
    pub duration: Duration,
}

impl Report {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "actions": self.actions,
            "elaspedTime": self.duration,
        })
    }

    pub fn to_json_string(&self) -> String {
        self.to_json().to_string()
    }
}

pub(super) fn run(manifest_file: &str, args: cli::ArgsApply) -> Result<Report, AliError> {
    let start = std::time::Instant::now();

    let manifest_yaml = std::fs::read_to_string(manifest_file)
        .map_err(|err| AliError::NoSuchFile(err, manifest_file.to_string()))?;

    // manifest is mutable because we might have to
    // help add packages such as lvm2 and btrfs-progs
    let mut manifest = Manifest::from_yaml(&manifest_yaml)?;

    if !args.no_validate {
        validation::validate(&manifest, args.overwrite)?;
    }

    update_manifest(&mut manifest);

    // TODO: ali-rs just prints valid manifest to stdout
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

    // See if root is on Btrfs
    if manifest.rootfs.fs_type.as_str() == btrfs {
        has_btrfs = true;
    }

    // See if other FS is Btrfs
    match (has_btrfs, &manifest.filesystems) {
        (false, Some(filesystems)) => {
            for fs in filesystems {
                if fs.fs_type.as_str() == btrfs {
                    has_btrfs = true;
                    break;
                }
            }
        }
        _ => {}
    }

    // Update manifest.pacstraps if any of the filesystems is Btrfs
    match (has_btrfs, manifest.pacstraps.as_mut()) {
        (true, Some(ref mut pacstraps)) => {
            pacstraps.insert(btrfs_progs.clone());
        }
        (true, None) => {
            manifest.pacstraps = Some(HashSet::from([btrfs_progs.clone()]));
        }
        _ => {}
    }

    // Find a manifest LVM device
    if let Some(ref dms) = manifest.device_mappers {
        for dm in dms {
            match dm {
                Dm::Lvm(_) => {
                    has_lvm = true;
                    break;
                }
                _ => continue,
            }
        }
    }

    // Update manifest.pacstraps if we have LVMs in manifest
    match (has_lvm, manifest.pacstraps.as_mut()) {
        (true, Some(ref mut pacstraps)) => {
            pacstraps.insert(lvm2.clone());
        }
        (true, None) => {
            manifest.pacstraps = Some(HashSet::from([lvm2.clone()]));
        }
        _ => {}
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Action {
    #[serde(rename = "prepareDisk")]
    PrepareDisk { deviec: String },

    #[serde(rename = "prepareDm")]
    PrepareDm,

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

    #[serde(rename = "setParitionType")]
    SetPartitionType {
        device: String,
        number: usize,
        partition_type: String,
    },

    #[serde(rename = "createDmLuks")]
    CreateDmLuks { device: String },

    #[serde(rename = "createLvmPv")]
    CreateDmLvmPv(String),

    #[serde(rename = "createLvmVg")]
    CreateDmLvmVg { pvs: Vec<String>, vg: String },

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
