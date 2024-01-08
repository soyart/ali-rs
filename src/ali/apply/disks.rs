use crate::ali;
use crate::types::action::ActionMountpoints;
use crate::errors::AliError;
use crate::linux::fdisk;

use super::map_err::map_err_mountpoints;

pub fn apply_disks(
    disks: &[ali::ManifestDisk],
) -> Result<Vec<ActionMountpoints>, AliError> {
    let mut actions: Vec<ActionMountpoints> = Vec::new();

    for disk in disks.iter() {
        let action_apply_disk = ActionMountpoints::ApplyDisk {
            device: disk.device.clone(),
        };

        match apply_disk(disk) {
            Err(err) => {
                return Err(map_err_mountpoints(
                    err,
                    action_apply_disk,
                    actions,
                ));
            }
            Ok(disk_actions) => {
                actions.extend(disk_actions);
                actions.push(action_apply_disk);
            }
        }
    }

    actions.push(ActionMountpoints::ApplyDisks);

    Ok(actions)
}

pub fn apply_disk(
    disk: &ali::ManifestDisk,
) -> Result<Vec<ActionMountpoints>, AliError> {
    let mut actions = Vec::new();

    let action_create_table = ActionMountpoints::CreatePartitionTable {
        device: disk.device.clone(),
        table: disk.table.clone(),
    };
    let cmd_create_table = fdisk::create_table_cmd(&disk.table);
    if let Err(err) = fdisk::run_fdisk_cmd(&disk.device, &cmd_create_table) {
        return Err(map_err_mountpoints(err, action_create_table, actions));
    }

    actions.push(action_create_table);

    // Actions:
    // 1. Create partition
    // 2. Set partition type
    for (n, part) in disk.partitions.iter().enumerate() {
        let partition_number = n + 1;
        let cmd_create_part =
            fdisk::create_partition_cmd(&disk.table, partition_number, part);

        let action_create_partition = ActionMountpoints::CreatePartition {
            device: disk.device.clone(),
            number: partition_number,
            size: part.size.clone().unwrap_or("100%".into()),
        };

        if let Err(err) = fdisk::run_fdisk_cmd(&disk.device, &cmd_create_part) {
            return Err(map_err_mountpoints(
                err,
                action_create_partition,
                actions,
            ));
        }

        actions.push(action_create_partition);

        let action_set_part_type = ActionMountpoints::SetPartitionType {
            device: disk.device.clone(),
            number: partition_number,
            partition_type: part.part_type.clone(),
        };

        let cmd_set_type =
            fdisk::set_partition_type_cmd(partition_number, part);
        let result_set_type = fdisk::run_fdisk_cmd(&disk.device, &cmd_set_type);

        if let Err(err) = result_set_type {
            return Err(map_err_mountpoints(
                err,
                action_set_part_type,
                actions,
            ));
        }

        actions.push(action_set_part_type);
    }

    Ok(actions)
}
