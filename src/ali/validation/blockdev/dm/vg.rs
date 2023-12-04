use super::*;
use crate::ali::ManifestLvmVg;

// Collect valid VG device path into valids
#[inline]
pub(super) fn collect_valid(
    vg: &ManifestLvmVg,
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_lvms: &mut HashMap<String, BlockDevPaths>,
    valids: &mut BlockDevPaths,
) -> Result<(), AliError> {
    let dev_vg: BlockDev = vg.into();

    let msg = "lvm vg validation failed";
    'validate_vg_pv: for pv_base in &vg.pvs {
        // Invalidate VG if its PV was already used as FS partition
        if let Some(fs) = sys_fs_devs.get(pv_base) {
            return Err(AliError::BadManifest(format!(
                "{msg}: vg {} base {} was already used as filesystem {fs}",
                vg.name, pv_base
            )));
        }

        // Invalidate VG if its PV was already used in sys LVM
        if let Some(sys_pv_lvms) = sys_lvms.get(pv_base) {
            for node in sys_pv_lvms.iter().flatten() {
                if node.device_type != TYPE_VG {
                    continue;
                }

                return Err(AliError::BadManifest(format!(
                    "{msg}: vg {} base {} was already used for other vg {}",
                    vg.name, pv_base, node.device,
                )));
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
                return Err(AliError::BadManifest(format!(
                    "{msg}: vg {} pv base {pv_base} cannot have type {}",
                    vg.name, top_most.device_type,
                )));
            }

            list.push_back(dev_vg.clone());

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
                if *top_most == dev_vg {
                    return Err(AliError::BadManifest(format!(
                        "{msg}: vg {} already exists",
                        vg.name,
                    )));
                }

                if top_most.device.as_str() != pv_base {
                    continue;
                }

                if !is_vg_base(&top_most.device_type) {
                    return Err(AliError::BadManifest(format!(
                        "{msg}: vg {} pv base {pv_base} cannot have type {}",
                        vg.name, top_most.device_type
                    )));
                }

                let mut new_list = sys_lvm.clone();
                new_list.push_back(dev_vg.clone());

                // Push to valids, and remove used up sys_lvms path
                valids.push(new_list);
                sys_lvm.clear();

                continue 'validate_vg_pv;
            }
        }

        return Err(AliError::BadManifest(format!(
            "{msg}: no pv device matching {pv_base} in manifest or in the system"
        )));
    }

    Ok(())
}
