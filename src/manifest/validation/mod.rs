mod trace_blk;

use std::collections::{HashMap, HashSet, LinkedList};

use crate::errors::NayiError;
use crate::manifest::{Dm, Manifest};
use crate::utils::fs::file_exists;
use crate::utils::shell::in_path;

pub fn validate(manifest: &Manifest) -> Result<(), NayiError> {
    // Get full blkid output
    let output_blkid = trace_blk::run_blkid("blkid")?;

    // A hash map of existing block device that can be used as filesystem base
    let sys_fs_ready_devs = trace_blk::sys_fs_ready(&output_blkid);

    // A hash map of existing block device and its filesystems
    let sys_fs_devs = trace_blk::sys_fs(&output_blkid);

    // Get all paths of existing LVM devices.
    // Unknown disks are not tracked - only LVM devices and their bases.
    let sys_lvms = trace_blk::sys_lvms("lvs", "pvs");

    validate_blk(&manifest, &sys_fs_devs, sys_fs_ready_devs, sys_lvms)?;

    let mkfs_rootfs = &format!("mkfs.{}", manifest.rootfs.fs_type);
    if !in_path(mkfs_rootfs) {
        return Err(NayiError::BadManifest(format!(
            "no such program to create rootfs: {mkfs_rootfs}"
        )));
    }

    for fs in &manifest.filesystems {
        let mkfs_cmd = &format!("mkfs.{}", fs.fs_type);
        if !in_path(mkfs_cmd) {
            let device = &fs.device;

            return Err(NayiError::BadManifest(format!(
                "no such program to create filesystem for device {device}: {mkfs_cmd}"
            )));
        }
    }

    let zone_info = format!("/usr/share/zoneinfo/{}", manifest.timezone);
    if !file_exists(&zone_info) {
        return Err(NayiError::BadManifest(format!(
            "no zone info file {zone_info}"
        )));
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq, std::hash::Hash, Clone)]
struct BlockDev {
    // Full path to device, with hard-coded format:
    // Disk => /dev/dev_name
    // PV   => /dev/pv_name          (same as disk)
    // VG   => /dev/vg_name
    // LV   => /dev/vg_name/lv_name
    // LUKS => /dev/mapper/luks_name
    device: String,
    device_type: BlockDevType,
}

#[derive(Debug, PartialEq, Eq, std::hash::Hash, Clone)]
enum DmType {
    Luks,
    LvmPv,
    LvmVg,
    LvmLv,
}

#[derive(Debug, PartialEq, Eq, std::hash::Hash, Clone)]
enum BlockDevType {
    // Disks
    Disk,

    // Disk partitions (GPT/MS-DOS)
    Partition,

    // UnknownBlock is anything that be can build filesystem, LUKS, and LVM PV on
    UnknownBlock,

    // Software-defined storage
    Dm(DmType),

    // Filesystems
    Fs(String),
}

const TYPE_DISK: BlockDevType = BlockDevType::Disk;
const TYPE_PART: BlockDevType = BlockDevType::Partition;
const TYPE_UNKNOWN: BlockDevType = BlockDevType::UnknownBlock;
const TYPE_LUKS: BlockDevType = BlockDevType::Dm(DmType::Luks);
const TYPE_PV: BlockDevType = BlockDevType::Dm(DmType::LvmPv);
const TYPE_VG: BlockDevType = BlockDevType::Dm(DmType::LvmVg);
const TYPE_LV: BlockDevType = BlockDevType::Dm(DmType::LvmLv);

fn is_pv_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Disk => true,
        BlockDevType::Partition => true,
        BlockDevType::UnknownBlock => true,
        BlockDevType::Dm(DmType::Luks) => true,
        _ => false,
    }
}

fn is_vg_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Dm(DmType::LvmPv) => true,
        _ => false,
    }
}

fn is_lv_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Dm(DmType::LvmVg) => true,
        _ => false,
    }
}

fn is_luks_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Disk => true,
        BlockDevType::Partition => true,
        BlockDevType::UnknownBlock => true,
        BlockDevType::Dm(DmType::LvmLv) => true,
        _ => false,
    }
}

fn is_fs_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Disk => true,
        BlockDevType::Partition => true,
        BlockDevType::UnknownBlock => true,
        BlockDevType::Dm(DmType::Luks) => true,
        BlockDevType::Dm(DmType::LvmLv) => true,
        _ => false,
    }
}

// Validates manifest block storage.
// sys_fs_ready_devs and sys_lvms are copied from caller,
// and are made mutable because we need to remove used up elements
fn validate_blk(
    manifest: &Manifest,
    sys_fs_devs: &HashMap<String, BlockDevType>, // Maps fs devs to their FS type (e.g. Btrfs)
    mut sys_fs_ready_devs: HashMap<String, BlockDevType>, // Maps fs-ready devs to their types (e.g. partition)
    mut sys_lvms: HashMap<String, Vec<LinkedList<BlockDev>>>, // Maps pv path to all possible LV paths
) -> Result<(), NayiError> {
    // valids collects all valid known devices to be created in the manifest
    let mut valids = Vec::<LinkedList<BlockDev>>::new();

    for disk in &manifest.disks {
        if !file_exists(&disk.device) {
            return Err(NayiError::BadManifest(format!(
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

        // Base disk
        let base = LinkedList::from([BlockDev {
            device: disk.device.clone(),
            device_type: TYPE_DISK,
        }]);

        // Check if this partition is already in use
        let msg = "partition validation failed";
        for (i, _) in disk.partitions.iter().enumerate() {
            let partition_name = format!("{partition_prefix}{}", i + 1);

            if let Some(_) = sys_fs_ready_devs.get(&partition_name) {
                return Err(NayiError::BadManifest(format!(
                    "{msg}: partition {partition_name} already exists on system"
                )));
            }

            if let Some(existing_fs) = sys_fs_devs.get(&partition_name) {
                return Err(NayiError::BadManifest(format!(
                    "{msg}: partition {partition_name} is already used as {existing_fs}"
                )));
            }

            let mut partition = base.clone();
            partition.push_back(BlockDev {
                device: partition_name,
                device_type: TYPE_PART,
            });

            valids.push(partition);
        }
    }

    'validate_dm: for dm in &manifest.dm {
        match dm {
            Dm::Luks(luks) => {
                let msg = "dm luks validation failed";

                let (luks_base_path, luks_path) =
                    (&luks.device, format!("/dev/mapper/{}", luks.name));

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

                    continue 'validate_dm;
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

                        continue 'validate_dm;
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
                    continue 'validate_dm;
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
            }

            // We validate a LVM manifest block by adding valid devices in these exact order:
            // PV -> VG -> LV
            // This gives us certainty that during VG validation, any known PV would have been in valids.
            Dm::Lvm(lvm) => {
                let mut msg = "lvm pv validation failed";

                'validate_pv: for pv_path in &lvm.pvs {
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
                            device: pv_path.clone(),
                            device_type: TYPE_PV,
                        });

                        continue 'validate_pv;
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
                        continue 'validate_pv;
                    }

                    // TODO: This may introduce error if such file is not a proper block device.
                    if !file_exists(pv_path) {
                        return Err(NayiError::BadManifest(format!(
                            "{msg}: no such pv device: {pv_path}"
                        )));
                    }

                    valids.push(LinkedList::from([
                        BlockDev {
                            device: pv_path.clone(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: pv_path.clone(),
                            device_type: TYPE_PV,
                        },
                    ]));
                }

                msg = "lvm vg validation failed";
                for vg in &lvm.vgs {
                    let multi_pvs = vg.pvs.len() > 1;
                    let last_pv_idx = vg.pvs.len() - 1;

                    // If a VG sits on top of >=1 PVs, then we will have to add a new list to valids (tmp_valids)
                    let mut tmp_valids = Vec::new();

                    let vg_dev = BlockDev {
                        device: format!("/dev/{}", vg.name),
                        device_type: TYPE_VG,
                    };

                    'validate_vg_pv: for (pv_idx, pv_base) in vg.pvs.iter().enumerate() {
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

                            // Copy a new list into valids
                            if multi_pvs && pv_idx != last_pv_idx {
                                let mut new_list = list.clone();
                                new_list.push_back(vg_dev.clone());

                                tmp_valids.push(new_list);
                                continue;
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

                        // Copy forked lists for VGs with >1 PVs
                        if !tmp_valids.is_empty() {
                            valids.append(&mut tmp_valids);
                            continue 'validate_vg_pv;
                        }

                        return Err(NayiError::BadManifest(format!(
                            "{msg}: no pv device matching {pv_base} in manifest or in the system"
                        )));
                    }
                }

                msg = "lvm lv validation failed";
                'validate_lv: for lv in &lvm.lvs {
                    let vg_name = format!("/dev/{}", lv.vg);
                    let lv_name = format!("{vg_name}/{}", lv.name);

                    let lv_dev = BlockDev {
                        device: lv_name.clone(),
                        device_type: TYPE_LV,
                    };

                    for sys_lvm_lists in sys_lvms.values_mut() {
                        for sys_lvm_list in sys_lvm_lists.iter_mut() {
                            let top_most = sys_lvm_list.back();

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

                            let mut list = sys_lvm_list.clone();
                            list.push_back(lv_dev);

                            valids.push(list);
                            sys_lvm_list.clear();

                            continue 'validate_lv;
                        }
                    }

                    for list in valids.iter_mut() {
                        let top_most = list
                            .back()
                            .expect("no back node for linked list in manifest_devs");

                        if *top_most == lv_dev {
                            return Err(NayiError::BadManifest(format!(
                                "{msg}: duplicate lv {lv_name} in manifest"
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

                        // Copy the path to VG, and leave it in-place
                        // for other LVs that sits on this VG to later use.
                        list.push_back(lv_dev);

                        continue 'validate_lv;
                    }

                    return Err(NayiError::BadManifest(format!(
                        "{msg}: no vg device matching {vg_name} in manifest or in the system"
                    )));
                }
            }
        }
    }

    // fs_ready_devs is used to validate manifest.fs
    let mut fs_ready_devs = HashSet::<String>::new();

    // Collect remaining sys_fs_ready_devs
    for (dev, dev_type) in sys_fs_ready_devs {
        if is_fs_base(&dev_type) {
            fs_ready_devs.insert(dev);
            continue;
        }

        return Err(NayiError::NayiRsBug(format!(
            "fs-ready dev {dev} is not fs-ready"
        )));
    }

    // Collect remaining sys_lvms - fs-ready only
    for sys_lvm_lists in sys_lvms.into_values() {
        for list in sys_lvm_lists {
            if let Some(top_most) = list.back() {
                if is_fs_base(&top_most.device_type) {
                    fs_ready_devs.insert(top_most.device.clone());
                }
            }
        }
    }

    // Collect from valids - fs-ready only
    for list in valids {
        let top_most = list.back().expect("v is missing top-most device");
        if is_fs_base(&top_most.device_type) {
            fs_ready_devs.insert(top_most.device.clone());
        }
    }

    // Validate root FS, other FS, and swap against fs_ready_devs
    let mut msg = "rootfs validation failed";
    if !fs_ready_devs.contains(&manifest.rootfs.device.clone()) {
        return Err(NayiError::BadManifest(format!(
            "{msg}: no top-level fs-ready device for rootfs: {}",
            manifest.rootfs.device,
        )));
    }

    // Remove used up fs-ready device
    fs_ready_devs.remove(&manifest.rootfs.device);

    msg = "fs validation failed";
    for (i, fs) in manifest.filesystems.iter().enumerate() {
        if !fs_ready_devs.contains(&fs.device) {
            return Err(NayiError::BadManifest(format!(
                "{msg}: device {} for fs #{} ({}) is not fs-ready",
                fs.device,
                i + 1,
                fs.fs_type,
            )));
        }

        // Remove used up fs-ready device
        fs_ready_devs.remove(&fs.device);
    }

    msg = "swap validation failed";
    if let Some(ref swaps) = manifest.swap {
        for (i, swap) in swaps.iter().enumerate() {
            if fs_ready_devs.contains(swap) {
                fs_ready_devs.remove(swap);
                continue;
            }

            return Err(NayiError::BadManifest(format!(
                "{msg}: device {swap} for swap #{} is not fs-ready",
                i + 1,
            )));
        }
    }

    Ok(())
}

impl std::fmt::Display for DmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Luks => write!(f, "LUKS"),
            Self::LvmPv => write!(f, "LVM PV"),
            Self::LvmVg => write!(f, "LVM VG"),
            Self::LvmLv => write!(f, "LVM LV"),
        }
    }
}

impl std::fmt::Display for BlockDevType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disk => write!(f, "DISK"),
            Self::Partition => write!(f, "PARTITION"),
            Self::UnknownBlock => write!(f, "UNKNOWN_FS_BASE"),
            Self::Dm(dm_type) => write!(f, "DM_{}", dm_type),
            Self::Fs(fs_type) => write!(f, "FS_{}", fs_type),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::*;
    use std::collections::HashSet;

    #[derive(Debug)]
    struct Test {
        case: String,
        context: Option<String>, // Extra info about the test
        manifest: Manifest,
        sys_fs_ready_devs: HashMap<String, BlockDevType>,
        sys_fs_devs: HashMap<String, BlockDevType>,
        sys_lvms: HashMap<String, Vec<LinkedList<BlockDev>>>,
    }

    #[test]
    fn test_validate_blk() {
        let tests_should_ok = vec![
            Test {
                case: "Root and swap on existing partition".into(),
                context: None,
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/sda1".into(), BlockDevType::Disk),
                    ("/dev/nvme0n1p2".into(), BlockDevType::Disk),
                ]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    disks: vec![],
                    dm: vec![],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/sda1".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root on existing LV, swap on existing partition".into(),
                context: None,
                sys_fs_ready_devs: HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Disk,
                )]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::from([(
                    "/dev/sda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/sda1".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/myvg".into(),
                            device_type: TYPE_VG,
                        },
                        BlockDev {
                            device: "/dev/myvg/mylv".into(),
                            device_type: TYPE_LV,
                        },
                    ])],
                )]),

                manifest: Manifest {
                    disks: vec![],
                    dm: vec![],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root on existing LV, swap on manifest partition".into(),
                context: None,
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/sda1".into(), BlockDevType::Disk),
                    ("/dev/nvme0n1p2".into(), BlockDevType::Disk),
                ]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::from([(
                    "/dev/sda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/sda1".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/myvg".into(),
                            device_type: TYPE_VG,
                        },
                        BlockDev {
                            device: "/dev/myvg/mylv".into(),
                            device_type: TYPE_LV,
                        },
                    ])],
                )]),

                manifest: Manifest {
                    disks: vec![],
                    dm: vec![],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root and swap on existing LV on existing VG".into(),
                context: None,
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/sda1".into(), BlockDevType::Disk),
                    ("/dev/nvme0n1p2".into(), BlockDevType::Disk),
                ]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::from([(
                    "/dev/sda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/sda1".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/myvg".into(),
                            device_type: TYPE_VG,
                        },
                    ])],
                )]),

                manifest: Manifest {
                    disks: vec![],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec![],
                        vgs: vec![],
                        lvs: vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root on manifest LVM, built on existing partition. Swap on existing partition".into(),
                context: None,
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/sda1".into(), BlockDevType::Disk),
                    ("/dev/nvme0n1p2".into(), BlockDevType::Disk),
                ]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    disks: vec![],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec!["/dev/sda1".into()],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["/dev/sda1".into()],
                        }],
                        lvs: vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case:
                    "Root on manifest LVM, built on manifest partition. Swap on manifest partition"
                        .into(),
                context: None,
                sys_fs_ready_devs: HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Disk,
                )]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec!["./mock_devs/sda2".into()],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into()],
                        }],
                        lvs: vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root on manifest LVM on manifest partition/existing partition. Swap on manifest partition".into(),
                context: None,
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/nvme0n1p1".into(), TYPE_PART), ("/dev/nvme0n1p2".into(), TYPE_PART),
                ]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec![
                            "./mock_devs/sda2".into(),
                            "/dev/nvme0n1p1".into(),
                        ],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "/dev/nvme0n1p1".into(),
                            ],
                        }],
                        lvs: vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                },
            },

            Test {
                case:
                    "Root on manifest LVM, built on manifest/existing partition. Swap on manifest partition".into(),
                context: None,
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/nvme0n1p1".into(), TYPE_PART),
                    ("/dev/nvme0n1p2".into(), TYPE_PART),
                ]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV1".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }, ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ]
                        }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                            ],
                        }],
                        lvs: vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p1".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root and Swap on manifest LVs from the same VG".into(),
                context: Some("2 LVs on 1 VGs - VGs on 3 PVs".into()),
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/nvme0n1p1".into(), TYPE_PART),
                    ("/dev/nvme0n1p2".into(), TYPE_PART)],
                ),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV1".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }, ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ]
                        }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                            ],
                        }],
                        lvs: vec![
                        ManifestLvmLv {
                            name: "myswap".into(),
                            vg: "myvg".into(),
                            size: Some("8G".into()),
                        },
                        ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root and Swap on manifest LVs from the same VG".into(),
                context: Some("2 LVs on 1 VG on 4 PVs. One of the PV already exists".into()),
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/nvme0n1p1".into(), TYPE_PART),
                    ("/dev/nvme0n1p2".into(), TYPE_PART),
                ]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::from([
                    ("/dev/nvme0n2p7".into(), vec![
                        LinkedList::from(
                            [BlockDev { device: "/dev/nvme0n2p7".into(), device_type: TYPE_PV }],
                        ),
                    ]),
                ]),

                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV1".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }, ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ]
                        }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                                "/dev/nvme0n2p7".into(),
                            ],
                        }],
                        lvs: vec![
                        ManifestLvmLv {
                            name: "myswap".into(),
                            vg: "myvg".into(),
                            size: Some("8G".into()),
                        },
                        ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Multiple LVs on multiple VGs on multiple PVs".into(),
                context: Some("3 LVs on 2 VGs, each VG on 2 PVs - one PV already exists".into()),
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/nvme0n1p1".into(), TYPE_PART),
                    ("/dev/nvme0n1p2".into(), TYPE_PART),
                ]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::from([
                    ("/dev/nvme0n2p7".into(), vec![
                        LinkedList::from(
                            [BlockDev { device: "/dev/nvme0n2p7".into(), device_type: TYPE_PV }],
                        ),
                    ]),
                ]),

                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV1".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }, ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ]
                        }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ],
                        vgs: vec![
                            ManifestLvmVg {
                                name: "mysatavg".into(),
                                pvs: vec![
                                    "./mock_devs/sda2".into(),
                                    "./mock_devs/sdb1".into(),
                                ],
                            },
                            ManifestLvmVg {
                                name: "mynvmevg".into(),
                                pvs: vec![
                                    "/dev/nvme0n1p2".into(),
                                    "/dev/nvme0n2p7".into(),
                                ],
                            },
                        ],
                        lvs: vec![
                            ManifestLvmLv {
                                name: "myswap".into(),
                                vg: "mynvmevg".into(),
                                size: None,
                            },
                            ManifestLvmLv {
                                name: "rootlv".into(),
                                vg: "mysatavg".into(),
                                size: Some("20G".into()),
                            },
                            ManifestLvmLv {
                                name: "datalv".into(),
                                vg: "mysatavg".into(),
                                size: None,
                            },
                        ],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/mysatavg/rootlv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![
                        ManifestFs {
                            device: "/dev/mysatavg/datalv".into(),
                            mnt: "/opt/data".into(),
                            fs_type: "xfs".into(),
                            fs_opts: "".into(),
                            mnt_opts: "".into(),
                        },
                    ],
                    swap: Some(vec!["/dev/mynvmevg/myswap".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },
        ];

        let tests_should_err: Vec<Test> = vec![
            Test {
                case: "No manifest disks, root on non-existent, swap on non-existent".into(),
                context: None,
                sys_fs_ready_devs: HashMap::new(),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                    disks: vec![],
                    dm: vec![],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/sda1".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                },
            },

            Test {
                case: "No manifest disks, root on existing ext4 fs, swap on non-existent".into(),
                context: None,
                sys_fs_ready_devs: HashMap::new(),
                sys_fs_devs: HashMap::from([(
                    "/dev/sda1".into(),
                    BlockDevType::Fs("btrfs".into()),
                )]),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    disks: vec![],
                    dm: vec![],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/sda1".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions, but missing LV manifest".into(),
                context: None,
                sys_fs_ready_devs: HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Disk,
                )]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec!["./mock_devs/sda2".into()],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into()],
                        }],
                        lvs: vec![],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("VG is based on used PV".into()),
                sys_fs_ready_devs: HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Disk,
                )]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec!["./mock_devs/sda2".into()],
                        vgs: vec![
                            ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec!["./mock_devs/sda2".into()],
                            },
                            ManifestLvmVg {
                                name: "somevg".into(),
                                pvs: vec!["./mock_devs/sda2".into()],
                            },
                        ],
                        lvs: vec![],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions, but 1 fs is re-using rootfs LV".into(),
                context: None,
                sys_fs_ready_devs: HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Disk,
                )]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec!["./mock_devs/sda2".into()],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into()],
                        }],
                        lvs: vec![
                            ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![
                        ManifestFs {
                            device: "/dev/myvg.mylv".into(),
                            mnt: "/data".into(),
                            fs_type: "".into(),
                            fs_opts: "".into(),
                            mnt_opts: "".into(),
                        },
                    ],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

             Test {
                case: "Root on manifest LVM, built on manifest partitions and existing partition. Swap on manifest partition that was used to build PV".into(),
                context: None,
                sys_fs_ready_devs: HashMap::from(
                    [("/dev/nvme0n1p1".into(), TYPE_PART), ("/dev/nvme0n1p2".into(), TYPE_PART)],
                ),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV1".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }, ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ]
                        }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                            ],
                        }],
                        lvs: vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p2".into()]), // Was already used as manifest PV
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root on manifest LVM, built on manifest partitions and non-existent partition. Swap on manifest partition".into(),
                context: None,
                sys_fs_ready_devs: HashMap::from([("/dev/nvme0n1p1".into(), TYPE_PART)]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),
                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV1".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }, ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ]
                        }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                            ],
                        }],
                        lvs: vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/nvme0n1p1".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root and Swap on manifest LVs from the same VG, but existing VG partition already has fs".into(),
                context: Some("2 LVs on 1 VG on 4 PVs, but 1 PV already has swap".into()),
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/nvme0n1p1".into(), TYPE_PART),
                    ("/dev/nvme0n1p2".into(), TYPE_PART),
                ]),
                sys_fs_devs: HashMap::from([
                    ("/dev/nvme0n2p7".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_lvms: HashMap::new(),

                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV1".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }, ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ]
                    }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                                "/dev/nvme0n2p7".into(),
                            ],
                        }],
                        lvs: vec![
                        ManifestLvmLv {
                            name: "myswap".into(),
                            vg: "myvg".into(),
                            size: Some("8G".into()),
                        },
                        ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },

            Test {
                case: "Root and Swap on manifest LVs from the same VG".into(),
                context: Some("2 LVs on 1 VG on 4 PVs, but 1 PV was already used".into()),
                sys_fs_ready_devs: HashMap::from([("/dev/nvme0n1p1".into(), TYPE_PART), ("/dev/nvme0n1p2".into(), TYPE_PART)]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::from([
                    ("/dev/nvme0n2p7".into(), vec![
                        LinkedList::from(
                            [
                                BlockDev { device: "/dev/nvme0n2p7".into(), device_type: TYPE_PV },
                                BlockDev { device: "/dev/sysvg".into(), device_type: TYPE_VG },
                            ],
                        ),
                    ]),
                ]),

                manifest: Manifest {
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".into(),
                        table: PartitionTable::Gpt,
                        partitions: vec![
                            ManifestPartition {
                                label: "PART_EFI".into(),
                                size: Some("500M".into()),
                                part_type: "ef".into(),
                            },
                            ManifestPartition {
                                label: "PART_PV1".into(),
                                size: None,
                                part_type: "8e".into(),
                            },
                        ],
                    }, ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ]
                        }],
                    dm: vec![Dm::Lvm(ManifestLvm {
                        pvs: vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                                "/dev/nvme0n2p7".into(),
                            ],
                        }],
                        lvs: vec![
                        ManifestLvmLv {
                            name: "myswap".into(),
                            vg: "myvg".into(),
                            size: Some("8G".into()),
                        },
                        ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }],
                    })],
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: "/".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: "".into(),
                        mnt_opts: "".into(),
                    }),
                    filesystems: vec![],
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                },
            },
        ];

        for (i, test) in tests_should_ok.iter().enumerate() {
            let result = validate_blk(
                &test.manifest,
                &test.sys_fs_devs,
                test.sys_fs_ready_devs.clone(),
                test.sys_lvms.clone(),
            );

            if result.is_err() {
                eprintln!("Unexpected error from test case {}: {}", i + 1, test.case);

                if let Some(ref ctx) = test.context {
                    eprintln!("\nCONTEXT: {ctx}\n");
                }

                eprintln!("Test structure: {test:?}");
                eprintln!("Error: {result:?}");
            }

            assert!(result.is_ok());
        }

        for (i, test) in tests_should_err.iter().enumerate() {
            let result = validate_blk(
                &test.manifest,
                &test.sys_fs_devs,
                test.sys_fs_ready_devs.clone(),
                test.sys_lvms.clone(),
            );

            if result.is_ok() {
                eprintln!(
                    "Unexpected ok result from test case {}: {}",
                    i + 1,
                    test.case
                );

                if let Some(ref ctx) = test.context {
                    eprintln!("\nCONTEXT: {ctx}\n");
                }

                eprintln!("Test structure: {test:?}");
            }

            assert!(result.is_err());
        }
    }
}
