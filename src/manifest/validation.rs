use std::collections::{HashMap, HashSet, LinkedList};
use std::process::Command;

use serde::{Deserialize, Serialize};
use toml;

use crate::errors::AyiError;
use crate::manifest::{Dm, Manifest};
use crate::utils::fs::file_exists;
use crate::utils::shell::in_path;

pub fn validate(manifest: &Manifest) -> Result<(), AyiError> {
    validate_blk(&manifest, "blkid", "lvs", "pvs")?;

    let mkfs_rootfs = &format!("mkfs.{}", manifest.rootfs.fs_type);
    if !in_path(mkfs_rootfs) {
        return Err(AyiError::BadManifest(format!(
            "no such program to create rootfs: {mkfs_rootfs}"
        )));
    }

    for archfs in manifest.filesystems.iter() {
        let mkfs_cmd = &format!("mkfs.{}", archfs.fs_type);
        if !in_path(mkfs_cmd) {
            let device = &archfs.device;

            return Err(AyiError::BadManifest(format!(
                "no such program to create filesystem for device {device}: {mkfs_cmd}"
            )));
        }
    }

    let zone_info = format!("/usr/share/zoneinfo/{}", manifest.timezone);
    if !file_exists(&zone_info) {
        return Err(AyiError::BadManifest(format!(
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
pub fn validate_blk(
    manifest: &Manifest,
    cmd_blkid: &str, // Allow override in tests
    cmd_lvs: &str,   // Allow override in tests
    cmd_pvs: &str,   // Allow override in tests
) -> Result<(), AyiError> {
    // A hash map of existing block device and its filesystems
    let mut existing_fs_devs = trace_existing_fs(cmd_blkid);
    // Get all paths of existing LVM devices.
    // Unknown disks are not tracked - only LVM devices and their bases.
    let mut existing_devs = trace_existing_lvms(cmd_lvs, cmd_pvs);

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
            return Err(AyiError::BadManifest(format!(
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
            let partition_name = format!("{partition_prefix}/{}", i + 1);

            if let Some(existing_fs) = existing_fs_devs.get(&partition_name) {
                return Err(AyiError::BadManifest(format!(
                    "partition {partition_name} is already used as {existing_fs}"
                )));
            }
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
            // (3) find its base device in existing_devs (if validated, update manifest_devs)
            // (4) find its base device as device file (file_exists)
            Dm::Luks(luks) => {
                let msg = "luks validation failed";

                let (luks_base_path, luks_path) =
                    (&luks.device, format!("/dev/mapper/{}", luks.name));

                if file_exists(&luks_path) {
                    return Err(AyiError::BadManifest(format!(
                        "{msg}: device {luks_path} already exists"
                    )));
                }

                if let Some(fs_type) = existing_fs_devs.get(luks_base_path) {
                    return Err(AyiError::BadManifest(format!(
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
                        return Err(AyiError::BadManifest(format!(
                            "duplicate luks {luks_path} in manifest"
                        )));
                    }

                    if !is_luks_base(&top_most.device_type) {
                        return Err(AyiError::BadManifest(format!(
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

                for (lvm_base, lists) in existing_devs.iter_mut() {
                    for list in lists {
                        let top_most = list
                            .back()
                            .expect("no back node in linked list from existing_devs");

                        if top_most.device.as_str() != luks_base_path {
                            continue;
                        }

                        if top_most.device_type == TYPE_LUKS {
                            return Err(AyiError::BadManifest(format!(
                                "{msg}: luks {luks_path} already exists"
                            )));
                        }

                        if !is_luks_base(&top_most.device_type) {
                            return Err(AyiError::BadManifest(format!(
                                "{msg}: luks base {} (itself is an LVM from {}) cannot have type {}",
                                luks_base_path, lvm_base, top_most.device_type
                            )));
                        }

                        list.push_back(BlockDev {
                            device: luks_path.clone(),
                            device_type: TYPE_LUKS,
                        });

                        // Copy list from existing_devs to manifest_devs
                        manifest_devs.insert(luks_base_path.clone(), list.clone());

                        continue 'validate_dm;
                    }
                }

                // TODO: This may introduce error if such file is not a proper block device.
                if !file_exists(luks_base_path) {
                    return Err(AyiError::NoSuchDevice(luks_base_path.to_string()));
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
                    // (3) find its base device in existing_devs (and make sure that there's no such existing PV)
                    // (4) find its base device as device file (file_exists)

                    if let Some(fs_type) = existing_fs_devs.get(pv_path) {
                        return Err(AyiError::BadManifest(format!(
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
                            return Err(AyiError::BadManifest(format!(
                                "{msg}: duplicate pv {pv_path} in manifest"
                            )));
                        }

                        if !is_pv_base(&top_most.device_type) {
                            return Err(AyiError::BadManifest(format!(
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

                    for lists in existing_devs.values_mut() {
                        for list in lists {
                            let top_most = list
                                .back()
                                .expect("no back node in linked list from existing_devs");

                            if top_most.device.as_str() != pv_path {
                                continue;
                            }

                            if top_most.device_type == TYPE_PV {
                                return Err(AyiError::BadManifest(format!(
                                    "{msg}: pv {pv_path} already exists"
                                )));
                            }

                            if !is_pv_base(&top_most.device_type) {
                                return Err(AyiError::BadManifest(format!(
                                    "{msg}: pv {} base cannot have type {}",
                                    pv_path, top_most.device_type,
                                )));
                            }

                            list.push_back(BlockDev {
                                device: pv_path.clone(),
                                device_type: TYPE_PV,
                            });

                            manifest_devs.insert(pv_path.clone(), list.clone());

                            continue 'validate_pv;
                        }
                    }

                    if !file_exists(pv_path) {
                        return Err(AyiError::NoSuchDevice(pv_path.clone()));
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
                    // (2) find its pv_base in existing_devs (and make sure that there's no such existing VG)
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
                                return Err(AyiError::BadManifest(format!(
                                    "{msg}: duplicate vg {} in manifest",
                                    vg.name,
                                )));
                            }

                            if top_most.device.as_str() != pv_base {
                                continue;
                            }

                            if !is_vg_base(&top_most.device_type) {
                                return Err(AyiError::BadManifest(format!(
                                    "{msg}: vg {} pv base {pv_base} cannot have type {}",
                                    vg.name, top_most.device_type,
                                )));
                            }

                            list.push_back(vg_dev.clone());

                            continue 'validate_vg_pv;
                        }

                        for lists in existing_devs.values_mut() {
                            for list in lists {
                                let top_most = list
                                    .back()
                                    .expect("no back node in linked list from existing_devs");

                                if *top_most == vg_dev {
                                    return Err(AyiError::BadManifest(format!(
                                        "{msg}: vg {} already exists",
                                        vg.name,
                                    )));
                                }

                                if top_most.device.as_str() != pv_base {
                                    continue;
                                }

                                if !is_vg_base(&top_most.device_type) {
                                    return Err(AyiError::BadManifest(format!(
                                        "{msg}: vg {} pv base {pv_base} cannot have type {}",
                                        vg.name, top_most.device_type
                                    )));
                                }

                                list.push_back(vg_dev.clone());
                                manifest_devs.insert(pv_base.clone(), list.clone());

                                continue 'validate_vg_pv;
                            }
                        }

                        return Err(AyiError::BadManifest(format!(
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
                            return Err(AyiError::BadManifest(format!(
                                "{msg}: duplicate lv {lv_name} in manifest"
                            )));
                        }

                        if top_most.device != vg_name {
                            continue;
                        }

                        if !is_lv_base(&top_most.device_type) {
                            return Err(AyiError::BadManifest(format!(
                                "{msg}: lv {lv_name} vg base {vg_name} cannot have type {}",
                                top_most.device_type
                            )));
                        }

                        list.push_back(lv_dev);

                        continue 'validate_lv;
                    }

                    for (base, lists) in existing_devs.iter_mut() {
                        for list in lists {
                            let top_most = list
                                .back()
                                .expect("no back node for linked list in existing_devs");

                            if *top_most == lv_dev {
                                return Err(AyiError::BadManifest(format!(
                                    "{msg}: lv {lv_name} already exists"
                                )));
                            }

                            if top_most.device != vg_name {
                                continue;
                            }

                            if !is_lv_base(&top_most.device_type) {
                                return Err(AyiError::BadManifest(format!(
                                    "{msg}: lv {lv_name} vg base {vg_name} cannot have type {}",
                                    top_most.device_type
                                )));
                            }

                            list.push_back(lv_dev);
                            manifest_devs.insert(base.clone(), list.clone());

                            continue 'validate_lv;
                        }
                    }

                    return Err(AyiError::BadManifest(format!(
                        "{msg}: no vg device matching {vg_name} in manifest or in the system"
                    )));
                }
            }
        }
    }

    // Holds a tuple of string (block device name)
    // and its suitability to host a filesystem (true).
    let mut fs_ready_devs = HashSet::<(String, bool)>::new();

    // TODO: trace fs_ready_devs
    for fs_dev in existing_fs_devs.keys() {
        fs_ready_devs.insert((fs_dev.clone(), true));
    }

    let mut msg = "rootfs validation failed";
    for list in manifest_devs.values_mut() {
        let top_most = list
            .back()
            .expect("no back node in linked list from manifest_devs");

        let is_fs_ready = is_fs_base(&top_most.device_type);

        // If device was already in use on the system as filesystem
        if !fs_ready_devs.insert((top_most.device.clone(), is_fs_ready)) {
            let existing_fs = existing_fs_devs
                .get(&top_most.device)
                .expect("missing device in fs_devs");

            return Err(AyiError::BadManifest(format!(
                "{msg}: filesystem device {} is already used as {existing_fs}",
                top_most.device
            )));
        }
    }

    if fs_ready_devs.contains(&(manifest.rootfs.device.clone(), true)) {
        return Err(AyiError::BadManifest(format!(
            "{msg}: no top-level device ready for rootfs: {}",
            manifest.rootfs.device,
        )));
    }

    existing_fs_devs.insert(
        manifest.rootfs.device.clone(),
        BlockDevType::Fs(manifest.rootfs.fs_type.clone()),
    );

    msg = "fs validation failed";
    for (i, fs) in manifest.filesystems.iter().enumerate() {
        if !fs_ready_devs.contains(&(fs.device.clone(), true)) {
            return Err(AyiError::BadManifest(format!(
                "{msg}: no device {} for fs #{} ({})",
                manifest.rootfs.device,
                i + 1,
                fs.fs_type,
            )));
        }

        existing_fs_devs.insert(fs.device.clone(), BlockDevType::Fs(fs.fs_type.clone()));
    }

    msg = "swap validation failed";
    if let Some(devices) = manifest.swap.clone() {
        for (i, device) in devices.into_iter().enumerate() {
            // dev_type already has some filesystem
            if let Some(BlockDevType::Fs(fs_type)) = existing_fs_devs.get(&device) {
                if fs_type.as_str() == "swap" {
                    println!("found duplicate swap device {device}, ignoring");

                    continue;
                }

                return Err(AyiError::BadManifest(format!(
                    "{msg}: swap device {device} already contains fs {fs_type}"
                )));
            }

            if fs_ready_devs.contains(&(device.clone(), true)) {
                existing_fs_devs.insert(device.clone(), BlockDevType::Fs("swap".to_string()));

                continue;
            }

            // TODO: validate if that file can be used as swap
            if file_exists(&device) {
                continue;
            }

            return Err(AyiError::BadManifest(format!(
                "{msg}: manifest swap #{i} device {device} is not a valid swap device",
            )));
        }
    }

    Ok(())
}

// For parsing Linux blkid output
#[derive(Serialize, Deserialize)]
struct OutputBlkid {
    #[serde(rename = "UUID")]
    uuid: String,

    #[serde(rename = "TYPE")]
    dev_type: String,

    #[serde(rename = "LABEL")]
    label: Option<String>,
}

// Trace existing block devices with filesystems. Non-FS devices will be omitted.
fn trace_existing_fs(blkid_cmd: &str) -> HashMap<String, BlockDevType> {
    let cmd_blkid = Command::new(blkid_cmd)
        .output()
        .expect("failed to run `blkid`");

    let output_blkid = String::from_utf8(cmd_blkid.stdout).expect("output is not utf-8");
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

        let dev_data: Vec<&str> = line_elems[1].split_whitespace().collect();
        let dev_data = dev_data.join("\n");

        let dev_data: OutputBlkid =
            toml::from_str(&dev_data).expect("failed to unmarshal blkid output");

        match dev_data.dev_type.as_str() {
            "squashfs" | "LVM2_member" | "dos" | "gpt" => {
                continue;
            }
            _ => {
                fs.insert(dev_name.to_string(), BlockDevType::Fs(dev_data.dev_type));
            }
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

    let traced = trace_existing_fs("./mock_cmd/blkid");
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
