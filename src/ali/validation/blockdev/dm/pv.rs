use super::*;

// Collect valid PV device path into valids
#[inline]
pub(super) fn collect_valid(
    pv_path: &str,
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_fs_ready_devs: &mut HashMap<String, BlockDevType>,
    sys_lvms: &mut HashMap<String, BlockDevPaths>,
    valids: &mut BlockDevPaths,
) -> Result<(), AliError> {
    let msg = "lvm pv validation failed";
    if let Some(fs_type) = sys_fs_devs.get(pv_path) {
        return Err(AliError::BadManifest(format!(
            "{msg}: pv {pv_path} base was already used as {fs_type}",
        )));
    }

    // Find and invalidate duplicate PV if it was used for other VG
    if let Some(sys_pv_lvms) = sys_lvms.get(pv_path) {
        for node in sys_pv_lvms.iter().flatten() {
            if node.device_type != TYPE_VG {
                continue;
            }

            return Err(AliError::BadManifest(format!(
                "{msg}: pv {pv_path} was already used for other vg {}",
                node.device,
            )));
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
            return Err(AliError::BadManifest(format!(
                "{msg}: duplicate pv {pv_path} in manifest"
            )));
        }

        if !is_pv_base(&top_most.device_type) {
            return Err(AliError::BadManifest(format!(
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
        return Err(AliError::BadManifest(format!(
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

    Ok(())
}
