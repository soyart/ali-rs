use crate::cli;
use crate::errors::NayiError;
use crate::manifest::{self, validation, Manifest};

#[derive(Debug)]
pub(super) enum Action {
    CreatePartitionTable {
        device: String,
        table: manifest::PartitionTable,
    },

    CreatePartition {
        device: String,
        number: usize,
        size: usize,
    },

    CreateDmLuks {
        device: String,
    },

    CreateDmLvmPv(String),

    CreateDmLvmVg {
        pv: String,
        vg: String,
    },

    CreateDmLvmLv {
        vg: String,
        lv: String,
    },

    CreateFs {
        device: String,
        fs_type: String,
        mountpoint: String,
    },

    InstallPackages {
        packages: Vec<String>,
    },

    RunCommandsChroot {
        commands: Vec<String>,
    },

    RunCommandsPostInstall {
        commands: Vec<String>,
    },
}

#[derive(Debug)]
pub(super) struct Report {
    pub actions: Vec<Action>,
    pub duration: std::time::Duration,
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

    for dm in manifest.dm.iter() {
        match dm {
            manifest::Dm::Lvm(_) => {
                has_lvm = true;
                break;
            }
            _ => continue,
        }
    }

    if manifest.rootfs.fs_type.as_str() == btrfs {
        has_btrfs = true;
    }

    if !has_btrfs {
        for fs in manifest.filesystems.iter() {
            if fs.fs_type.as_str() == btrfs {
                has_btrfs = true;
                break;
            }
        }
    }

    if has_lvm {
        manifest.pacstraps.insert(lvm2);
    }

    if has_btrfs {
        manifest.pacstraps.insert(btrfs_progs);
    }
}
