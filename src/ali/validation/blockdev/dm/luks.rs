use super::*;
use crate::ali::ManifestLuks;

// Collects valid block device path(s) into valids
#[inline]
pub(super) fn collect_valid(
    luks: &ManifestLuks,
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_fs_ready_devs: &mut HashMap<String, BlockDevType>,
    sys_lvms: &mut HashMap<String, BlockDevPaths>,
    valids: &mut BlockDevPaths,
) -> Result<(), AliError> {
    let (luks_base_path, luks_path) =
        (&luks.device, format!("/dev/mapper/{}", luks.name));

    let msg = "dm luks validation failed";
    if file_exists(&luks_path) {
        return Err(AliError::BadManifest(format!(
            "{msg}: device {luks_path} already exists"
        )));
    }

    if let Some(fs_type) = sys_fs_devs.get(luks_base_path) {
        return Err(AliError::BadManifest(format!(
            "{msg}: luks {} base {luks_base_path} was already in use as {fs_type}",
            luks.name
        )));
    }

    let mut found_vg: Option<BlockDev> = None;

    // Find base LV and its VG in existing LVMs
    'find_some_vg: for (lvm_base, sys_lvm_lists) in sys_lvms.iter() {
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
                return Err(AliError::BadManifest(format!(
                    "{msg}: luks base {} (itself is an LVM from {}) cannot have type {}",
                    luks_base_path, lvm_base, top_most.device_type
                )));
            }

            // We could really use unstable Cursor type here
            // See also: https://doc.rust-lang.org/std/collections/linked_list/struct.Cursor.html
            let mut path = sys_lvm.clone();
            path.pop_back();
            let should_be_vg = path.pop_back().expect("no vg after 2 pops");

            if should_be_vg.device_type != TYPE_VG {
                return Err(AliError::AliRsBug(format!(
                    "{msg}: unexpected device type {} - expecting a VG",
                    should_be_vg.device_type,
                )));
            }

            found_vg = Some(should_be_vg);
            break 'find_some_vg;
        }
    }

    let dev_luks: BlockDev = luks.into();

    // Although a LUKS can only sit on 1 LV,
    // We keep pushing since an LV may sit on VG with >1 PVs
    if let Some(vg) = found_vg {
        // Push all paths leading to VG and LV
        'new_pv: for sys_lvm_lists in sys_lvms.values_mut() {
            for sys_lvm in sys_lvm_lists.iter_mut() {
                let top_most = sys_lvm.back();

                if top_most.is_none() {
                    continue;
                }

                // Check if this path contains our VG -> LV
                let top_most = top_most.unwrap();
                if top_most.device.as_str() != luks_base_path {
                    continue;
                }

                let mut tmp_path = sys_lvm.clone();
                tmp_path.pop_back();
                let maybe_vg = tmp_path.pop_back().expect("no vg after 2 pops");

                if maybe_vg.device_type != TYPE_VG {
                    return Err(AliError::AliRsBug(format!(
                        "{msg}: unexpected device type {} - expecting a VG",
                        maybe_vg.device_type,
                    )));
                }

                if maybe_vg.device.as_str() != vg.device {
                    continue;
                }

                let mut list = sys_lvm.clone();
                list.push_back(dev_luks.clone());
                valids.push(list);
                sys_lvm.clear();

                continue 'new_pv;
            }
        }

        return Ok(());
    }

    // Find base device for LUKS
    // There's a possibility that LUKS sits on manifest LV on some VG
    // with itself having >1 PVs
    let mut found = false;
    for list in valids.iter_mut() {
        let top_most = list.back().expect("no back node in linked list in v");

        if top_most.device.as_str() != luks_base_path {
            continue;
        }

        if !is_luks_base(&top_most.device_type) {
            return Err(AliError::BadManifest(format!(
                "{msg}: luks {} base {luks_base_path} cannot have type {}",
                luks.name, top_most.device_type,
            )));
        }

        found = true;
        list.push_back(dev_luks.clone());
    }

    if found {
        return Ok(());
    }

    let unknown_base = BlockDev {
        device: luks_base_path.clone(),
        device_type: TYPE_UNKNOWN,
    };

    if sys_fs_ready_devs.contains_key(luks_base_path) {
        valids.push(LinkedList::from([unknown_base, dev_luks]));

        // Clear used up sys fs_ready device
        sys_fs_ready_devs.remove(luks_base_path);

        return Ok(());
    }

    // TODO: This may introduce error if such file is not a proper block device.
    if !file_exists(luks_base_path) {
        return Err(AliError::NoSuchDevice(luks_base_path.to_string()));
    }

    valids.push(LinkedList::from([unknown_base, dev_luks]));

    Ok(())
}
