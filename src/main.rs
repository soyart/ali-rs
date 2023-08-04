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
    let validation_result = validation::validate(&manifest)?;

    update_manifest(&mut manifest, validation_result);

    println!("{:?}", manifest);
    Ok(())
}

// Update manifest to suit the manifest
fn update_manifest(manifest: &mut Manifest, validation_result: validation::ValidationResult) {
    if validation_result.has_lvm {
        if manifest.pacstraps.contains(&"lvm2".to_string()) {
            manifest.pacstraps.push("lvm2".to_string());
        }
    }

    if validation_result.has_btrfs {
        if manifest.pacstraps.contains(&"btrfs-progs".to_string()) {
            manifest.pacstraps.push("btrfs-progs".to_string());
        }
    }
}
