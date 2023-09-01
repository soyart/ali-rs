use crate::ali;
use crate::errors::AliError;
use crate::linux::fdisk;
use crate::run::apply::Action;

pub fn apply_disks(disks: &[ali::ManifestDisk]) -> Result<Vec<Action>, AliError> {
    let mut actions = Vec::new();
    for disk in disks.iter() {
        let action_prepare = Action::PrepareDisk {
            deviec: disk.device.clone(),
        };

        match apply_disk(disk) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    action_failed: Box::new(action_prepare),
                    actions_performed: actions,
                });
            }
            Ok(disk_actions) => {
                actions.extend(disk_actions);
                actions.push(action_prepare);
            }
        }
    }

    Ok(actions)
}

pub fn apply_disk(disk: &ali::ManifestDisk) -> Result<Vec<Action>, AliError> {
    let cmd_create_table = fdisk::create_table_cmd(&disk.table);
    fdisk::run_fdisk_cmd(&disk.device, &cmd_create_table)?;

    let mut actions = vec![Action::CreatePartitionTable {
        device: disk.device.clone(),
        table: disk.table.clone(),
    }];

    // Actions:
    // 1. Create partition
    // 2. Set partition type
    for (n, part) in disk.partitions.iter().enumerate() {
        let partition_number = n + 1;
        let cmd_create_part = fdisk::create_partition_cmd(&disk.table, partition_number, part);

        let action_create_partition = Action::CreatePartition {
            device: disk.device.clone(),
            number: partition_number,
            size: part.size.clone().unwrap_or("100%".into()),
        };

        if let Err(err) = fdisk::run_fdisk_cmd(&disk.device, &cmd_create_part) {
            return Err(AliError::InstallError {
                error: Box::new(err),
                action_failed: Box::new(action_create_partition),
                actions_performed: actions,
            });
        }

        actions.push(action_create_partition);

        let action_set_type = Action::SetPartitionType {
            device: disk.device.clone(),
            number: partition_number,
            partition_type: part.part_type.clone(),
        };

        let cmd_set_type = fdisk::set_partition_type_cmd(partition_number, part);
        let result_set_type = fdisk::run_fdisk_cmd(&disk.device, &cmd_set_type);

        if let Err(err) = result_set_type {
            return Err(AliError::InstallError {
                error: Box::new(err),
                action_failed: Box::new(action_set_type),
                actions_performed: actions,
            });
        }

        actions.push(action_set_type);
    }

    Ok(actions)
}
