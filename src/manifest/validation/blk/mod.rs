mod dm;
mod trace_blk;

use std::collections::{HashMap, HashSet, LinkedList};

use crate::entity::{blockdev::*, parse_human_bytes};
use crate::errors::AliError;
use crate::manifest::{Dm, Manifest};
use crate::utils::fs::file_exists;

pub fn validate(manifest: &Manifest, overwrite: bool) -> Result<BlockDevPaths, AliError> {
    // Validate no duplicate mountpoints
    if let Some(ref filesystems) = manifest.filesystems {
        let mut known_mountpoints = HashSet::new();
        for fs in filesystems {
            if fs.mnt.is_none() {
                continue;
            }

            let mnt = fs.mnt.clone().unwrap();
            if !known_mountpoints.insert(mnt.clone()) {
                return Err(AliError::BadManifest(format!(
                    "duplicate mountpoints {mnt}"
                )));
            }
        }
    }

    let paths = match overwrite {
        true => {
            // Overwrite disk devices - we don't need to trace any existing devices,
            // as all devices required must already be in the manifest
            validate_blk(
                &manifest,
                &HashMap::<String, BlockDevType>::new(),
                HashMap::<String, BlockDevType>::new(),
                HashMap::<String, BlockDevPaths>::new(),
            )?;

            BlockDevPaths::new()
        }

        false => {
            // Get full blkid output
            let output_blkid = trace_blk::run_blkid("blkid")?;

            // A hash map of existing block device that can be used as filesystem base
            let sys_fs_ready_devs = trace_blk::sys_fs_ready(&output_blkid);

            // A hash map of existing block device and its filesystems
            let sys_fs_devs = trace_blk::sys_fs(&output_blkid);

            // Get all paths of existing LVM devices.
            // Unknown disks are not tracked - only LVM devices and their bases.
            let sys_lvms = trace_blk::sys_lvms("lvs", "pvs");

            validate_blk(&manifest, &sys_fs_devs, sys_fs_ready_devs, sys_lvms)?
        }
    };

    Ok(paths)
}

// Validates manifest block storage.
// sys_fs_ready_devs and sys_lvms are copied from caller,
// and are made mutable because we need to remove used up elements.
fn validate_blk(
    manifest: &Manifest,
    sys_fs_devs: &HashMap<String, BlockDevType>, // Maps fs devs to their FS type (e.g. Btrfs)
    mut sys_fs_ready_devs: HashMap<String, BlockDevType>, // Maps fs-ready devs to their types (e.g. partition)
    mut sys_lvms: HashMap<String, BlockDevPaths>,         // Maps pv path to all possible LV paths
) -> Result<BlockDevPaths, AliError> {
    // valids collects all valid known devices to be created in the manifest
    let mut valids = BlockDevPaths::new();

    if let Some(disks) = &manifest.disks {
        for disk in disks {
            if !file_exists(&disk.device) {
                return Err(AliError::BadManifest(format!(
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

            // Find if this disk has any used partitions
            // A GPT table can hold a maximum of 128 partitions
            for i in 1_u8..128 {
                let partition_name = format!("{partition_prefix}{i}");
                if sys_fs_devs.contains_key(&partition_name) {
                    let fs = sys_fs_devs.get(&partition_name).unwrap();
                    return Err(AliError::BadManifest(format!(
                        "disk {} already in use on {partition_name} as {fs}",
                        disk.device
                    )));
                }
            }

            // Base disk
            let base = LinkedList::from([BlockDev {
                device: disk.device.clone(),
                device_type: TYPE_DISK,
            }]);

            // Check if this partition is already in use
            let msg = "partition validation failed";

            let l = disk.partitions.len();
            for (i, part) in disk.partitions.iter().enumerate() {
                let partition_name = format!("{partition_prefix}{}", i + 1);

                // If multiple partitions are to be created on this disk,
                // only the last partition could be unsized
                if i != l - 1 && l != 1 {
                    if part.size.is_none() {
                        return Err(AliError::BadManifest(format!(
                            "unsized partition {partition_name} must be the last partition"
                        )));
                    }
                }

                if sys_fs_ready_devs.get(&partition_name).is_some() {
                    return Err(AliError::BadManifest(format!(
                        "{msg}: partition {partition_name} already exists on system"
                    )));
                }

                if let Some(existing_fs) = sys_fs_devs.get(&partition_name) {
                    return Err(AliError::BadManifest(format!(
                        "{msg}: partition {partition_name} is already used as {existing_fs}"
                    )));
                }

                if let Some(ref size) = part.size {
                    if let Err(err) = parse_human_bytes(size) {
                        return Err(AliError::BadManifest(format!(
                            "bad partition size {size}: {err}"
                        )));
                    }
                }

                let mut partition = base.clone();
                partition.push_back(BlockDev {
                    device: partition_name,
                    device_type: TYPE_PART,
                });

                valids.push(partition);
            }
        }
    }

    if let Some(dms) = &manifest.device_mappers {
        // Validate sizing of LVs
        // Only the last LV on each VG could be unsized (100%FREE)
        dm::validate_lv_size(dms)?;

        // Collect all DMs into valids to be used later in filesystems validation
        for dm in dms {
            match dm {
                Dm::Luks(luks) => {
                    // Appends LUKS to a path in valids, if OK
                    dm::collect_valid_luks(
                        luks,
                        sys_fs_devs,
                        &mut sys_fs_ready_devs,
                        &mut sys_lvms,
                        &mut valids,
                    )?;
                }

                // We validate a LVM manifest block by adding valid devices in these exact order:
                // PV -> VG -> LV
                // This gives us certainty that during VG validation, any known PV would have been in valids.
                Dm::Lvm(lvm) => {
                    if let Some(pvs) = &lvm.pvs {
                        for pv_path in pvs {
                            // Appends PV to a path in valids, if OK
                            dm::collect_valid_pv(
                                pv_path,
                                sys_fs_devs,
                                &mut sys_fs_ready_devs,
                                &mut sys_lvms,
                                &mut valids,
                            )?;
                        }
                    }

                    if let Some(vgs) = &lvm.vgs {
                        for vg in vgs {
                            // Appends VG to paths in valids, if OK
                            dm::collect_valid_vg(vg, sys_fs_devs, &mut sys_lvms, &mut valids)?;
                        }
                    }

                    if let Some(lvs) = &lvm.lvs {
                        for lv in lvs {
                            // Appends LV to paths in valids, if OK
                            dm::collect_valid_lv(lv, sys_fs_devs, &mut sys_lvms, &mut valids)?;
                        }
                    }
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

        return Err(AliError::AliRsBug(format!(
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
    for list in &valids {
        let top_most = list.back().expect("v is missing top-most device");
        if is_fs_base(&top_most.device_type) {
            fs_ready_devs.insert(top_most.device.clone());
        }
    }

    // Validate root FS, other FS, and swap against fs_ready_devs
    let mut msg = "rootfs validation failed";
    if !fs_ready_devs.contains(&manifest.rootfs.device.clone()) {
        return Err(AliError::BadManifest(format!(
            "{msg}: no top-level fs-ready device for rootfs: {}",
            manifest.rootfs.device,
        )));
    }

    // Remove used up fs-ready device
    fs_ready_devs.remove(&manifest.rootfs.device);

    if let Some(filesystems) = &manifest.filesystems {
        msg = "fs validation failed";
        for (i, fs) in filesystems.iter().enumerate() {
            if !fs_ready_devs.contains(&fs.device) {
                return Err(AliError::BadManifest(format!(
                    "{msg}: device {} for fs #{} ({}) is not fs-ready",
                    fs.device,
                    i + 1,
                    fs.fs_type,
                )));
            }

            // Remove used up fs-ready device
            fs_ready_devs.remove(&fs.device);
        }
    }

    msg = "swap validation failed";
    if let Some(ref swaps) = manifest.swap {
        for (i, swap) in swaps.iter().enumerate() {
            if fs_ready_devs.contains(swap) {
                fs_ready_devs.remove(swap);
                continue;
            }

            return Err(AliError::BadManifest(format!(
                "{msg}: device {swap} for swap #{} is not fs-ready",
                i + 1,
            )));
        }
    }

    Ok(valids)
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

    #[derive(Debug)]
    struct Test {
        case: String,
        context: Option<String>, // Extra info about the test
        manifest: Manifest,
        sys_fs_ready_devs: Option<HashMap<String, BlockDevType>>,
        sys_fs_devs: Option<HashMap<String, BlockDevType>>,
        sys_lvms: Option<HashMap<String, BlockDevPaths>>,
    }

    #[test]
    fn test_validate_blk() {
        let tests_should_ok = vec![
            Test {
                case: "Root and swap on existing partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/sda1".into(), TYPE_PART),
                    ("/dev/nvme0n1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/sda1".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on existing LV, swap on existing partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    TYPE_PART,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
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
                )])),

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LUKS on existing partition, swap on existing LV".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    TYPE_PART,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
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
                )])),

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: Some(vec![
                        Dm::Luks(ManifestLuks {
                            device: "/dev/nvme0n1p2".into(),
                            name:  "cryptroot".into(),
                        }),
                    ]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/mapper/cryptroot".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/myvg/mylv".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LUKS on existing LV, swap on LUKS on existing partition".into(),
                context: Some("Existing LV on VG on >1 PVs".into()),
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    TYPE_PART,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
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
                ), (
                    "/dev/sdb2".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/sdb2".into(),
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
                )])),

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: Some(vec![
                        Dm::Luks(ManifestLuks {
                            device: "/dev/myvg/mylv".into(),
                            name:  "cryptroot".into(),
                        }),
                        Dm::Luks(ManifestLuks {
                            device: "/dev/nvme0n1p2".into(),
                            name:  "cryptswap".into(),
                        })
                    ]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/mapper/cryptroot".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/mapper/cryptswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LUKS on existing LV, swap on LUKS on existing partition".into(),
                context: Some("Existing LV on VG on >1 existing + new PVs".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    (
                        "/dev/nvme0n1p2".into(),
                        TYPE_PART,
                    ),
                    (
                        "/dev/sdb2".into(),
                        TYPE_PART,
                    ),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/sda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/sda1".into(),
                            device_type: TYPE_PV,
                        },
                    ])],
                )])),

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: Some(vec![
                        Dm::Lvm(ManifestLvm {
                            pvs: Some(vec![
                                "/dev/sdb2".into(),
                            ]),
                            vgs: Some(vec![ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec![
                                    "/dev/sda1".into(), // sys_lvm PV
                                    "/dev/sdb2".into(), // new PV
                                ]
                            }]),
                            lvs: Some(vec![ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: None,
                            }]),
                        }),
                        Dm::Luks(ManifestLuks {
                            device: "/dev/myvg/mylv".into(),
                            name:  "cryptroot".into(),
                        }),
                        Dm::Luks(ManifestLuks {
                            device: "/dev/nvme0n1p2".into(),
                            name:  "cryptswap".into(),
                        })
                    ]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/mapper/cryptroot".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/mapper/cryptswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on existing LV, swap on manifest partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/sda1".into(), TYPE_PART),
                    ("/dev/nvme0n1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
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
                )])),

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root and swap on existing LV on existing VG".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/sda1".into(), TYPE_PART),
                    ("/dev/nvme0n1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
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
                )])),

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: None,
                        vgs: None,
                        lvs: Some(vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                    case: "Root on manifest LVM, built on existing partition. Swap on existing partition".into(),
                    context: None,
                    sys_fs_ready_devs: Some(HashMap::from([
                        ("/dev/sda1".into(), TYPE_PART),
                        ("/dev/nvme0n1p2".into(), TYPE_PART),
                    ])),
                    sys_fs_devs: None,
                    sys_lvms: None,

                    manifest: Manifest {
                        location: None,
                        disks: None,
                        device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                            pvs: Some(vec!["/dev/sda1".into()]),
                            vgs: Some(vec![ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec!["/dev/sda1".into()],
                            }]),
                            lvs: Some(vec![ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: None,
                            }]),
                        })]),
                        rootfs: ManifestRootFs(ManifestFs {
                            device: "/dev/myvg/mylv".into(),
                            mnt: None,
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                            mnt_opts: None,
                        }),
                        filesystems: None,
                        swap: Some(vec!["/dev/nvme0n1p2".into()]),
                        pacstraps: None,
                        chroot: None,
                        postinstall: None,
                        hostname: None,
                        timezone: None,
                    },
                },

                Test {
                    case:"Root on manifest LVM, built on manifest partition. Swap on manifest partition".into(),
                    context: None,
                    sys_fs_ready_devs: Some(HashMap::from([
                        ("/dev/nvme0n1p2".into(), TYPE_PART),
                    ])),
                    sys_fs_devs: None,
                    sys_lvms: None,

                    manifest: Manifest {
                        location: None,
                        disks: Some(vec![ManifestDisk {
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
                        }]),
                        device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                            pvs: Some(vec!["./mock_devs/sda2".into()]),
                            vgs: Some(vec![ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec!["./mock_devs/sda2".into()],
                            }]),
                            lvs: Some(vec![ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: None,
                            }]),
                        })]),
                        rootfs: ManifestRootFs(ManifestFs {
                            device: "/dev/myvg/mylv".into(),
                            mnt: None,
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                            mnt_opts: None,
                        }),
                        filesystems: None,
                        swap: Some(vec!["/dev/nvme0n1p2".into()]),
                        pacstraps: None,
                        chroot: None,
                        postinstall: None,
                        hostname: None,
                        timezone: None,
                    },
                },

                Test {
                    case: "Root on manifest LVM on manifest partition/existing partition. Swap on manifest partition".into(),
                    context: None,
                    sys_fs_ready_devs: Some(HashMap::from([
                        ("/dev/nvme0n1p1".into(), TYPE_PART),
                        ("/dev/nvme0n1p2".into(), TYPE_PART),
                    ])),
                    sys_fs_devs: None,
                    sys_lvms: None,

                    manifest: Manifest {
                        location: None,
                        disks: Some(vec![
                            ManifestDisk {
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
                            },
                        ]),
                        device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                            pvs: Some(vec![
                                "./mock_devs/sda2".into(),
                                "/dev/nvme0n1p1".into(),
                            ]),
                            vgs: Some(vec![ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec![
                                    "./mock_devs/sda2".into(),
                                    "/dev/nvme0n1p1".into(),
                                ],
                            }]),
                            lvs: Some(vec![ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: None,
                            }]),
                        })]),
                        rootfs: ManifestRootFs(ManifestFs {
                            device: "/dev/myvg/mylv".into(),
                            mnt: None,
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                            mnt_opts:None,
                        }),
                        filesystems: None,
                        swap: Some(vec!["/dev/nvme0n1p2".into()]),
                        pacstraps: None,
                        chroot: None,
                        postinstall: None,
                        hostname: None,
                        timezone: None,
                    },
                },

                Test {
                    case: "Root on manifest LVM, built on manifest/existing partition. Swap on manifest partition".into(),
                    context: None,
                    sys_fs_ready_devs: Some(HashMap::from([
                        ("/dev/nvme0n1p1".into(), TYPE_PART),
                        ("/dev/nvme0n1p2".into(), TYPE_PART),
                    ])),
                    sys_fs_devs: None,
                    sys_lvms: None,

                    manifest: Manifest {
                        location: None,
                        disks: Some(vec![
                            ManifestDisk {
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
                            },
                            ManifestDisk {
                                device: "./mock_devs/sdb".into(),
                                table: PartitionTable::Mbr,
                                partitions: vec![
                                    ManifestPartition {
                                        label: "PART_PV2".into(),
                                        size: None,
                                        part_type: "8e".into(),
                                    }
                                ]
                            },
                        ]),
                        device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                            pvs: Some(vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                            ]),
                            vgs: Some(vec![ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec![
                                    "./mock_devs/sda2".into(),
                                    "./mock_devs/sdb1".into(),
                                    "/dev/nvme0n1p2".into(),
                                ],
                            }]),
                            lvs: Some(vec![ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: None,
                            }]),
                        })]),
                        rootfs: ManifestRootFs(ManifestFs {
                            device: "/dev/myvg/mylv".into(),
                            mnt: None,
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                            mnt_opts: None,
                        }),
                        filesystems: None,
                        swap: Some(vec!["/dev/nvme0n1p1".into()]),
                        pacstraps: None,
                        chroot: None,
                        postinstall: None,
                        hostname: None,
                        timezone: None,
                    },
                },

                Test {
                    case: "Root and Swap on manifest LVs from the same VG".into(),
                    context: Some("2 LVs on 1 VGs - VGs on 3 PVs".into()),
                    sys_fs_ready_devs: Some(HashMap::from([
                        ("/dev/nvme0n1p1".into(), TYPE_PART),
                        ("/dev/nvme0n1p2".into(), TYPE_PART)],
                    )),
                    sys_fs_devs: None,
                    sys_lvms: None,

                    manifest: Manifest {
                        location: None,
                        disks: Some(vec![
                            ManifestDisk {
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
                            },
                            ManifestDisk {
                                device: "./mock_devs/sdb".into(),
                                table: PartitionTable::Mbr,
                                partitions: vec![
                                    ManifestPartition {
                                        label: "PART_PV2".into(),
                                        size: None,
                                        part_type: "8e".into(),
                                    }
                                ]
                            },
                        ]),
                        device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                            pvs: Some(vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                            ]),
                            vgs: Some(vec![ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec![
                                    "./mock_devs/sda2".into(),
                                    "./mock_devs/sdb1".into(),
                                    "/dev/nvme0n1p2".into(),
                                ],
                            }]),
                            lvs: Some(vec![
                                ManifestLvmLv {
                                    name: "myswap".into(),
                                    vg: "myvg".into(),
                                    size: Some("8G".into()),
                                },
                                ManifestLvmLv {
                                    name: "mylv".into(),
                                    vg: "myvg".into(),
                                    size: None,
                                },
                            ]),
                        })]),
                        rootfs: ManifestRootFs(ManifestFs {
                            device: "/dev/myvg/mylv".into(),
                            mnt: None,
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                            mnt_opts: None,
                        }),
                        filesystems: None,
                        swap: Some(vec!["/dev/myvg/myswap".into()]),
                        pacstraps: None,
                        chroot: None,
                        postinstall: None,
                        hostname: None,
                        timezone: None,
                    },
                },

                Test {
                    case: "Root and Swap on manifest LVs from the same VG".into(),
                    context: Some("2 LVs on 1 VG on 4 PVs. One of the PV already exists".into()),
                    sys_fs_ready_devs: Some(HashMap::from([
                        ("/dev/nvme0n1p1".into(), TYPE_PART),
                        ("/dev/nvme0n1p2".into(), TYPE_PART),
                    ])),
                    sys_fs_devs: None,
                    sys_lvms: Some(HashMap::from([
                        ("/dev/nvme0n2p7".into(), vec![
                            LinkedList::from(
                                [BlockDev { device: "/dev/nvme0n2p7".into(), device_type: TYPE_PV }],
                            ),
                        ]),
                    ])),

                    manifest: Manifest {
                        location: None,
                        disks: Some(vec![
                            ManifestDisk {
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
                            },
                            ManifestDisk {
                                device: "./mock_devs/sdb".into(),
                                table: PartitionTable::Mbr,
                                partitions: vec![
                                    ManifestPartition {
                                        label: "PART_PV2".into(),
                                        size: None,
                                        part_type: "8e".into(),
                                    }
                                ]
                            },
                        ]),
                        device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                            pvs: Some(vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                            ]),
                            vgs: Some(vec![ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec![
                                    "./mock_devs/sda2".into(),
                                    "./mock_devs/sdb1".into(),
                                    "/dev/nvme0n1p2".into(),
                                    "/dev/nvme0n2p7".into(),
                                ],
                            }]),
                            lvs: Some(vec![
                                ManifestLvmLv {
                                    name: "myswap".into(),
                                    vg: "myvg".into(),
                                    size: Some("8G".into()),
                                },
                                ManifestLvmLv {
                                    name: "mylv".into(),
                                    vg: "myvg".into(),
                                    size: None,
                                }
                            ]),
                        })]),
                        rootfs: ManifestRootFs(ManifestFs {
                            device: "/dev/myvg/mylv".into(),
                            mnt: None,
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                            mnt_opts: None,
                        }),
                        filesystems: None,
                        swap: Some(vec!["/dev/myvg/myswap".into()]),
                        pacstraps: None,
                        chroot: None,
                        postinstall: None,
                        hostname: None,
                        timezone: None,
                    },
                },

                Test {
                    case: "Multiple LVs on multiple VGs on multiple PVs".into(),
                    context: Some("3 LVs on 2 VGs, each VG on 2 PVs - one PV already exists".into()),
                    sys_fs_ready_devs: Some(HashMap::from([
                        ("/dev/nvme0n1p1".into(), TYPE_PART),
                        ("/dev/nvme0n1p2".into(), TYPE_PART),
                    ])),
                    sys_fs_devs: None,
                    sys_lvms: Some(HashMap::from([(
                        "/dev/nvme0n2p7".into(),
                        vec![LinkedList::from([BlockDev {
                            device: "/dev/nvme0n2p7".into(),
                            device_type: TYPE_PV,
                        }])],
                    )])),

                    manifest: Manifest {
                        location: None,
                        disks: Some(vec![
                            ManifestDisk {
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
                            },
                            ManifestDisk {
                                device: "./mock_devs/sdb".into(),
                                table: PartitionTable::Mbr,
                                partitions: vec![ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }],
                            },
                        ]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ]),
                        vgs: Some(vec![
                            ManifestLvmVg {
                                name: "mysatavg".into(),
                                pvs: vec!["./mock_devs/sda2".into(), "./mock_devs/sdb1".into()],
                            },
                            ManifestLvmVg {
                                name: "mynvmevg".into(),
                                pvs: vec!["/dev/nvme0n1p2".into(), "/dev/nvme0n2p7".into()],
                            },
                        ]),
                        lvs: Some(vec![
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
                        ]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/mysatavg/rootlv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: Some(vec![ManifestFs {
                        device: "/dev/mysatavg/datalv".into(),
                        mnt: Some("/opt/data".into()),
                        fs_type: "xfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }]),
                    swap: Some(vec!["/dev/mynvmevg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },
        ];

        let tests_should_err: Vec<Test> = vec![
            Test {
                case: "No manifest disks, root on non-existent, swap on non-existent".into(),
                context: None,
                sys_fs_ready_devs: None,
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/sda1".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "No manifest disks, root on existing ext4 fs, swap on non-existent".into(),
                context: None,
                sys_fs_ready_devs: None,
                sys_fs_devs: Some(HashMap::from([(
                    "/dev/sda1".into(),
                    BlockDevType::Fs("btrfs".into()),
                )])),
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/sda1".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LUKS on existing LV, but swap reuses rootfs base".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Partition,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
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
                )])),

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: Some(vec![
                        Dm::Luks(ManifestLuks {
                            device: "/dev/nvme0n1p2".into(),
                            name:  "cryptroot".into(),
                        }),
                    ]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/mapper/cryptroot".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LUKS on existing LV, swap on used-up LV".into(),
                context: Some("Existing LV on VG on >1 PVs".into()),
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Partition,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
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
                ), (
                    "/dev/sdb2".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/sdb2".into(),
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
                )])),

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: Some(vec![
                        Dm::Luks(ManifestLuks {
                            device: "/dev/myvg/mylv".into(),
                            name:  "cryptroot".into(),
                        }),
                    ]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/mapper/cryptroot".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/myvg/mylv".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions, but missing LV manifest".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Partition,
                )])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
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
                    }]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into()],
                        }]),
                        lvs: None,
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("Non-last partition has None size".into()),
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Partition,
                )])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./mock_devs/sda".into(),
                            table: PartitionTable::Gpt,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_EFI".into(),
                                    size: None,
                                    part_type: "ef".into(),
                                },
                                ManifestPartition {
                                    label: "PART_PV".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                },
                            ],
                    }]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into()],
                        }]),
                        lvs: Some(vec![
                            ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: Some("10G".into()),
                            },
                            ManifestLvmLv {
                                name: "myswap".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("Last partition has bad size (decimal)".into()),
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Partition,
                )])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
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
                                    size: Some("5.6T".into()),
                                    part_type: "8e".into(),
                                },
                            ],
                    }]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into()],
                        }]),
                        lvs: Some(vec![
                            ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: Some("10G".into()),
                            },
                            ManifestLvmLv {
                                name: "myswap".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("Non-last partition has bad size".into()),
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Partition,
                )])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./mock_devs/sda".into(),
                            table: PartitionTable::Gpt,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_EFI".into(),
                                    size: Some("5 gigabytes".into()),
                                    part_type: "ef".into(),
                                },
                                ManifestPartition {
                                    label: "PART_PV".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                },
                            ],
                    }]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into()],
                        }]),
                        lvs: Some(vec![
                            ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: Some("10G".into()),
                            },
                            ManifestLvmLv {
                                name: "myswap".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("Non-last LV has None size".into()),
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Partition,
                )])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
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
                    }]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into()],
                        }]),
                        lvs: Some(vec![
                            ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                            ManifestLvmLv {
                                name: "myswap".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("LV has bad size".into()),
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Partition,
                )])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
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
                    }]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into()],
                        }]),
                        lvs: Some(vec![
                            ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: Some("5G".into()),
                            },
                            ManifestLvmLv {
                                name: "myswap".into(),
                                vg: "myvg".into(),
                                size: Some("500.1G".into()),
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("Non-last LV has bad size".into()),
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Partition,
                )])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
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
                    }]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into()],
                        }]),
                        lvs: Some(vec![
                            ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: Some("5 gigabytes".into()),
                            },
                            ManifestLvmLv {
                                name: "myswap".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("VG is based on used PV".into()),
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Partition,
                )])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
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
                    }]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec!["./mock_devs/sda2".into()]),
                        vgs: Some(vec![
                            ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec!["./mock_devs/sda2".into()],
                            },
                            ManifestLvmVg {
                                name: "somevg".into(),
                                pvs: vec!["./mock_devs/sda2".into()],
                            },
                        ]),
                        lvs: None,
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on LVM, built on manifest partitions, but 1 fs is re-using rootfs LV".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/nvme0n1p2".into(),
                    BlockDevType::Partition,
                )])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
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
                        },
                    ]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec!["./mock_devs/sda2".into()]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./mock_devs/sda2".into()],
                        }]),
                        lvs: Some(vec![
                            ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/myvg.mylv".into(),
                            mnt: Some("/data".into()),
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                            mnt_opts: None,
                        },
                    ]),
                    swap: Some(vec!["/dev/nvme0n1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

             Test {
                case: "Root on manifest LVM, built on manifest partitions and existing partition. Swap on manifest partition that was used to build PV".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from(
                    [("/dev/nvme0n1p1".into(), TYPE_PART), ("/dev/nvme0n1p2".into(), TYPE_PART)],
                )),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
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
                        },
                        ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ]
                        }]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                            ],
                        }]),
                        lvs: Some(vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p2".into()]), // Was already used as manifest PV
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root on manifest LVM, built on manifest partitions and non-existent partition. Swap on manifest partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([("/dev/nvme0n1p1".into(), TYPE_PART)])),
                sys_fs_devs: None,
                sys_lvms: None,
                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
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
                        },
                        ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ]
                        },
                    ]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                            ],
                        }]),
                        lvs: Some(vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/nvme0n1p1".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root and Swap on manifest LVs from the same VG, but existing VG partition already has fs".into(),
                context: Some("2 LVs on 1 VG on 4 PVs, but 1 PV already has swap".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/nvme0n1p1".into(), TYPE_PART),
                    ("/dev/nvme0n1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: Some(HashMap::from([
                    ("/dev/nvme0n2p7".into(), BlockDevType::Fs("swap".into())),
                ])),
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
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
                        },
                        ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                },
                            ]
                        },
                    ]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                                "/dev/nvme0n2p7".into(),
                            ],
                        }]),
                        lvs: Some(vec![
                        ManifestLvmLv {
                            name: "myswap".into(),
                            vg: "myvg".into(),
                            size: Some("8G".into()),
                        },
                        ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },

            Test {
                case: "Root and Swap on manifest LVs from the same VG".into(),
                context: Some("2 LVs on 1 VG on 4 PVs, but 1 PV was already used".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/nvme0n1p1".into(), TYPE_PART),
                    ("/dev/nvme0n1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([
                    ("/dev/nvme0n2p7".into(), vec![
                        LinkedList::from(
                            [
                                BlockDev { device: "/dev/nvme0n2p7".into(), device_type: TYPE_PV },
                                BlockDev { device: "/dev/sysvg".into(), device_type: TYPE_VG },
                            ],
                        ),
                    ]),
                ])),

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
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
                        },
                        ManifestDisk {
                            device: "./mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ]
                        },
                    ]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./mock_devs/sda2".into(),
                            "./mock_devs/sdb1".into(),
                            "/dev/nvme0n1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./mock_devs/sda2".into(),
                                "./mock_devs/sdb1".into(),
                                "/dev/nvme0n1p2".into(),
                                "/dev/nvme0n2p7".into(),
                            ],
                        }]),
                        lvs: Some(vec![
                        ManifestLvmLv {
                            name: "myswap".into(),
                            vg: "myvg".into(),
                            size: Some("8G".into()),
                        },
                        ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }]),
                    })]),
                    rootfs: ManifestRootFs(ManifestFs {
                        device: "/dev/myvg/mylv".into(),
                        mnt: None,
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    }),
                    filesystems: None,
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                },
            },
        ];

        for (i, test) in tests_should_ok.iter().enumerate() {
            let result = validate_blk(
                &test.manifest,
                &test.sys_fs_devs.clone().unwrap_or(HashMap::new()),
                test.sys_fs_ready_devs.clone().unwrap_or_default(),
                test.sys_lvms.clone().unwrap_or_default(),
            );

            if let Err(ref err) = result {
                eprintln!("Unexpected error from test case {}: {}", i + 1, test.case);

                if let Some(ref ctx) = test.context {
                    eprintln!("\nCONTEXT: {ctx}\n");
                }

                eprintln!("Test structure: {test:?}");
                eprintln!("Error: {err:?}");
            }

            assert!(result.is_ok());
        }

        for (i, test) in tests_should_err.iter().enumerate() {
            let result = validate_blk(
                &test.manifest,
                &test.sys_fs_devs.clone().unwrap_or_default(),
                test.sys_fs_ready_devs.clone().unwrap_or_default(),
                test.sys_lvms.clone().unwrap_or_default(),
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

                let paths = result.unwrap();
                let paths_json = serde_json::to_string(&paths).unwrap();

                eprintln!("Test structure: {test:?}");
                eprintln!("BlockDevPaths: {paths_json}");

                panic!("test_should_err did not return error")
            }
        }
    }
}
