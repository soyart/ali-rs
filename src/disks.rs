use crate::errors::NayiError;
use crate::linux;
use crate::manifest;

pub fn do_disks(disks: &[manifest::ManifestDisk]) -> Result<(), NayiError> {
    for disk in disks.iter() {
        do_disk(disk)?;
    }

    Ok(())
}

fn do_disk(disk: &manifest::ManifestDisk) -> Result<(), NayiError> {
    let create_table_cmd = linux::fdisk::create_table_cmd(&disk.device, &disk.table);
    linux::fdisk::run_fdisk_cmd(&disk.device, &create_table_cmd)?;

    for (n, part) in disk.partitions.iter().enumerate() {
        let create_part_cmd = linux::fdisk::create_partition_cmd(&disk.table, n + 1, part);

        linux::fdisk::run_fdisk_cmd(&disk.device, &create_part_cmd)?;
    }

    Ok(())
}
