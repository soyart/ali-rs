use std::collections::HashSet;

use crate::ali::{
    apply,
    validation,
    Dm,
    Manifest,
};
use crate::cli;
use crate::types::report::Report;
use crate::types::stage;
use crate::errors::AliError;

pub(super) fn run(
    manifest_file: &str,
    install_location: &str,
    args: cli::ArgsApply,
) -> Result<Report, AliError> {
    let start = std::time::Instant::now();

    let mut skip_stages: HashSet<stage::Stage> =
        HashSet::from_iter(args.skip_stages);
    if let Some(stages) = args.stages {
        for explicit_stage in stages.iter() {
            if skip_stages.contains(explicit_stage) {
                return Err(AliError::BadArgs(format!(
                    "stage {explicit_stage} is ambiguous"
                )));
            }
        }

        let mut all_stages: HashSet<stage::Stage> =
            HashSet::from(stage::STAGES);
        for skip in skip_stages.iter() {
            all_stages.remove(skip);
        }
        skip_stages = HashSet::new();

        let explicit_stages: HashSet<stage::Stage> = HashSet::from_iter(stages);
        let diff: HashSet<_> =
            all_stages.difference(&explicit_stages).collect();
        for d in diff {
            skip_stages.insert(d.to_owned());
        }
    }

    let manifest_yaml = std::fs::read_to_string(manifest_file)
        .map_err(|err| AliError::NoSuchFile(err, manifest_file.to_string()))?;

    // manifest is mutable because we might have to
    // help add packages such as lvm2 and btrfs-progs
    let mut manifest = Manifest::from_yaml(&manifest_yaml)?;

    if !args.no_validate {
        validation::validate(&manifest, install_location, args.overwrite)?;
    }

    // Update manifest in some cases
    update_manifest(&mut manifest);

    // Apply manifest to location
    let location = super::install_location();
    let stages_applied =
        apply::apply_manifest(&manifest, &location, skip_stages)?;

    Ok(Report {
        location,
        summary: stages_applied,
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
    if let (false, Some(filesystems)) = (has_btrfs, &manifest.filesystems) {
        for fs in filesystems {
            if fs.fs_type.as_str() == btrfs {
                has_btrfs = true;

                break;
            }
        }
    }

    // Update manifest.pacstraps if any of the filesystems is Btrfs
    match (has_btrfs, manifest.pacstraps.as_mut()) {
        (true, Some(ref mut pacstraps)) => {
            pacstraps.insert(btrfs_progs);
        }
        (true, None) => {
            manifest.pacstraps = Some(HashSet::from([btrfs_progs]));
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
            pacstraps.insert(lvm2);
        }

        (true, None) => {
            manifest.pacstraps = Some(HashSet::from([lvm2]));
        }
        _ => {}
    }
}
