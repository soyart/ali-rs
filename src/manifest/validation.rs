use std::collections::{HashMap, HashSet, LinkedList};
use std::process::Command;

use serde::{Deserialize, Serialize};
use toml;

use crate::errors::NayiError;
use crate::manifest::{Dm, Manifest};
use crate::utils::fs::file_exists;
use crate::utils::shell::in_path;

pub fn validate(manifest: &Manifest) -> Result<(), NayiError> {
    // Get full blkid output
    let output_blkid = run_blkid("blkid")?;

    // A hash map of existing block device that can be directly
    // formatted with a filesystem
    let sys_fs_ready_devs = trace_existing_fs_ready(&output_blkid);

    // A hash map of existing block device and its filesystems
    let sys_fs_devs = trace_existing_fs(&output_blkid);

    // Get all paths of existing LVM devices.
    // Unknown disks are not tracked - only LVM devices and their bases.
    let sys_lvms = trace_existing_lvms("lvs", "pvs");

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
    Pv,
    Vg,
    Lv,
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
const TYPE_PV: BlockDevType = BlockDevType::Dm(DmType::Pv);
const TYPE_VG: BlockDevType = BlockDevType::Dm(DmType::Vg);
const TYPE_LV: BlockDevType = BlockDevType::Dm(DmType::Lv);

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
        BlockDevType::Dm(DmType::Pv) => true,
        _ => false,
    }
}

fn is_lv_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Dm(DmType::Vg) => true,
        _ => false,
    }
}

fn is_luks_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Disk => true,
        BlockDevType::Partition => true,
        BlockDevType::UnknownBlock => true,
        BlockDevType::Dm(DmType::Lv) => true,
        _ => false,
    }
}

fn is_fs_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::Disk => true,
        BlockDevType::Partition => true,
        BlockDevType::UnknownBlock => true,
        BlockDevType::Dm(DmType::Luks) => true,
        BlockDevType::Dm(DmType::Lv) => true,
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

            if let Some(existing_part) = sys_fs_ready_devs.get(&partition_name) {
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
                    let vg_dev = BlockDev {
                        device: format!("/dev/{}", vg.name),
                        device_type: TYPE_VG,
                    };

                    'validate_vg_pv: for pv_base in &vg.pvs {
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
    let mut fs_ready_devs = HashSet::<(String, bool)>::new();

    // Collect remaining sys_fs_ready_devs
    for (dev, dev_type) in sys_fs_ready_devs {
        fs_ready_devs.insert((dev, is_fs_base(&dev_type)));
    }

    // Collect remaining sys_lvms
    for sys_lvm_lists in sys_lvms.into_values() {
        for list in sys_lvm_lists {
            if let Some(top_most) = list.back() {
                fs_ready_devs.insert((top_most.device.clone(), is_fs_base(&top_most.device_type)));
            }
        }
    }

    for list in valids {
        let top_most = list.back().expect("v is missing top-most device");
        fs_ready_devs.insert((top_most.device.clone(), is_fs_base(&top_most.device_type)));
    }

    // Validate root FS, other FS, and swap against fs_ready_devs
    let mut msg = "rootfs validation failed";
    if !fs_ready_devs.contains(&(manifest.rootfs.device.clone(), true)) {
        return Err(NayiError::BadManifest(format!(
            "{msg}: no top-level fs-ready device for rootfs: {}",
            manifest.rootfs.device,
        )));
    }

    msg = "fs validation failed";
    for (i, fs) in manifest.filesystems.iter().enumerate() {
        if !fs_ready_devs.contains(&(fs.device.clone(), true)) {
            return Err(NayiError::BadManifest(format!(
                "{msg}: device {} for fs #{} ({}) is not fs-ready",
                fs.device,
                i + 1,
                fs.fs_type,
            )));
        }
    }

    msg = "swap validation failed";
    if let Some(ref swaps) = manifest.swap {
        for (i, swap) in swaps.iter().enumerate() {
            if is_fs_ready(&fs_ready_devs, swap.clone()) {
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

#[inline]
fn is_fs_ready(fs_ready_devs: &HashSet<(String, bool)>, device: String) -> bool {
    return fs_ready_devs.contains(&(device, true));
}

// For parsing Linux blkid output
#[derive(Serialize, Deserialize)]
struct EntryBlkid {
    #[serde(rename = "UUID")]
    uuid: Option<String>,

    #[serde(rename = "PARTUUID")]
    part_uuid: Option<String>,

    #[serde(rename = "TYPE")]
    dev_type: Option<String>,

    #[serde(rename = "LABEL")]
    label: Option<String>,
}

fn run_blkid(cmd_blkid: &str) -> Result<String, NayiError> {
    let cmd_blkid = Command::new(cmd_blkid).output().map_err(|err| {
        NayiError::CmdFailed(Some(err), format!("blkid command {cmd_blkid} failed"))
    })?;

    String::from_utf8(cmd_blkid.stdout).map_err(|err| {
        NayiError::NayiRsBug(format!("blkid output not string: {}", err.to_string()))
    })
}

fn trace_existing_fs_ready(output_blkid: &str) -> HashMap<String, BlockDevType> {
    let lines_blkid: Vec<&str> = output_blkid.lines().collect();

    let mut fs_ready = HashMap::new();
    for line in lines_blkid {
        if line.len() == 0 {
            continue;
        }

        let line_elems: Vec<&str> = line.split(':').collect();
        let dev_name = line_elems[0];

        // Make dev_data looks like TOML
        // KEY1=VAL1
        // KEY2=VAL2

        let dev_entry: Vec<&str> = line_elems[1].split_whitespace().collect();
        let dev_entry = dev_entry.join("\n");

        let dev_entry: EntryBlkid =
            toml::from_str(&dev_entry).expect("failed to unmarshal blkid output");

        // Non-LVM fs-ready devs should not have type yet
        if dev_entry.dev_type.is_some() {
            continue;
        }

        if dev_entry.part_uuid.is_none() {
            continue;
        }

        fs_ready.insert(dev_name.to_string(), BlockDevType::UnknownBlock);
    }

    fs_ready
}

// Trace existing block devices with filesystems. Non-FS devices will be omitted.
fn trace_existing_fs(output_blkid: &str) -> HashMap<String, BlockDevType> {
    let lines_blkid: Vec<&str> = output_blkid.lines().collect();

    let mut fs = HashMap::new();
    for line in lines_blkid {
        if line.len() == 0 {
            continue;
        }

        let line_elems: Vec<&str> = line.split(':').collect();
        let dev_name = line_elems[0];

        // Make dev_data looks like TOML
        // KEY1=VAL1
        // KEY2=VAL2

        let dev_entry: Vec<&str> = line_elems[1].split_whitespace().collect();
        let dev_entry = dev_entry.join("\n");

        let dev_entry: EntryBlkid =
            toml::from_str(&dev_entry).expect("failed to unmarshal blkid output");

        if let Some(dev_type) = dev_entry.dev_type {
            match dev_type.as_str() {
                "iso9660" | "LVM2_member" | "crypto_LUKS" | "squashfs" => continue,
                _ => fs.insert(dev_name.to_string(), BlockDevType::Fs(dev_type.to_string())),
            };
        }
    }

    fs
}

// Traces the LVM devices by listing all LVs and PVs,
// returning a hash map with key mapped to LVM PV name (as a disk),
// and values being paths from base -> pv -> vg -> lv.
//
// We trace LVM devices by first getting all LVs, then all PVs,
// and we construct VGs based on LVs and PVs
//
// Note: Takes in `lvs_cmd` and `pvs_cmd` to allow tests.
// TODO: New trace output schema
fn trace_existing_lvms(lvs_cmd: &str, pvs_cmd: &str) -> HashMap<String, Vec<LinkedList<BlockDev>>> {
    let cmd_lvs = Command::new(lvs_cmd).output().expect("failed to run `lvs`");
    let output_lvs = String::from_utf8(cmd_lvs.stdout).expect("output is not utf-8");
    let lines_lvs: Vec<&str> = output_lvs.lines().skip(1).collect();

    let mut lv_paths = Vec::<LinkedList<BlockDev>>::new();

    for line in lines_lvs {
        if line.len() == 0 {
            continue;
        }

        let line = line.split_whitespace().collect::<Vec<&str>>();

        if line.len() < 2 {
            continue;
        }

        if line[0] == "LV" {
            continue;
        }

        let lv_name = line.get(0).expect("missing 1st string on output");
        let vg_name = line.get(1).expect("missing 2nd string on output");

        lv_paths.push(LinkedList::<BlockDev>::from([
            BlockDev {
                device: format!("/dev/{vg_name}"),
                device_type: BlockDevType::Dm(DmType::Vg),
            },
            BlockDev {
                device: format!("{vg_name}/{lv_name}"),
                device_type: BlockDevType::Dm(DmType::Lv),
            },
        ]));
    }

    let cmd_pvs = Command::new(pvs_cmd).output().expect("failed to run `pvs`");

    let output_pvs = String::from_utf8(cmd_pvs.stdout).expect("output is not utf-8");
    let lines_pvs: Vec<&str> = output_pvs.lines().skip(1).collect();

    let mut lvms = HashMap::new();

    for line in lines_pvs {
        if line.len() == 0 {
            continue;
        }

        let line = line.split_whitespace().collect::<Vec<&str>>();

        if line.len() < 2 {
            continue;
        }

        if !line[0].starts_with('/') {
            continue;
        }

        let pv_name = line
            .get(0)
            .expect("missing 1st string on pvs output")
            .to_string();

        let vg_name = line.get(1).expect("missing 2nd string on pvs output");
        let vg_name = format!("/dev/{vg_name}");

        let pv_base = BlockDev {
            device: pv_name.clone(),
            device_type: TYPE_UNKNOWN,
        };

        let pv = BlockDev {
            device: pv_name.clone(),
            device_type: TYPE_PV,
        };

        let vg = BlockDev {
            device: vg_name.to_string(),
            device_type: TYPE_VG,
        };

        let mut lists = Vec::new();
        for lv_path in &mut lv_paths.clone() {
            let vg_tmp = lv_path.pop_back().expect("None vg_tmp");
            if vg_tmp == vg {
                let mut list = LinkedList::new();
                let lv_tmp = lv_path.pop_back().expect("None lv_tmp");

                list.push_back(pv_base.clone());
                list.push_back(pv.clone());
                list.push_back(vg_tmp);
                list.push_back(lv_tmp);

                lists.push(list);
            }
        }

        lvms.insert(pv_name.clone(), lists);
    }

    lvms
}

impl std::fmt::Display for DmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Luks => write!(f, "LUKS"),
            Self::Pv => write!(f, "LVM PV"),
            Self::Vg => write!(f, "LVM VG"),
            Self::Lv => write!(f, "LVM LV"),
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

    #[test]
    fn test_trace_existing_fs_ready() {
        let mut expected_results = HashMap::new();
        expected_results.insert("/dev/vda2".to_string(), BlockDevType::UnknownBlock);

        let output_blkid = run_blkid("./mock_cmd/blkid").expect("run_blkid failed");
        let traced = trace_existing_fs_ready(&output_blkid);
        for (k, v) in traced.into_iter() {
            let expected = expected_results.get(&k);

            assert!(expected.is_some());
            assert_eq!(expected.unwrap().clone(), v);
        }
    }

    #[test]
    fn test_trace_existing_fs() {
        // Hard-coded expected values from ./mock_cmd/blkid
        let mut expected_results = HashMap::new();
        expected_results.insert(
            "/dev/mapper/archvg-swaplv".to_string(),
            BlockDevType::Fs("swap".to_string()),
        );
        expected_results.insert(
            "/dev/mapper/archvg-rootlv".to_string(),
            BlockDevType::Fs("btrfs".to_string()),
        );

        let output_blkid = run_blkid("./mock_cmd/blkid").expect("run_blkid failed");
        let traced = trace_existing_fs(&output_blkid);
        for (k, v) in traced.into_iter() {
            let expected = expected_results.get(&k);
            assert!(expected.is_some());

            assert_eq!(expected.unwrap().clone(), v);
        }
    }

    #[test]
    fn test_trace_existing_lvms() {
        // Hard-coded expected values from ./mock_cmd/{lvs,pvs}
        let traced = trace_existing_lvms("./mock_cmd/lvs", "./mock_cmd/pvs");

        // Hard-coded expected values
        let lists_vda1 = vec![
            LinkedList::from([
                BlockDev {
                    device: "/dev/vda1".to_string(),
                    device_type: TYPE_UNKNOWN,
                },
                BlockDev {
                    device: "/dev/vda1".to_string(),
                    device_type: TYPE_PV,
                },
                BlockDev {
                    device: "/dev/archvg".to_string(),
                    device_type: TYPE_VG,
                },
                BlockDev {
                    device: "/dev/archvg/rootlv".to_string(),
                    device_type: TYPE_LV,
                },
            ]),
            LinkedList::from([
                BlockDev {
                    device: "/dev/vda1".to_string(),
                    device_type: TYPE_UNKNOWN,
                },
                BlockDev {
                    device: "/dev/vda1".to_string(),
                    device_type: TYPE_PV,
                },
                BlockDev {
                    device: "/dev/archvg".to_string(),
                    device_type: TYPE_VG,
                },
                BlockDev {
                    device: "/dev/archvg/swaplv".to_string(),
                    device_type: TYPE_LV,
                },
            ]),
        ];

        let lists_sda2 = vec![
            LinkedList::from([
                BlockDev {
                    device: "/dev/sda2".to_string(),
                    device_type: TYPE_UNKNOWN,
                },
                BlockDev {
                    device: "/dev/sda2".to_string(),
                    device_type: TYPE_PV,
                },
                BlockDev {
                    device: "/dev/archvg".to_string(),
                    device_type: TYPE_VG,
                },
                BlockDev {
                    device: "/dev/archvg/rootlv".to_string(),
                    device_type: TYPE_LV,
                },
            ]),
            LinkedList::from([
                BlockDev {
                    device: "/dev/sda2".to_string(),
                    device_type: TYPE_UNKNOWN,
                },
                BlockDev {
                    device: "/dev/sda2".to_string(),
                    device_type: TYPE_PV,
                },
                BlockDev {
                    device: "/dev/archvg".to_string(),
                    device_type: TYPE_VG,
                },
                BlockDev {
                    device: "/dev/archvg/swaplv".to_string(),
                    device_type: TYPE_LV,
                },
            ]),
        ];

        let lists_sda1 = vec![LinkedList::from([
            BlockDev {
                device: "/dev/sda1".to_string(),
                device_type: TYPE_UNKNOWN,
            },
            BlockDev {
                device: "/dev/sda1".to_string(),
                device_type: TYPE_PV,
            },
            BlockDev {
                device: "/dev/somevg".to_string(),
                device_type: TYPE_VG,
            },
            BlockDev {
                device: "/dev/somevg/datalv".to_string(),
                device_type: TYPE_LV,
            },
        ])];

        for (k, v) in traced {
            let mut expecteds = match k.as_str() {
                "/dev/vda1" => lists_vda1.clone(),
                "/dev/sda1" => lists_sda1.clone(),
                "/dev/sda2" => lists_sda2.clone(),
                _ => panic!("bad key {k}"),
            };

            for (i, list) in v.into_iter().enumerate() {
                let expected = expecteds
                    .get_mut(i)
                    .expect(&format!("no such expected list {i} for key {k}"));

                for (j, item) in list.into_iter().enumerate() {
                    let expected_item = expected.pop_front().expect(&format!(
                        "no such expected item {j} on list {i} for key {k}",
                    ));

                    assert_eq!(expected_item, item);
                }
            }

            println!();
        }
    }

    #[derive(Debug)]
    struct Test {
        case: String,
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
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/sda1".into(), BlockDevType::Disk),
                    ("/dev/nvme0n1p2".into(), BlockDevType::Disk),
                ]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),
            },
            Test {
                case: "Root on existing LV, swap on existing partition".into(),
                manifest: Manifest {
                    hostname: "foo".into(),
                    timezone: "foo".into(),
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
                },
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
            },
            Test {
                case: "Root on existing LV, swap on manifest partition".into(),
                manifest: Manifest {
                    hostname: "foo".into(),
                    timezone: "foo".into(),
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
                },
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
            },
            Test {
                case:
                    "Root on manifest LVM, built on existing partition. Swap on existing partition"
                        .into(),
                manifest: Manifest {
                    hostname: "foo".into(),
                    timezone: "foo".into(),
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
                },
                sys_fs_ready_devs: HashMap::from([
                    ("/dev/sda1".into(), BlockDevType::Disk),
                    ("/dev/nvme0n1p2".into(), BlockDevType::Disk),
                ]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),
            },
            Test {
                case:
                    "Root on manifest LVM, built on manifest partition. Swap on manifest partition"
                        .into(),
                manifest: Manifest {
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".to_string(),
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
                },
                sys_fs_ready_devs: HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Disk,
                )]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),
            },
            Test {
                case:
                    "Root on manifest LVM, built on manifest partition and existing partition. Swap on manifest partition"
                        .into(),
                manifest: Manifest {
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".to_string(),
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
                        pvs: vec!["./mock_devs/sda2".into(), "/dev/nvme0n1p1".into()],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into(), "/dev/nvme0n1p1".into()],
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
                sys_fs_ready_devs: HashMap::from([("/dev/nvme0n1p1".into(), TYPE_PART), ("/dev/nvme0n1p2".into(), TYPE_PART)]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),
            },
            Test {
                case:
                    "Root on manifest LVM, built on manifest partitions and existing partition. Swap on manifest partition"
                        .into(),
                manifest: Manifest {
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".to_string(),
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
                            device: "./mock_devs/sdb".to_string(),
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
                        pvs: vec!["./mock_devs/sda2".into(), "./mock_devs/sdb1".into(), "/dev/nvme0n1p2".into()],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into(), "./mock_devs/sdb1".into(), "/dev/nvme0n1p2".into()],
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
                },
                sys_fs_ready_devs: HashMap::from([("/dev/nvme0n1p1".into(), TYPE_PART), ("/dev/nvme0n1p2".into(), TYPE_PART)]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),
            },
        ];

        let tests_should_err: Vec<Test> = vec![
            Test {
                case: "No manifest disks, root on non-existent, swap on non-existent".into(),
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
                    swap: Some(vec!["/dev/nvme0n1p2".to_string()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                },
                sys_fs_ready_devs: HashMap::new(),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),
            },
            Test {
                case: "No manifest disks, root on existing ext4 fs, swap on non-existent".into(),
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
                    swap: Some(vec!["/dev/nvme0n1p2".to_string()]),
                    pacstraps: HashSet::new(),
                    chroot: None,
                    postinstall: None,
                },
                sys_fs_ready_devs: HashMap::new(),
                sys_fs_devs: HashMap::from([(
                    "/dev/sda1".to_string(),
                    BlockDevType::Fs("btrfs".to_string()),
                )]),
                sys_lvms: HashMap::new(),
            },
            // Root on Btrfs /dev/myvg/mylv (manifest LV on manifest partition, but missing LV)
            Test {
                case: "Root on LVM, built on manifest partitions, but missing LV manifest".into(),
                manifest: Manifest {
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".to_string(),
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
                },
                sys_fs_ready_devs: HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Disk,
                )]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),
            },
             Test {
                case:
                    "Root on manifest LVM, built on manifest partitions and existing partition. Swap on manifest partition that was used to build PV"
                        .into(),
                manifest: Manifest {
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".to_string(),
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
                            device: "./mock_devs/sdb".to_string(),
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
                        pvs: vec!["./mock_devs/sda2".into(), "./mock_devs/sdb1".into(), "/dev/nvme0n1p2".into()],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into(), "./mock_devs/sdb1".into(), "/dev/nvme0n1p2".into()],
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
                },
                sys_fs_ready_devs: HashMap::from([("/dev/nvme0n1p1".into(), TYPE_PART), ("/dev/nvme0n1p2".into(), TYPE_PART)]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),
            },
            Test {
                case:
                    "Root on manifest LVM, built on manifest partitions and non-existent partition. Swap on manifest partition"
                        .into(),
                manifest: Manifest {
                    hostname: "foo".into(),
                    timezone: "foo".into(),
                    disks: vec![ManifestDisk {
                        device: "./mock_devs/sda".to_string(),
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
                            device: "./mock_devs/sdb".to_string(),
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
                        pvs: vec!["./mock_devs/sda2".into(), "./mock_devs/sdb1".into(), "/dev/nvme0n1p2".into()],
                        vgs: vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into(), "./mock_devs/sdb1".into(), "/dev/nvme0n1p2".into()],
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
                },
                sys_fs_ready_devs: HashMap::from([("/dev/nvme0n1p1".into(), TYPE_PART)]),
                sys_fs_devs: HashMap::new(),
                sys_lvms: HashMap::new(),
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
                eprintln!("Test structure: {test:?}");
            }

            assert!(result.is_err());
        }
    }
}
