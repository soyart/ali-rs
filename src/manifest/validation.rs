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
    let existing_fs_ready_devs = trace_existing_fs_ready(&output_blkid);

    // A hash map of existing block device and its filesystems
    let existing_fs_devs = trace_existing_fs(&output_blkid);

    // Get all paths of existing LVM devices.
    // Unknown disks are not tracked - only LVM devices and their bases.
    let existing_lvms = trace_existing_lvms("lvs", "pvs");

    validate_blk(
        &manifest,
        &existing_fs_ready_devs,
        &existing_fs_devs,
        &existing_lvms,
    )?;

    let mkfs_rootfs = &format!("mkfs.{}", manifest.rootfs.fs_type);
    if !in_path(mkfs_rootfs) {
        return Err(NayiError::BadManifest(format!(
            "no such program to create rootfs: {mkfs_rootfs}"
        )));
    }

    for archfs in manifest.filesystems.iter() {
        let mkfs_cmd = &format!("mkfs.{}", archfs.fs_type);
        if !in_path(mkfs_cmd) {
            let device = &archfs.device;

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
    // Disks or partitions
    DiskOrPart,

    // UnknownBlock is anything that be can build filesystem, LUKS, and LVM PV on
    UnknownBlock,

    // Software-defined storage
    Dm(DmType),

    // Filesystems
    Fs(String),
}

const TYPE_DISK: BlockDevType = BlockDevType::DiskOrPart;
const TYPE_UNKNOWN: BlockDevType = BlockDevType::UnknownBlock;
const TYPE_LUKS: BlockDevType = BlockDevType::Dm(DmType::Luks);
const TYPE_PV: BlockDevType = BlockDevType::Dm(DmType::Pv);
const TYPE_VG: BlockDevType = BlockDevType::Dm(DmType::Vg);
const TYPE_LV: BlockDevType = BlockDevType::Dm(DmType::Lv);

fn is_pv_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::DiskOrPart => true,
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
        BlockDevType::DiskOrPart => true,
        BlockDevType::UnknownBlock => true,
        BlockDevType::Dm(DmType::Lv) => true,
        _ => false,
    }
}

fn is_fs_base(dev_type: &BlockDevType) -> bool {
    match dev_type {
        BlockDevType::DiskOrPart => true,
        BlockDevType::UnknownBlock => true,
        BlockDevType::Dm(DmType::Luks) => true,
        BlockDevType::Dm(DmType::Lv) => true,
        _ => false,
    }
}

/// Validate storage defined in manifest.
fn validate_blk(
    manifest: &Manifest,
    existing_fs_ready_devs: &HashMap<String, BlockDevType>, // Maps device path to fs type
    existing_fs_devs: &HashMap<String, BlockDevType>,       // Maps device path to device type
    existing_lvms: &HashMap<String, Vec<LinkedList<BlockDev>>>, // Maps pv path to all possible LV paths
) -> Result<(), NayiError> {
    // manifest_devs tracks devices and their dependencies in the manifest,
    // with key being the lowest-level device known.
    //
    // If a manifest device uses a non-manifest device,
    // then add the whole linked list from start to top-most device
    //
    // The 1st item in the list is usually a manifest disk,
    // but it could also be some existing device from existing_devs
    let mut manifest_devs = HashMap::<String, LinkedList<BlockDev>>::new();

    // Collect all manifest disks into manifest_devs as base/entrypoint
    for disk in manifest.disks.iter() {
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

        // Check if this partition is already in use
        for (i, _) in disk.partitions.iter().enumerate() {
            let partition_name = format!("{partition_prefix}{}", i + 1);

            if let Some(existing_fs) = existing_fs_devs.get(&partition_name) {
                return Err(NayiError::BadManifest(format!(
                    "partition {partition_name} is already used as {existing_fs}"
                )));
            }

            manifest_devs.insert(
                partition_name.clone(),
                LinkedList::from([BlockDev {
                    device: partition_name,
                    device_type: BlockDevType::DiskOrPart,
                }]),
            );
        }

        let list = LinkedList::<BlockDev>::from([BlockDev {
            device: disk.device.to_string(),
            device_type: TYPE_DISK,
        }]);

        manifest_devs.insert(disk.device.clone(), list);
    }

    // Collect and validate ManifestDm,
    // in the order that they appear in the manifest.
    //
    // It first checks if the base device was in manifest_devs,
    // and if not, it checks existing_devs.
    'validate_dm: for dm in manifest.dm.iter() {
        match dm {
            // Validate LUKS devices
            //
            // (1) check if it conflicts with existing_fs_devs
            // (2) find its base device in manifest_devs
            // (3) find its base device in existing_lvms (if validated, update manifest_devs)
            // (4) find its base device as device file (file_exists)
            Dm::Luks(luks) => {
                let msg = "luks validation failed";

                let (luks_base_path, luks_path) =
                    (&luks.device, format!("/dev/mapper/{}", luks.name));

                if file_exists(&luks_path) {
                    return Err(NayiError::BadManifest(format!(
                        "{msg}: device {luks_path} already exists"
                    )));
                }

                if let Some(fs_type) = existing_fs_devs.get(luks_base_path) {
                    return Err(NayiError::BadManifest(format!(
                        "{msg}: luks {} base {luks_base_path} was already in use as {fs_type}",
                        luks.name
                    )));
                }

                for list in manifest_devs.values_mut() {
                    let top_most = list
                        .back()
                        .expect("no back node in linked list from manifest_devs");

                    if top_most.device.as_str() != luks_base_path {
                        continue;
                    }

                    if top_most.device_type == TYPE_LUKS {
                        return Err(NayiError::BadManifest(format!(
                            "duplicate luks {luks_path} in manifest"
                        )));
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

                for (lvm_base, lists) in existing_lvms.iter() {
                    for list in lists {
                        let top_most = list
                            .back()
                            .expect("no back node in linked list from existing_devs");

                        if top_most.device.as_str() != luks_base_path {
                            continue;
                        }

                        if top_most.device_type == TYPE_LUKS {
                            return Err(NayiError::BadManifest(format!(
                                "{msg}: luks {luks_path} already exists"
                            )));
                        }

                        if !is_luks_base(&top_most.device_type) {
                            return Err(NayiError::BadManifest(format!(
                                "{msg}: luks base {} (itself is an LVM from {}) cannot have type {}",
                                luks_base_path, lvm_base, top_most.device_type
                            )));
                        }

                        // Copy and update list
                        // from existing_lvms to manifest_devs
                        let mut list = list.clone();
                        list.push_back(BlockDev {
                            device: luks_path.clone(),
                            device_type: TYPE_LUKS,
                        });
                        manifest_devs.insert(luks_base_path.clone(), list);

                        continue 'validate_dm;
                    }
                }

                // TODO: This may introduce error if such file is not a proper block device.
                if !file_exists(luks_base_path) {
                    return Err(NayiError::NoSuchDevice(luks_base_path.to_string()));
                }

                let luks_base_dev = BlockDev {
                    device: luks_base_path.clone(),
                    device_type: TYPE_UNKNOWN,
                };

                let luks_dev = BlockDev {
                    device: luks_path,
                    device_type: TYPE_LUKS,
                };

                let list = LinkedList::from([luks_base_dev, luks_dev]);

                manifest_devs.insert(luks_base_path.to_string(), list);
            }

            // Validate LVM devices from PVs -> VGs -> LVs
            Dm::Lvm(lvm) => {
                let mut msg = "lvm pv validation failed";

                'validate_pv: for pv_path in lvm.pvs.iter() {
                    // Validate LVM PV devices
                    //
                    // (1) check if it conflicts with existing_fs_devs
                    // (2) find its base device in manifest_devs
                    // (3) find its base device in existing_lvms (and make sure that there's no such existing PV)
                    // (4) find its base device as device file (file_exists)

                    if let Some(fs_type) = existing_fs_devs.get(pv_path) {
                        return Err(NayiError::BadManifest(format!(
                            "{msg}: pv {pv_path} base was already used as {fs_type}",
                        )));
                    }

                    for list in manifest_devs.values_mut() {
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

                    for lists in existing_lvms.values() {
                        for list in lists {
                            let top_most = list
                                .back()
                                .expect("no back node in linked list from existing_devs");

                            if top_most.device.as_str() != pv_path {
                                continue;
                            }

                            if top_most.device_type == TYPE_PV {
                                return Err(NayiError::BadManifest(format!(
                                    "{msg}: pv {pv_path} already exists"
                                )));
                            }

                            if !is_pv_base(&top_most.device_type) {
                                return Err(NayiError::BadManifest(format!(
                                    "{msg}: pv {} base cannot have type {}",
                                    pv_path, top_most.device_type,
                                )));
                            }

                            let mut list = list.clone();
                            list.push_back(BlockDev {
                                device: pv_path.clone(),
                                device_type: TYPE_PV,
                            });
                            manifest_devs.insert(pv_path.clone(), list);

                            continue 'validate_pv;
                        }
                    }

                    if existing_fs_ready_devs.contains_key(pv_path) {
                        manifest_devs.insert(
                            pv_path.clone(),
                            LinkedList::from([
                                BlockDev {
                                    device: pv_path.to_string(),
                                    device_type: TYPE_UNKNOWN,
                                },
                                BlockDev {
                                    device: pv_path.to_string(),
                                    device_type: TYPE_PV,
                                },
                            ]),
                        );

                        continue 'validate_pv;
                    }

                    if !file_exists(pv_path) {
                        return Err(NayiError::BadManifest(format!(
                            "{msg}: no such pv device: {pv_path}"
                        )));
                    }

                    // TODO: This may introduce error if such file is not a proper block device.
                    let pv_base_dev = BlockDev {
                        device: pv_path.clone(),
                        device_type: TYPE_UNKNOWN,
                    };

                    let pv_dev = BlockDev {
                        device: pv_path.clone(),
                        device_type: TYPE_PV,
                    };

                    let list = LinkedList::from([pv_base_dev, pv_dev]);

                    manifest_devs.insert(pv_path.clone(), list);
                }

                msg = "lvm vg validation failed";
                for vg in lvm.vgs.iter() {
                    // Validate LVM VG devices
                    //
                    // (1) find its pv_base in manifest_devs
                    // (2) find its pv_base in existing_lvms (and make sure that there's no such existing VG)
                    //
                    // Note: Error if an existing VG exists on the pv_base

                    let vg_dev = BlockDev {
                        device: format!("/dev/{}", vg.name),
                        device_type: TYPE_VG,
                    };

                    'validate_vg_pv: for pv_base in vg.pvs.iter() {
                        for list in manifest_devs.values_mut() {
                            let top_most = list
                                .back()
                                .expect("no back node in linked list from manifest_devs");

                            if *top_most == vg_dev {
                                return Err(NayiError::BadManifest(format!(
                                    "{msg}: duplicate vg {} in manifest",
                                    vg.name,
                                )));
                            }

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

                        for lists in existing_lvms.values() {
                            for list in lists {
                                let top_most = list
                                    .back()
                                    .expect("no back node in linked list from existing_devs");

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

                                let mut list = list.clone();
                                list.push_back(vg_dev.clone());
                                manifest_devs.insert(pv_base.clone(), list.clone());

                                continue 'validate_vg_pv;
                            }
                        }

                        return Err(NayiError::BadManifest(format!(
                            "{msg}: no pv device matching {pv_base} in manifest or in the system"
                        )));
                    }
                }

                msg = "lvm lv validation failed";
                'validate_lv: for lv in lvm.lvs.iter() {
                    // Validate LV devices
                    //
                    // (1) a known vg in manifest_devs
                    // (2) some existing vg in existing_devs

                    let vg_name = format!("/dev/{}", lv.vg);
                    let lv_name = format!("{vg_name}/{}", lv.name);

                    let lv_dev = BlockDev {
                        device: lv_name.clone(),
                        device_type: TYPE_LV,
                    };

                    for list in manifest_devs.values_mut() {
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

                        list.push_back(lv_dev);

                        continue 'validate_lv;
                    }

                    for (base, lists) in existing_lvms.iter() {
                        for list in lists {
                            let top_most = list
                                .back()
                                .expect("no back node for linked list in existing_devs");

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

                            let mut list = list.clone();
                            list.push_back(lv_dev);
                            manifest_devs.insert(base.clone(), list);

                            continue 'validate_lv;
                        }
                    }

                    return Err(NayiError::BadManifest(format!(
                        "{msg}: no vg device matching {vg_name} in manifest or in the system"
                    )));
                }
            }
        }
    }

    // Holds a tuple of string (block device name)
    // and its suitability to host a filesystem (true).
    //
    // We collect all ready devices into fs_ready_devs,
    // and then use it to validate manifest filesystems
    let mut fs_ready_devs = HashSet::<(String, bool)>::new();

    // Copy existing fs-ready devices from fs_ready_devs
    for sys_fs_dev in existing_fs_ready_devs.keys() {
        fs_ready_devs.insert((sys_fs_dev.clone(), true));
    }

    for lists in existing_lvms.values() {
        for list in lists {
            match list.back() {
                None => continue,
                Some(lvm_device) => match lvm_device.device_type {
                    TYPE_LV => {
                        fs_ready_devs.insert((lvm_device.device.to_string(), true));
                    }
                    _ => continue,
                },
            }
        }
    }

    let mut msg = "fs-ready device validation failed";
    for list in manifest_devs.values_mut() {
        let top_most = list
            .back()
            .expect("no back node in linked list from manifest_devs");

        let is_fs_ready = is_fs_base(&top_most.device_type);

        // If duplicate, then insertion will return false
        let duplicate = !fs_ready_devs.insert((top_most.device.clone(), is_fs_ready));
        if duplicate {
            // If device was already in use on the system as filesystem
            if let Some(existing_fs) = existing_fs_devs.get(&top_most.device) {
                return Err(NayiError::BadManifest(format!(
                    "{msg}: filesystem device {} is already used as {existing_fs}",
                    top_most.device,
                )));
            }
        }
    }

    msg = "rootfs validation failed";
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
    if let Some(devices) = manifest.swap.clone() {
        for (i, device) in devices.into_iter().enumerate() {
            // device already has some filesystem
            if let Some(BlockDevType::Fs(fs_type)) = existing_fs_devs.get(&device) {
                return Err(NayiError::BadManifest(format!(
                    "{msg}: swap device {device} on the system already contains fs {fs_type}"
                )));
            }

            if fs_ready_devs.contains(&(device.clone(), true)) {
                continue;
            }

            // TODO: validate if that file can be used as swap
            if file_exists(&device) {
                continue;
            }

            return Err(NayiError::BadManifest(format!(
                "{msg}: manifest swap #{} device {device} is not a valid swap device",
                i + 1,
            )));
        }
    }

    Ok(())
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

        fs_ready.insert(dev_name.to_string(), BlockDevType::DiskOrPart);
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
// Note: Takes in `lvs_cmd` and `pvs_cmd` to allow tests.
// TODO: Trace normal block devices too
fn trace_existing_lvms(lvs_cmd: &str, pvs_cmd: &str) -> HashMap<String, Vec<LinkedList<BlockDev>>> {
    let cmd_lvs = Command::new(lvs_cmd).output().expect("failed to run `lvs`");
    let output_lvs = String::from_utf8(cmd_lvs.stdout).expect("output is not utf-8");
    let lines_lvs: Vec<&str> = output_lvs.lines().skip(1).collect();

    let mut tmp = Vec::<LinkedList<BlockDev>>::new();

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

        let vg_name = format!("/dev/{vg_name}");
        let lv_name = format!("{vg_name}/{lv_name}");

        let vg = BlockDev {
            device: vg_name,
            device_type: BlockDevType::Dm(DmType::Vg),
        };

        let lv = BlockDev {
            device: lv_name.clone(),
            device_type: BlockDevType::Dm(DmType::Lv),
        };

        let list = LinkedList::<BlockDev>::from([lv, vg.clone()]);
        tmp.push(list);
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
        for t in &mut tmp.clone() {
            let vg_tmp = t.pop_back().expect("None vg_tmp");
            if vg_tmp == vg {
                let mut list = LinkedList::new();
                let lv_tmp = t.pop_back().expect("None lv_tmp");

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
            Self::DiskOrPart => write!(f, "DISK/PART"),
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
        expected_results.insert("/dev/vda2".to_string(), BlockDevType::DiskOrPart);

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
        manifest: Manifest,
        existing_fs_ready_devs: HashMap<String, BlockDevType>,
        existing_fs_devs: HashMap<String, BlockDevType>,
        existing_lvms: HashMap<String, Vec<LinkedList<BlockDev>>>,
    }

    #[test]
    fn test_validate_blk() {
        let tests_should_ok = vec![
            // Root on Btrfs /dev/sda1 (existing partition)
            // Swap on /dev/nvme0n1p2 (existing partition)
            Test {
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
                existing_fs_ready_devs: HashMap::from([
                    ("/dev/sda1".into(), BlockDevType::DiskOrPart),
                    ("/dev/nvme0n1p2".into(), BlockDevType::DiskOrPart),
                ]),
                existing_fs_devs: HashMap::new(),
                existing_lvms: HashMap::new(),
            },
            // Root on Btrfs /dev/myvg/mylv (existing LV)
            // Swap on /dev/nvme0n1p2 (existing partition)
            Test {
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
                existing_fs_ready_devs: HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::DiskOrPart,
                )]),
                existing_fs_devs: HashMap::new(),
                existing_lvms: HashMap::from([(
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
            // Root on Btrfs /dev/myvg/mylv (existing LV)
            // Swap on /dev/nvme0n1p2 (manifest partition)
            Test {
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
                existing_fs_ready_devs: HashMap::from([
                    ("/dev/sda1".into(), BlockDevType::DiskOrPart),
                    ("/dev/nvme0n1p2".into(), BlockDevType::DiskOrPart),
                ]),
                existing_fs_devs: HashMap::new(),
                existing_lvms: HashMap::from([(
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
            // Root on Btrfs /dev/myvg/mylv (manifest LVM on existing partition /dev/sda1)
            // Swap on /dev/nvme0n1p2 (existing partition)
            Test {
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
                existing_fs_ready_devs: HashMap::from([
                    ("/dev/sda1".into(), BlockDevType::DiskOrPart),
                    ("/dev/nvme0n1p2".into(), BlockDevType::DiskOrPart),
                ]),
                existing_fs_devs: HashMap::new(),
                existing_lvms: HashMap::new(),
            },
            // Root on Btrfs /dev/myvg/mylv (manifest LV on manifest partition)
            // Swap on /dev/nvme0n1p2 (manifest partition)
            Test {
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
                existing_fs_ready_devs: HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::DiskOrPart,
                )]),
                existing_fs_devs: HashMap::new(),
                existing_lvms: HashMap::new(),
            },
        ];

        let tests_should_err: Vec<Test> = vec![
            Test {
                // Root on /dev/sda2 (non-existent)
                // Swap on /dev/nvme0n1p1 (non-existent)
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
                existing_fs_ready_devs: HashMap::new(),
                existing_fs_devs: HashMap::new(),
                existing_lvms: HashMap::new(),
            },
            Test {
                // Root on /dev/sda2 (already used as ext4)
                // Swap on /dev/nvme0n1p1 (non-existent)
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
                existing_fs_ready_devs: HashMap::new(),
                existing_fs_devs: HashMap::from([(
                    "/dev/sda1".to_string(),
                    BlockDevType::Fs("btrfs".to_string()),
                )]),
                existing_lvms: HashMap::new(),
            },
        ];

        for (i, test) in tests_should_ok.iter().enumerate() {
            let result = validate_blk(
                &test.manifest,
                &test.existing_fs_ready_devs,
                &test.existing_fs_devs,
                &test.existing_lvms,
            );

            if result.is_err() {
                eprintln!("Unexpected error from test case {}: {:?}", i + 1, result);
            }

            assert!(result.is_ok());
        }

        for (i, test) in tests_should_err.iter().enumerate() {
            let result = validate_blk(
                &test.manifest,
                &test.existing_fs_ready_devs,
                &test.existing_fs_devs,
                &test.existing_lvms,
            );

            if result.is_ok() {
                eprintln!("Unexpected ok result from test case {}: {:?}", i + 1, test);
            }

            assert!(result.is_err());
        }
    }
}
