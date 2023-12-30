use std::collections::{
    HashMap,
    HashSet,
};

use super::is_fs_ready;
use crate::entity::blockdev::{
    BlockDevPaths,
    BlockDevType,
};
use crate::errors::AliError;

// Collects fs-ready devices on the system into fs_ready_devs
pub(super) fn collect_fs_ready_devs(
    sys_fs_ready_devs: &mut HashMap<String, BlockDevType>,
    fs_ready_devs: &mut HashSet<String>,
) -> Result<(), AliError> {
    const MSG: &str = "device validation failed";
    for (dev, dev_type) in sys_fs_ready_devs {
        if !is_fs_ready(dev_type) {
            return Err(AliError::AliRsBug(format!(
                "{MSG}: device {dev} ({dev_type}) cannot be used as base for filesystems"
            )));
        }

        if fs_ready_devs.insert(dev.clone()) {
            continue;
        }

        return Err(AliError::AliRsBug(format!(
            "{MSG}: duplicate device {dev} ({dev_type}) as base for filesystems"
        )));
    }
    Ok(())
}

// Collect remaining sys_lvms - fs-ready only
pub(crate) fn collect_fs_ready_devs_lvm(
    sys_lvms: HashMap<String, BlockDevPaths>,
    fs_ready_devs: &mut HashSet<String>,
) {
    for list in sys_lvms.into_values().flatten() {
        let dev = list.back();
        if dev.is_none() {
            continue;
        }

        let dev = dev.unwrap();
        if !is_fs_ready(&dev.device_type) {
            continue;
        }

        // We should be able to ignore LVM LV duplicates
        fs_ready_devs.insert(dev.device.clone());
    }
}

pub(super) fn collect_fs_devs(
    sys_fs_devs: &HashMap<String, BlockDevType>,
    fs_devs: &mut HashSet<String>,
) -> Result<(), AliError> {
    const MSG: &str = "fs collection failed";

    for (dev, dev_type) in sys_fs_devs {
        if fs_devs.insert(dev.clone()) {
            continue;
        }

        return Err(AliError::AliRsBug(format!(
            "{MSG}: duplicate filesystem devices from from system filesystems: {dev} ({dev_type})",
        )));
    }

    Ok(())
}
