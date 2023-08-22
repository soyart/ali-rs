use std::collections::{HashMap, LinkedList};

use crate::entity::blockdev::*;
use crate::errors::NayiError;
use crate::manifest::validation::*;
use crate::manifest::{ManifestLuks, ManifestLvmLv, ManifestLvmVg};

#[inline(always)]
fn is_pv_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Disk => true,
        BlockDevType::Partition => true,
        BlockDevType::UnknownBlock => true,
        BlockDevType::Dm(DmType::Luks) => true,
        _ => false,
    }
}

#[inline(always)]
fn is_vg_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Dm(DmType::LvmPv) => true,
        _ => false,
    }
}

#[inline(always)]
fn is_lv_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Dm(DmType::LvmVg) => true,
        _ => false,
    }
}

#[inline(always)]
fn is_luks_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Disk => true,
        BlockDevType::Partition => true,
        BlockDevType::UnknownBlock => true,
        BlockDevType::Dm(DmType::LvmLv) => true,
        _ => false,
    }
}

// Collects valid block device path(s) into valids
#[inline]
pub(super) fn collect_valid_luks(
    luks: &ManifestLuks,
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_fs_ready_devs: &mut HashMap<String, BlockDevType>,
    sys_lvms: &mut HashMap<String, BlockDevPaths>,
    valids: &mut BlockDevPaths,
) -> Result<(), NayiError> {
    let (luks_base_path, luks_path) = (&luks.device, format!("/dev/mapper/{}", luks.name));

    let msg = "dm luks validation failed";
    if file_exists(&luks_path) {
        return Err(NayiError::BadManifest(format!(
            "{msg}: device {luks_path} already exists"
        )));
    }

    if let Some(fs_type) = sys_fs_devs.get(luks_base_path) {
        return Err(NayiError::BadManifest(format!(
            "{msg}: luks {} base {luks_base_path} was already in use as {fs_type}",
            luks.name
        )));
    }

    // Find base device for LUKS
    for list in valids.iter_mut() {
        let top_most = list.back().expect("no back node in linked list in v");

        if top_most.device.as_str() != luks_base_path {
            continue;
        }

        if !is_luks_base(&top_most.device_type) {
            return Err(NayiError::BadManifest(format!(
                "{msg}: luks {} base {luks_base_path} cannot have type {}",
                luks.name, top_most.device_type,
            )));
        }

        list.push_back(BlockDev {
            device: luks_path.clone(),
            device_type: TYPE_LUKS,
        });

        return Ok(());
    }

    // Find base LV in existing LVM
    for (lvm_base, sys_lvm_lists) in sys_lvms.iter_mut() {
        for sys_lvm in sys_lvm_lists {
            let top_most = sys_lvm.back();

            if top_most.is_none() {
                continue;
            }

            let top_most = top_most.unwrap();
            if top_most.device.as_str() != luks_base_path {
                continue;
            }

            if !is_luks_base(&top_most.device_type) {
                return Err(NayiError::BadManifest(format!(
                    "{msg}: luks base {} (itself is an LVM from {}) cannot have type {}",
                    luks_base_path, lvm_base, top_most.device_type
                )));
            }

            // Copy and update list from existing_lvms to valids
            let mut list = sys_lvm.clone();

            list.push_back(BlockDev {
                device: luks_path.clone(),
                device_type: TYPE_LUKS,
            });

            // Push to v, and clear used up sys LVM device
            valids.push(list);
            sys_lvm.clear();

            return Ok(());
        }
    }

    if sys_fs_ready_devs.contains_key(luks_base_path) {
        valids.push(LinkedList::from([
            BlockDev {
                device: luks_base_path.clone(),
                device_type: TYPE_UNKNOWN,
            },
            BlockDev {
                device: luks_path,
                device_type: TYPE_LUKS,
            },
        ]));

        // Clear used up sys fs_ready device
        sys_fs_ready_devs.remove(luks_base_path);

        return Ok(());
    }

    // TODO: This may introduce error if such file is not a proper block device.
    if !file_exists(luks_base_path) {
        return Err(NayiError::NoSuchDevice(luks_base_path.to_string()));
    }

    valids.push(LinkedList::from([
        BlockDev {
            device: luks_base_path.clone(),
            device_type: TYPE_UNKNOWN,
        },
        BlockDev {
            device: luks_path,
            device_type: TYPE_LUKS,
        },
    ]));

    Ok(())
}

// Collect valid PV device path into valids
#[inline]
pub(super) fn collect_valid_pv(
    pv_path: &str,
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_fs_ready_devs: &mut HashMap<String, BlockDevType>,
    sys_lvms: &mut HashMap<String, BlockDevPaths>,
    valids: &mut BlockDevPaths,
) -> Result<(), NayiError> {
    let msg = "lvm pv validation failed";
    if let Some(fs_type) = sys_fs_devs.get(pv_path) {
        return Err(NayiError::BadManifest(format!(
            "{msg}: pv {pv_path} base was already used as {fs_type}",
        )));
    }

    // Find and invalidate duplicate PV if it was used for other VG
    if let Some(sys_pv_lvms) = sys_lvms.get(pv_path) {
        for sys_pv_path in sys_pv_lvms {
            for node in sys_pv_path {
                if node.device_type == TYPE_VG {
                    return Err(NayiError::BadManifest(format!(
                        "{msg}: pv {pv_path} was already used for other vg {}",
                        node.device,
                    )));
                }
            }
        }
    }

    // Find PV base from top-most values in v
    for list in valids.iter_mut() {
        let top_most = list
            .back()
            .expect("no back node in linked list from manifest_devs");

        if top_most.device.as_str() != pv_path {
            continue;
        }

        if top_most.device_type == TYPE_PV {
            return Err(NayiError::BadManifest(format!(
                "{msg}: duplicate pv {pv_path} in manifest"
            )));
        }

        if !is_pv_base(&top_most.device_type) {
            return Err(NayiError::BadManifest(format!(
                "{msg}: pv {} base cannot have type {}",
                pv_path, top_most.device_type,
            )));
        }

        list.push_back(BlockDev {
            device: pv_path.to_string(),
            device_type: TYPE_PV,
        });

        return Ok(());
    }

    // Check if PV base device is in sys_fs_ready_devs
    if sys_fs_ready_devs.contains_key(pv_path) {
        // Add both base and PV
        valids.push(LinkedList::from([
            BlockDev {
                device: pv_path.to_string(),
                device_type: TYPE_UNKNOWN,
            },
            BlockDev {
                device: pv_path.to_string(),
                device_type: TYPE_PV,
            },
        ]));

        // Removed used up sys fs_ready device
        sys_fs_ready_devs.remove(pv_path);
        return Ok(());
    }

    // TODO: This may introduce error if such file is not a proper block device.
    if !file_exists(pv_path) {
        return Err(NayiError::BadManifest(format!(
            "{msg}: no such pv device: {pv_path}"
        )));
    }

    valids.push(LinkedList::from([
        BlockDev {
            device: pv_path.to_string(),
            device_type: TYPE_UNKNOWN,
        },
        BlockDev {
            device: pv_path.to_string(),
            device_type: TYPE_PV,
        },
    ]));

    return Ok(());
}

// Collect valid VG device path into valids
#[inline]
pub(super) fn collect_valid_vg(
    vg: &ManifestLvmVg,
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_lvms: &mut HashMap<String, BlockDevPaths>,
    valids: &mut BlockDevPaths,
) -> Result<(), NayiError> {
    let vg_dev = BlockDev {
        device: format!("/dev/{}", vg.name),
        device_type: TYPE_VG,
    };

    let msg = "lvm vg validation failed";
    'validate_vg_pv: for pv_base in &vg.pvs {
        // Invalidate VG if its PV was already used as FS partition
        if let Some(fs) = sys_fs_devs.get(pv_base) {
            return Err(NayiError::BadManifest(format!(
                "{msg}: vg {} base {} was already used as filesystem {fs}",
                vg.name, pv_base
            )));
        }

        // Invalidate VG if its PV was already used in sys LVM
        if let Some(sys_pv_lvms) = sys_lvms.get(pv_base) {
            for sys_pv_path in sys_pv_lvms {
                for node in sys_pv_path {
                    if node.device_type == TYPE_VG {
                        return Err(NayiError::BadManifest(format!(
                            "{msg}: vg {} base {} was already used for other vg {}",
                            vg.name, pv_base, node.device,
                        )));
                    }
                }
            }
        }

        // Check if top-most device is PV
        for list in valids.iter_mut() {
            let top_most = list
                .back()
                .expect("no back node in linked list from manifest_devs");

            if top_most.device.as_str() != pv_base {
                continue;
            }

            if !is_vg_base(&top_most.device_type) {
                return Err(NayiError::BadManifest(format!(
                    "{msg}: vg {} pv base {pv_base} cannot have type {}",
                    vg.name, top_most.device_type,
                )));
            }

            list.push_back(vg_dev.clone());

            continue 'validate_vg_pv;
        }

        // Find sys_lvm PV to base on
        for sys_lvm_lists in sys_lvms.values_mut() {
            for sys_lvm in sys_lvm_lists {
                let top_most = sys_lvm.back();

                if top_most.is_none() {
                    continue;
                }

                let top_most = top_most.unwrap();
                if *top_most == vg_dev {
                    return Err(NayiError::BadManifest(format!(
                        "{msg}: vg {} already exists",
                        vg.name,
                    )));
                }

                if top_most.device.as_str() != pv_base {
                    continue;
                }

                if !is_vg_base(&top_most.device_type) {
                    return Err(NayiError::BadManifest(format!(
                        "{msg}: vg {} pv base {pv_base} cannot have type {}",
                        vg.name, top_most.device_type
                    )));
                }

                let mut new_list = sys_lvm.clone();
                new_list.push_back(vg_dev.clone());

                // Push to valids, and remove used up sys_lvms path
                valids.push(new_list);
                sys_lvm.clear();

                continue 'validate_vg_pv;
            }
        }

        return Err(NayiError::BadManifest(format!(
            "{msg}: no pv device matching {pv_base} in manifest or in the system"
        )));
    }

    Ok(())
}

// Collect valid LV device path(s) into valids
#[inline]
pub(super) fn collect_valid_lv(
    lv: &ManifestLvmLv,
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_lvms: &mut HashMap<String, BlockDevPaths>,
    valids: &mut BlockDevPaths,
) -> Result<(), NayiError> {
    let vg_name = format!("/dev/{}", lv.vg);
    let lv_name = format!("{vg_name}/{}", lv.name);

    let msg = "lvm lv validation failed";
    if let Some(fs) = sys_fs_devs.get(&lv_name) {
        return Err(NayiError::BadManifest(format!(
            "{msg}: another lv with matching name {lv_name} was already used as filesystem {fs}"
        )));
    }

    let lv_dev = BlockDev {
        device: lv_name.clone(),
        device_type: TYPE_LV,
    };

    // A VG can host multiple LVs, so we will need to copy the LV
    // to all paths leading to it. This means that we must leave the
    // matching VG path in-place before we can
    let mut lv_vgs = Vec::new();

    let msg = "lvm lv validation failed";
    for sys_lvm_lists in sys_lvms.values_mut() {
        for sys_lvm in sys_lvm_lists.iter_mut() {
            let top_most = sys_lvm.back();

            if top_most.is_none() {
                continue;
            }

            let top_most = top_most.unwrap();
            if *top_most == lv_dev {
                return Err(NayiError::BadManifest(format!(
                    "{msg}: lv {lv_name} already exists"
                )));
            }

            if top_most.device != vg_name {
                continue;
            }

            if !is_lv_base(&top_most.device_type) {
                return Err(NayiError::BadManifest(format!(
                    "{msg}: lv {lv_name} vg base {vg_name} cannot have type {}",
                    top_most.device_type
                )));
            }

            let mut list = sys_lvm.clone();
            list.push_back(lv_dev.clone());
            lv_vgs.push(list);
        }
    }

    for old_list in valids.iter_mut() {
        let top_most = old_list
            .back()
            .expect("no back node for linked list in manifest_devs");

        // Skip path from different VG
        if *top_most == lv_dev {
            continue;
        }

        if top_most.device != vg_name {
            continue;
        }

        if !is_lv_base(&top_most.device_type) {
            return Err(NayiError::BadManifest(format!(
                "{msg}: lv {lv_name} vg base {vg_name} cannot have type {}",
                top_most.device_type
            )));
        }

        let mut list = old_list.clone();
        list.push_back(lv_dev.clone());
        lv_vgs.push(list);
    }

    if lv_vgs.is_empty() {
        return Err(NayiError::BadManifest(format!(
            "{msg}: lv {lv_name} no vg device matching {vg_name} in manifest or in the system"
        )));
    }

    valids.extend_from_slice(&lv_vgs);

    Ok(())
}
