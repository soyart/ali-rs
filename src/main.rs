#![feature(fs_try_exists)]
#![feature(exit_status_error)]
#![feature(map_try_insert)]

mod cli;
mod disks;
mod errors;
mod linux;
mod manifest;
mod utils;

use clap::Parser;

use manifest::{validation, Manifest};

fn main() -> Result<(), errors::AyiError> {
    let args = cli::Args::parse();
    let manifest_yaml = std::fs::read_to_string(args.manifest).unwrap();

    let mut manifest = manifest::parse(&manifest_yaml).expect("failed to parse manifest yaml");
    validation::validate(&manifest)?;

    update_manifest(&mut manifest);

    println!("{:?}", manifest);
    Ok(())
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
        if !manifest.pacstraps.contains(&lvm2) {
            manifest.pacstraps.push(lvm2);
        }
    }

    if has_btrfs {
        if !manifest.pacstraps.contains(&btrfs_progs) {
            manifest.pacstraps.push(btrfs_progs);
        }
    }
}
