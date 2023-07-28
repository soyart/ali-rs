#![feature(fs_try_exists)]
#![feature(exit_status_error)]

mod cli;
mod disks;
mod errors;
mod linux;
mod manifest;
mod utils;

use clap::Parser;

use errors::AyiError;
use manifest::Manifest;
use utils::fs::file_exists;
use utils::shell::in_path;

fn main() -> Result<(), errors::AyiError> {
    let args = cli::Args::parse();
    let manifest_yaml = std::fs::read_to_string(args.manifest).unwrap();
    let manifest = manifest::parse_manifest(&manifest_yaml).expect("failed to parse manifest yaml");

    sanity_check(&manifest)?;

    println!("{:?}", manifest);
    Ok(())
}

fn sanity_check(manifest: &Manifest) -> Result<(), AyiError> {
    // Check disks
    for disk in manifest.disks.iter() {
        if !file_exists(&disk.device) {
            return Err(AyiError::NoSuchDevice(disk.device.to_string()));
        }
    }

    let rootfs_fs = &manifest.rootfs.0.fs_type;
    if !in_path(rootfs_fs) {
        return Err(AyiError::CmdFailed(
            None,
            format!("no such program to create rootfs: {rootfs_fs}"),
        ));
    }

    for archfs in manifest.filesystems.iter() {
        if !in_path(&archfs.fs_type) {
            let device = &archfs.device;
            return Err(AyiError::CmdFailed(
                None,
                format!("no such program to create filesystem for device {device}: {rootfs_fs}"),
            ));
        }
    }

    Ok(())
}
