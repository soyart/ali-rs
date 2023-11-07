use std::collections::{
    HashMap,
    LinkedList,
};

use crate::ali::ManifestDisk;
use crate::entity::blockdev::*;
use crate::entity::parse_human_bytes;
use crate::errors::AliError;
use crate::utils::fs::file_exists;

pub(crate) fn collect_valids(
    disks: &[ManifestDisk],
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_fs_ready_devs: &HashMap<String, BlockDevType>,
    valids: &mut BlockDevPaths,
) -> Result<(), AliError> {
    for disk in disks {
        if !file_exists(&disk.device) {
            return Err(AliError::BadManifest(format!(
                "no such disk device: {}",
                disk.device
            )));
        }
        let partition_prefix: String = {
            if disk.device.contains("nvme") || disk.device.contains("mmcblk") {
                format!("{}p", disk.device)
            } else {
                disk.device.clone()
            }
        };

        // Find if this disk has any used partitions
        // A GPT table can hold a maximum of 128 partitions
        for i in 1_u8..=128 {
            let partition_name = format!("{partition_prefix}{i}");
            if sys_fs_devs.contains_key(&partition_name) {
                let fs = sys_fs_devs.get(&partition_name).unwrap();
                return Err(AliError::BadManifest(format!(
                    "disk {} already in use on {partition_name} as {fs}",
                    disk.device
                )));
            }
        }

        // Base disk
        let base = LinkedList::from([BlockDev {
            device: disk.device.clone(),
            device_type: TYPE_DISK,
        }]);

        // Check if this partition is already in use
        let msg = "partition validation failed";

        let l = disk.partitions.len();
        for (i, part) in disk.partitions.iter().enumerate() {
            let partition_name = format!("{partition_prefix}{}", i + 1);

            // If multiple partitions are to be created on this disk,
            // only the last partition could be unsized
            if i != l - 1 && l != 1 && part.size.is_none() {
                return Err(AliError::BadManifest(format!(
                        "unsized partition {partition_name} must be the last partition"
                    )));
            }

            if sys_fs_ready_devs.get(&partition_name).is_some() {
                return Err(AliError::BadManifest(format!(
                        "{msg}: partition {partition_name} already exists on system"
                    )));
            }

            if let Some(existing_fs) = sys_fs_devs.get(&partition_name) {
                return Err(AliError::BadManifest(format!(
                        "{msg}: partition {partition_name} is already used as {existing_fs}"
                    )));
            }

            if let Some(ref size) = part.size {
                if let Err(err) = parse_human_bytes(size) {
                    return Err(AliError::BadManifest(format!(
                        "bad partition size {size}: {err}"
                    )));
                }
            }

            let mut partition = base.clone();
            partition.push_back(BlockDev {
                device: partition_name,
                device_type: TYPE_PART,
            });

            valids.push(partition);
        }
    }

    Ok(())
}
