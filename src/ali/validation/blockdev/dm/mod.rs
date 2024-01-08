mod luks;
mod lv;
mod pv;
mod vg;

use std::collections::{
    HashMap,
    LinkedList,
};

use crate::ali::validation::*;
use crate::ali::{
    self,
    Dm,
};
use crate::errors::AliError;
use crate::types::blockdev::*;

pub(super) fn collect_valids(
    dms: &[Dm],
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_fs_ready_devs: &mut HashMap<String, BlockDevType>,
    sys_lvms: &mut HashMap<String, BlockDevPaths>,
    valids: &mut BlockDevPaths,
) -> Result<(), AliError> {
    // Validate sizing of LVs
    // Only the last LV on each VG could be unsized (100%FREE)
    lv::validate_size(dms)?;

    // Collect all DMs into valids to be used later in filesystems validation
    for dm in dms {
        match dm {
            Dm::Luks(luks) => {
                // Appends LUKS to a path in valids, if OK
                luks::collect_valid(
                    luks,
                    sys_fs_devs,
                    sys_fs_ready_devs,
                    sys_lvms,
                    valids,
                )?;
            }

            // We validate a LVM manifest block by adding valid devices in these exact order:
            // PV -> VG -> LV
            // This gives us certainty that during VG validation, any known PV would have been in valids.
            Dm::Lvm(lvm) => {
                if let Some(pvs) = &lvm.pvs {
                    for pv_path in pvs {
                        // Appends PV to a path in valids, if OK
                        pv::collect_valid(
                            pv_path,
                            sys_fs_devs,
                            sys_fs_ready_devs,
                            sys_lvms,
                            valids,
                        )?;
                    }
                }

                if let Some(vgs) = &lvm.vgs {
                    for vg in vgs {
                        // Appends VG to paths in valids, if OK
                        vg::collect_valid(vg, sys_fs_devs, sys_lvms, valids)?;
                    }
                }

                if let Some(lvs) = &lvm.lvs {
                    for lv in lvs {
                        // Appends LV to paths in valids, if OK
                        lv::collect_valid(lv, sys_fs_devs, sys_lvms, valids)?;
                    }
                }
            }
        }
    }

    Ok(())
}

#[inline(always)]
fn is_luks_base(dev_type: &BlockDevType) -> bool {
    matches!(
        dev_type,
        BlockDevType::UnknownBlock
            | BlockDevType::Disk
            | BlockDevType::Partition
            | BlockDevType::Dm(DmType::LvmLv)
    )
}

#[inline(always)]
fn is_pv_base(dev_type: &BlockDevType) -> bool {
    matches!(
        dev_type,
        BlockDevType::UnknownBlock
            | BlockDevType::Disk
            | BlockDevType::Partition
            | BlockDevType::Dm(DmType::Luks)
    )
}

#[inline(always)]
fn is_vg_base(dev_type: &BlockDevType) -> bool {
    matches!(dev_type, BlockDevType::Dm(DmType::LvmPv))
}

// #[inline(always)]
// fn is_lv_base(dev_type: &BlockDevType) -> bool {
//     matches!(dev_type, BlockDevType::Dm(DmType::LvmVg))
// }
