mod disk;
mod dm;
mod fs;
mod mount;
mod swap;
mod sysfs;
mod trace_blk;

use std::collections::{
    HashMap,
    HashSet,
};

use crate::ali::*;
use crate::errors::AliError;
use crate::types::blockdev::*;

/// Validates manifest for `stage_mountpoints`
/// See [`validate_blockdev`] for details.
///
/// If `overwrite` is false, `validate` passes zeroed valued
/// system state to `validate_blockdev`.
///
/// Otherwise, it collects the current system state as hash maps
/// and then pass those to `validate_blockdev`.
///
/// The system state hash maps are used to check the manifest items against,
/// to ensure that no instruction in the manifest would be able to modify
/// current partitions or filesystems on the disks.
pub(crate) fn validate(
    manifest: &Manifest,
    overwrite: bool,
) -> Result<BlockDevPaths, AliError> {
    // Empty state maps will bypass the checks, allowing ali-rs to wipe any
    // existing system resources which appear in the manifest.
    match overwrite {
        true => {
            validate_blockdev(
                manifest,
                &HashMap::<String, BlockDevType>::new(),
                HashMap::<String, BlockDevType>::new(),
                HashMap::<String, BlockDevPaths>::new(),
            )
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

            validate_blockdev(
                manifest,
                &sys_fs_devs,
                sys_fs_ready_devs,
                sys_lvms,
            )
        }
    }
}

/// Validates manifest block storage.
///
/// It first collects all valid system and manifest devices
/// into a list `valids`, returning error if found during collection.
///
/// If all names are successfully collected into `valids`,
/// `valids` is then used to construct `fs_ready_devs` and `fs_devs`.
/// All 3 variables are then used validate the block devices:
///
/// The parameters it takes are the current state of the system
/// before applying the manifest, which is used to ensure that
/// no system filesystems or partitions are modified during manifest application.
///
/// sys_fs_ready_devs and sys_lvms are copied from caller,
/// and are made mutable because we may need to modify their elements,
/// i.e. removing used up elements as we collect more devices.
fn validate_blockdev(
    manifest: &Manifest,
    sys_fs_devs: &HashMap<String, BlockDevType>, /* Maps fs devs to their FS type (e.g. Btrfs) */
    mut sys_fs_ready_devs: HashMap<String, BlockDevType>, /* Maps fs-ready devs to their types (e.g. partition) */
    mut sys_lvms: HashMap<String, BlockDevPaths>, /* Maps pv path to all possible LV paths */
) -> Result<BlockDevPaths, AliError> {
    // Valid block devices
    let valids = collect_valids(
        &manifest.disks,
        &manifest.device_mappers,
        sys_fs_devs,
        &mut sys_fs_ready_devs,
        &mut sys_lvms,
    )?;

    // Valid block devices that can be used as fs base (fs-ready)
    let mut fs_ready_devs =
        collect_fs_ready_devs(&mut sys_fs_ready_devs, sys_lvms, &valids)?;

    // Valid block devices used as filesystems
    let mut fs_devs =
        collect_fs_devs(manifest, sys_fs_devs, &mut fs_ready_devs)?;

    fs::validate_rootfs(
        &manifest.rootfs.device,
        &mut fs_ready_devs,
        &mut fs_devs,
    )?;

    fs_ready_devs.remove(&manifest.rootfs.device);

    if let Some(mountpoints) = &manifest.mountpoints {
        mount::validate_dups(mountpoints)?;
        mount::validate(mountpoints, &mut fs_devs)?;
    }

    if let Some(ref swaps) = manifest.swap {
        swap::validate(swaps, &mut fs_ready_devs)?;
    }

    Ok(valids)
}

fn collect_valids(
    disks: &Option<Vec<ManifestDisk>>,
    device_mappers: &Option<Vec<Dm>>,
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_fs_ready_devs: &mut HashMap<String, BlockDevType>,
    sys_lvms: &mut HashMap<String, BlockDevPaths>,
) -> Result<BlockDevPaths, AliError> {
    // valids collects all valid known devices to be created in the manifest.
    // The back of each linked list is the top-most device.
    let mut valids = BlockDevPaths::new();

    if let Some(disks) = &disks {
        disk::collect_valids(
            disks,
            sys_fs_devs,
            sys_fs_ready_devs,
            &mut valids,
        )?;
    }

    if let Some(dms) = &device_mappers {
        dm::collect_valids(
            dms,
            sys_fs_devs,
            sys_fs_ready_devs,
            sys_lvms,
            &mut valids,
        )?;
    }

    Ok(valids)
}

fn collect_fs_ready_devs(
    sys_fs_ready_devs: &mut HashMap<String, BlockDevType>,
    sys_lvms: HashMap<String, BlockDevPaths>,
    valids: &BlockDevPaths,
) -> Result<HashSet<String>, AliError> {
    // Valid block devices that can be used as fs base (fs-ready)
    let mut fs_ready_devs = HashSet::<String>::new();

    sysfs::collect_fs_ready_devs(sys_fs_ready_devs, &mut fs_ready_devs)?;
    sysfs::collect_lvm_fs_ready_devs(sys_lvms, &mut fs_ready_devs);

    // Collect fs-ready devices from valids to fs_ready_devs
    for list in valids {
        let dev = list.back().expect("`valids` is missing top-most device");
        if !is_fs_ready(&dev.device_type) {
            continue;
        }

        fs_ready_devs.insert(dev.device.clone());
    }

    Ok(fs_ready_devs)
}

fn collect_fs_devs<'a>(
    manifest: &'a Manifest,
    sys_fs_devs: &'a HashMap<String, BlockDevType>,
    fs_ready_devs: &'a mut HashSet<String>,
) -> Result<HashSet<String>, AliError> {
    let mut fs_devs = HashSet::new();

    sysfs::collect_fs_devs(sys_fs_devs, &mut fs_devs)?;

    if let Some(filesystems) = &manifest.filesystems {
        fs::collect_fs_devs(filesystems, fs_ready_devs, &mut fs_devs)?;
    }

    Ok(fs_devs)
}

fn is_fs_ready(dev_type: &BlockDevType) -> bool {
    matches!(
        dev_type,
        BlockDevType::Disk
            | BlockDevType::Partition
            | BlockDevType::UnknownBlock
            | BlockDevType::Dm(DmType::Luks)
            | BlockDevType::Dm(DmType::LvmLv)
    )
}

impl std::fmt::Display for DmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Luks => write!(f, "LUKS"),
            Self::LvmPv => write!(f, "LVM_PV"),
            Self::LvmVg => write!(f, "LVM_VG"),
            Self::LvmLv => write!(f, "LVM_LV"),
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
            Self::Fs(fs_type) => write!(f, "FS_{}", fs_type.to_uppercase()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::LinkedList;

    use super::*;

    #[derive(Debug)]
    struct TestCollectValids {
        manifest_disks: Vec<ManifestDisk>,
        manifest_dms: Vec<Dm>,
        sys_fs_devs: HashMap<String, BlockDevType>,
        sys_fs_ready_devs: HashMap<String, BlockDevType>,
        sys_lvms: HashMap<String, BlockDevPaths>,
        expected_valids: BlockDevPaths,
    }

    #[derive(Debug)]
    struct TestValidateBlockDev {
        case: String,
        context: Option<String>, // Extra info about the test
        manifest: Manifest,
        sys_fs_ready_devs: Option<HashMap<String, BlockDevType>>,
        sys_fs_devs: Option<HashMap<String, BlockDevType>>,
        sys_lvms: Option<HashMap<String, BlockDevPaths>>,
    }

    #[test]
    fn test_collect_valids() {
        let should_ok = vec![TestCollectValids {
            manifest_disks: vec![
                //
                ManifestDisk {
                    device: "./test_assets/mock_devs/sda".into(),
                    table: PartitionTable::Gpt,
                    partitions: vec![
                        //
                        ManifestPartition {
                            label: "ROOTFS".into(),
                            size: None,
                            part_type: "linux".into(),
                        },
                    ],
                },
            ],
            manifest_dms: vec![
                //
                Dm::Luks(ManifestLuks {
                    device: "./test_assets/mock_devs/sda1".into(),
                    name: "cryptroot".into(),
                    passphrase: None,
                }),
            ],
            sys_fs_devs: HashMap::new(),
            sys_fs_ready_devs: HashMap::from([]),
            sys_lvms: HashMap::new(),
            expected_valids: BlockDevPaths::from([
                //
                LinkedList::from([
                    //
                    BlockDev {
                        device: "./test_assets/mock_devs/sda".into(),
                        device_type: TYPE_DISK,
                    },
                    BlockDev {
                        device: "./test_assets/mock_devs/sda1".into(),
                        device_type: TYPE_PART,
                    },
                    BlockDev {
                        device: "/dev/mapper/cryptroot".into(),
                        device_type: TYPE_LUKS,
                    },
                ]),
            ]),
        }];

        for (i, t) in should_ok.into_iter().enumerate() {
            let result = collect_valids(
                &Some(t.manifest_disks),
                &Some(t.manifest_dms),
                &mut t.sys_fs_devs.clone(),
                &mut t.sys_fs_ready_devs.clone(),
                &mut t.sys_lvms.clone(),
            );

            if let Err(ref err) = result {
                eprintln!("unexpected error from case {}: {err}", i + 1);
            }

            assert!(result.is_ok());

            let valids = result.unwrap();
            let mut actual = HashSet::<BlockDevPath>::new();
            for p in valids.iter() {
                actual.insert(p.clone());
            }

            let mut expected = HashSet::<BlockDevPath>::new();
            for p in t.expected_valids.iter() {
                expected.insert(p.clone());
            }

            let diff: Vec<_> = actual.difference(&expected).collect();
            let diff_count = diff.len();

            if diff_count != 0 {
                eprintln!("unexpected result");
                eprintln!("expecting {expected:?}");
                eprintln!("actual {actual:?}");
            }

            assert_eq!(0, diff_count);
        }
    }

    #[test]
    fn test_validate_blk() {
        let should_ok = vec![
            TestValidateBlockDev {
                case: "Root and swap on existing partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fda1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs{
                        device: "/dev/fda1".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root FS on existing system device".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                ])),
                sys_fs_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), BlockDevType::Fs("ufs".into())),
                ])),
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs{
                        device: "/dev/fake1p1".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: None,
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on existing LV, swap on existing partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/fake1p2".into(),
                    TYPE_PART,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LUKS on existing partition, swap on existing LV".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/fake1p2".into(),
                    TYPE_PART,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                            device: "/dev/fake1p2".into(),
                            name:  "cryptroot".into(),
                            passphrase: None,
                        }),
                    ]),
                    rootfs: ManifestRootFs{
                        device: "/dev/mapper/cryptroot".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/myvg/mylv".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LUKS on existing partition, swap on existing LV, data on new LV".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/fake1p2".into(),
                    TYPE_PART,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                            device: "/dev/fake1p2".into(),
                            name:  "cryptroot".into(),
                            passphrase: None,
                        }),
                        Dm::Lvm(ManifestLvm {
                            pvs: None,
                            vgs: None,
                            lvs: Some(vec![
                                ManifestLvmLv{
                                    name: "datalv".into(),
                                    vg: "myvg".into(),
                                    size: None,
                                },
                            ]),
                        })
                    ]),
                    rootfs: ManifestRootFs{
                        device: "/dev/mapper/cryptroot".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/myvg/mylv".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LUKS on existing partition, mountpoint on existing LV".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/fake1p2".into(),
                    TYPE_PART,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                            device: "/dev/fake1p2".into(),
                            name:  "cryptroot".into(),
                            passphrase: None,
                        }),
                    ]),
                    rootfs: ManifestRootFs{
                        device: "/dev/mapper/cryptroot".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs{
                            device: "/dev/myvg/mylv".into(),
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: Some(vec![
                        ManifestMountpoint{
                            device: "/dev/myvg/mylv".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        },
                    ]),
                    swap: None,
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LUKS on existing LV, swap on LUKS on existing partition".into(),
                context: Some("Existing LV on VG on >1 PVs".into()),
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/fake1p2".into(),
                    TYPE_PART,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                    "/dev/fdb2".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fdb2".into(),
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
                            passphrase: None,
                        }),
                        Dm::Luks(ManifestLuks {
                            device: "/dev/fake1p2".into(),
                            name:  "cryptswap".into(),
                            passphrase: None,
                        })
                    ]),
                    rootfs: ManifestRootFs {
                        device: "/dev/mapper/cryptroot".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/mapper/cryptswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LUKS on existing LV, swap on LUKS on existing partition".into(),
                context: Some("Existing LV on VG on >1 existing + new PVs".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    (
                        "/dev/fake1p2".into(),
                        TYPE_PART,
                    ),
                    (
                        "/dev/fdb2".into(),
                        TYPE_PART,
                    ),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                                "/dev/fdb2".into(),
                            ]),
                            vgs: Some(vec![ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec![
                                    "/dev/fda1".into(), // sys_lvm PV
                                    "/dev/fdb2".into(), // new PV
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
                            passphrase: None,
                        }),
                        Dm::Luks(ManifestLuks {
                            device: "/dev/fake1p2".into(),
                            name:  "cryptswap".into(),
                            passphrase: None,
                        })
                    ]),
                    rootfs: ManifestRootFs{
                        device: "/dev/mapper/cryptroot".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/mapper/cryptswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on existing LV, swap on manifest partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fda1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root and swap on existing LV on existing VG".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fda1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on manifest LVM, built on existing partition. Swap on existing partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fda1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec!["/dev/fda1".into()]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["/dev/fda1".into()],
                        }]),
                        lvs: Some(vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case:"Root on manifest LVM, built on manifest partition. Swap on manifest partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![ManifestDisk {
                        device: "./test_assets/mock_devs/sda".into(),
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
                        pvs: Some(vec!["./test_assets/mock_devs/sda2".into()]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./test_assets/mock_devs/sda2".into()],
                        }]),
                        lvs: Some(vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on manifest LVM on manifest partition/existing partition. Swap on manifest partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "/dev/fake1p1".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "/dev/fake1p1".into(),
                            ],
                        }]),
                        lvs: Some(vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts:None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on manifest LVM, built on manifest/existing partition. Swap on manifest partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p2".into(),
                            ],
                        }]),
                        lvs: Some(vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p1".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root and Swap on manifest LVs from the same VG".into(),
                context: Some("2 LVs on 1 VGs - VGs on 3 PVs".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART)],
                )),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p2".into(),
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root, Data, and Swap on manifest LVs from the same VG".into(),
                context: Some("3 LVs on 1 VGs - VGs on 3 PVs".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART)],
                )),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p2".into(),
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
                                size: Some("10GB".into()),
                            },
                            ManifestLvmLv {
                                name: "mydata".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root, Data, and Swap on manifest LVs from the same VG, with 1 direct fs mount".into(),
                context: Some("3 LVs on 1 VGs - VGs on 3 PVs".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART)],
                )),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p1".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p1".into(),
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
                                size: Some("10GB".into()),
                            },
                            ManifestLvmLv {
                                name: "mydata".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/fake1p2".into(),
                            fs_type: "xfs".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: Some(vec![
                        ManifestMountpoint {
                            device: "/dev/fake1p2".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        },
                    ]),
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root, Data, and Swap on manifest LVs from the same VG, with 2 mounts (1 fs, 1 lv)".into(),
                context: Some("3 LVs on 1 VGs on 3 PVs, and 1 direct FS mount".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART)],
                )),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p1".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p1".into(),
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
                                size: Some("10GB".into()),
                            },
                            ManifestLvmLv {
                                name: "mydata".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/fake1p2".into(),
                            fs_type: "xfs".into(),
                            fs_opts: None,
                        },
                        ManifestFs {
                            device: "/dev/myvg/mydata".into(),
                            fs_type: "ext4".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: Some(vec![
                        ManifestMountpoint {
                            device: "/dev/fake1p2".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        },
                        ManifestMountpoint {
                            device: "/dev/myvg/mydata".into(),
                            dest: "/mydata".into(),
                            mnt_opts: None,
                        },
                    ]),
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root, Data, and Swap on manifest LVs from the same VG, with 2 LV mounts".into(),
                context: Some("3 LVs on 2 VGs on 4 PVs, and 2 LV mounts".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART)],
                )),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ],
                        },
                    ]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p1".into(),
                            "/dev/fake1p2".into(),
                        ]),
                        vgs: Some(vec![
                            ManifestLvmVg {
                                name: "datavg".into(),
                                pvs: vec![
                                    "./test_assets/mock_devs/sda2".into(),
                                    "./test_assets/mock_devs/sdb1".into(),
                                ],
                            },
                            ManifestLvmVg {
                                name: "sysvg".into(),
                                pvs: vec![
                                    "/dev/fake1p1".into(),
                                    "/dev/fake1p2".into(),
                                ],
                            },
                        ]),
                        lvs: Some(vec![
                            ManifestLvmLv {
                                name: "swaplv".into(),
                                vg: "sysvg".into(),
                                size: Some("8G".into()),
                            },
                            ManifestLvmLv {
                                name: "rootlv".into(),
                                vg: "sysvg".into(),
                                size: None,
                            },
                            ManifestLvmLv {
                                name: "data".into(),
                                vg: "datavg".into(),
                                size: Some("200GB".into()),
                            },
                            ManifestLvmLv {
                                name: "mydata".into(),
                                vg: "datavg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/sysvg/rootlv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/datavg/data".into(),
                            fs_type: "ext4".into(),
                            fs_opts: None,
                        },
                        ManifestFs {
                            device: "/dev/datavg/mydata".into(),
                            fs_type: "xfs".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: Some(vec![
                        ManifestMountpoint {
                            device: "/dev/datavg/data".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        },
                        ManifestMountpoint {
                            device: "/dev/datavg/mydata".into(),
                            dest: "/mydata".into(),
                            mnt_opts: None,
                        },
                    ]),
                    swap: Some(vec!["/dev/sysvg/swaplv".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root and Swap on manifest LVs from the same VG".into(),
                context: Some("2 LVs on 1 VG on 4 PVs. One of the PV already exists".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([
                    ("/dev/fake2p7".into(), vec![
                        LinkedList::from(
                            [BlockDev { device: "/dev/fake2p7".into(), device_type: TYPE_PV }],
                        ),
                    ]),
                ])),

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p2".into(),
                                "/dev/fake2p7".into(),
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Multiple LVs on multiple VGs on multiple PVs".into(),
                context: Some("3 LVs on 2 VGs, each VG on 2 PVs - one PV already exists".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fake2p7".into(),
                    vec![LinkedList::from([BlockDev {
                        device: "/dev/fake2p7".into(),
                        device_type: TYPE_PV,
                    }])],
                )])),

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                        "./test_assets/mock_devs/sda2".into(),
                        "./test_assets/mock_devs/sdb1".into(),
                        "/dev/fake1p2".into(),
                    ]),
                    vgs: Some(vec![
                        ManifestLvmVg {
                            name: "mysatavg".into(),
                            pvs: vec!["./test_assets/mock_devs/sda2".into(), "./test_assets/mock_devs/sdb1".into()],
                        },
                        ManifestLvmVg {
                            name: "mynvmevg".into(),
                            pvs: vec!["/dev/fake1p2".into(), "/dev/fake2p7".into()],
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
                rootfs: ManifestRootFs{
                    device: "/dev/mysatavg/rootlv".into(),
                    fs_type: "btrfs".into(),
                    fs_opts: None,
                    mnt_opts: None,
                },
                filesystems: Some(vec![
                    ManifestFs {
                        device: "/dev/mysatavg/datalv".into(),
                        fs_type: "xfs".into(),
                        fs_opts: None,
                    },
                ]),
                mountpoints: Some(vec![
                    ManifestMountpoint {
                        device: "/dev/mysatavg/datalv".into(),
                        dest: "/opt/data".into(),
                        mnt_opts: None,
                    },
                ]),
                swap: Some(vec![
                    "/dev/mynvmevg/myswap".into(),
                ]),
                pacstraps: None,
                chroot: None,
                postinstall: None,
                hostname: None,
                timezone: None,
                rootpasswd: None,
            },
        }];

        let should_err: Vec<TestValidateBlockDev> = vec![
            TestValidateBlockDev {
                case: "No manifest disks, root on non-existent, swap on non-existent".into(),
                context: None,
                sys_fs_ready_devs: None,
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs {
                        device: "/dev/fda1".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "No manifest disks, root on existing ext4 fs, swap on non-existent".into(),
                context: None,
                sys_fs_ready_devs: None,
                sys_fs_devs: Some(HashMap::from([(
                    "/dev/fake1p1".into(),
                    BlockDevType::Fs("btrfs".into()),
                )])),
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs{
                        device: "/dev/fake1p1".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Swap uses existing FS".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: Some(HashMap::from([
                    ("/dev/fake1p3".into(), BlockDevType::Fs("ufs".into())),
                ])),
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs{
                        device: "/dev/fake1p1".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p3".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Duplicate FS on root FS".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: Some(HashMap::from([
                    ("/dev/fake1p3".into(), BlockDevType::Fs("ufs".into())),
                ])),
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs{
                        device: "/dev/fake1p1".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/fake1p1".into(),
                            fs_type: "ext4".into(),
                            fs_opts: None,
                        }
                    ]),
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Duplicate FS on swap device".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: Some(HashMap::from([
                    ("/dev/fake1p3".into(), BlockDevType::Fs("ufs".into())),
                ])),
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs{
                        device: "/dev/fake1p1".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/fake1p2".into(),
                            fs_type: "ext4".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: None,
                    swap: Some(vec![
                        "/dev/fake1p2".into(),
                    ]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Duplicate FS on some device".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: Some(HashMap::from([
                    ("/dev/fake1p3".into(), BlockDevType::Fs("ufs".into())),
                ])),
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: None,
                    device_mappers: None,
                    rootfs: ManifestRootFs{
                        device: "/dev/fake1p1".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/fake1p2".into(),
                            fs_type: "ext4".into(),
                            fs_opts: None,
                        },
                        ManifestFs {
                            device: "/dev/fake1p2".into(),
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                        }
                    ]),
                    mountpoints: None,
                    swap: None,
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LUKS on existing LV, but swap reuses rootfs base".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                            device: "/dev/fake1p2".into(),
                            name:  "cryptroot".into(),
                            passphrase: None,
                        }),
                    ]),
                    rootfs: ManifestRootFs{
                        device: "/dev/mapper/cryptroot".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LUKS on existing LV, swap on used-up LV".into(),
                context: Some("Existing LV on VG on >1 PVs".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                    "/dev/fdb2".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fdb2".into(),
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
                            passphrase: None,
                        }),
                    ]),
                    rootfs: ManifestRootFs{
                        device: "/dev/mapper/cryptroot".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/myvg/mylv".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LUKS on existing partition, but 1 fs on rootfs device".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/fake1p2".into(),
                    TYPE_PART,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                            device: "/dev/fake1p2".into(),
                            name:  "cryptroot".into(),
                            passphrase: None,
                        }),
                    ]),
                    rootfs: ManifestRootFs{
                        device: "/dev/mapper/cryptroot".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs{
                            device: "/dev/mapper/cryptroot".into(),
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: None,
                    swap: None,
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LUKS on existing partition, but 1 mountpoint on rootfs".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([(
                    "/dev/fake1p2".into(),
                    TYPE_PART,
                )])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([(
                    "/dev/fda1".into(),
                    vec![LinkedList::from([
                        BlockDev {
                            device: "/dev/fda1".into(),
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
                            device: "/dev/fake1p2".into(),
                            name:  "cryptroot".into(),
                            passphrase: None,
                        }),
                    ]),
                    rootfs: ManifestRootFs{
                        device: "/dev/mapper/cryptroot".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs{
                            device: "/dev/myvg/mylv".into(),
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: Some(vec![
                        ManifestMountpoint{
                            device: "/dev/mapper/cryptroot".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        },
                    ]),
                    swap: None,
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LVM, built on manifest partitions, but 1 fs on rootfs device".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![
                            ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec!["./test_assets/mock_devs/sda2".into()],
                            },
                        ]),
                        lvs: Some(vec![
                            ManifestLvmLv{
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/myvg/mylv".into(),
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LVM, built on manifest partitions, but 1 mountpoint on rootfs".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![
                            ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec!["./test_assets/mock_devs/sda2".into()],
                            },
                        ]),
                        lvs: Some(vec![
                            ManifestLvmLv{
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: Some(vec![
                        ManifestMountpoint {
                            device: "/dev/myvg/mylv".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        },
                    ]),
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LVM, built on manifest partitions, but missing LV manifest".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./test_assets/mock_devs/sda2".into()],
                        }]),
                        lvs: None,
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("Non-last partition has None size".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./test_assets/mock_devs/sda2".into()],
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("Last partition has bad size (decimal)".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./test_assets/mock_devs/sda2".into()],
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("Non-last partition has bad size".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_UNKNOWN),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./test_assets/mock_devs/sda2".into()],
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("Non-last LV has None size".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./test_assets/mock_devs/sda2".into()],
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("LV has bad size".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./test_assets/mock_devs/sda2".into()],
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("Non-last LV has bad size".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./test_assets/mock_devs/sda2".into()],
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LVM, built on manifest partitions".into(),
                context: Some("VG is based on used PV".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART)
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                        pvs: Some(vec!["./test_assets/mock_devs/sda2".into()]),
                        vgs: Some(vec![
                            ManifestLvmVg {
                                name: "myvg".into(),
                                pvs: vec!["./test_assets/mock_devs/sda2".into()],
                            },
                            ManifestLvmVg {
                                name: "somevg".into(),
                                pvs: vec!["./test_assets/mock_devs/sda2".into()],
                            },
                        ]),
                        lvs: None,
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on LVM, built on manifest partitions, but 1 fs is re-using rootfs LV".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                        pvs: Some(vec!["./test_assets/mock_devs/sda2".into()]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec!["./test_assets/mock_devs/sda2".into()],
                        }]),
                        lvs: Some(vec![
                            ManifestLvmLv {
                                name: "mylv".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/myvg/mylv".into(),
                            fs_type: "btrfs".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: Some(vec![
                        ManifestMountpoint{
                            device: "/dev/myvg/mylv".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        }
                    ]),
                    swap: Some(vec!["/dev/fake1p2".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on manifest LVM, built on manifest partitions and existing partition. Swap on manifest partition that was used to build PV".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from(
                    [("/dev/fake1p1".into(), TYPE_PART), ("/dev/fake1p2".into(), TYPE_PART)],
                )),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p2".into(),
                            ],
                        }]),
                        lvs: Some(vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p2".into()]), // Was already used as manifest PV
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root on manifest LVM, built on manifest partitions and non-existent partition. Swap on manifest partition".into(),
                context: None,
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: None,
                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p2".into(),
                            ],
                        }]),
                        lvs: Some(vec![ManifestLvmLv {
                            name: "mylv".into(),
                            vg: "myvg".into(),
                            size: None,
                        }]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/fake1p1".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root and Swap on manifest LVs from the same VG, but existing VG partition already has fs".into(),
                context: Some("2 LVs on 1 VG on 4 PVs, but 1 PV already has swap".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: Some(HashMap::from([
                    ("/dev/fake2p7".into(), BlockDevType::Fs("swap".into())),
                ])),
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p2".into(),
                                "/dev/fake2p7".into(),
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root and Swap on manifest LVs from the same VG".into(),
                context: Some("2 LVs on 1 VG on 4 PVs, but 1 PV was already used".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART),
                ])),
                sys_fs_devs: None,
                sys_lvms: Some(HashMap::from([
                    ("/dev/fake2p7".into(), vec![
                        LinkedList::from(
                            [
                                BlockDev { device: "/dev/fake2p7".into(), device_type: TYPE_PV },
                                BlockDev { device: "/dev/sysvg".into(), device_type: TYPE_VG },
                            ],
                        ),
                    ]),
                ])),

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p2".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p2".into(),
                                "/dev/fake2p7".into(),
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
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: None,
                    mountpoints: None,
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root, Data, and Swap on manifest LVs from the same VG, with 2 mounts".into(),
                context: Some("3 LVs on 1 VGs - VGs on 3 PVs, but missing mountpoint device".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART)],
                )),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p1".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p1".into(),
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
                                size: Some("10GB".into()),
                            },
                            ManifestLvmLv {
                                name: "mydata".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/fake1p2".into(),
                            fs_type: "xfs".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: Some(vec![
                        ManifestMountpoint {
                            device: "/dev/fake1p2".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        },
                        ManifestMountpoint {
                            device: "/dev/fake1p9".into(),
                            dest: "/mydata".into(),
                            mnt_opts: None,
                        },
                    ]),
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root, Data, and Swap on manifest LVs from the same VG, with 2 mounts, but missing 1 FS for mountpoints".into(),
                context: Some("3 LVs on 1 VGs - VGs on 3 PVs, but missing 1 FS for mountpoint".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART)],
                )),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
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
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p1".into(),
                        ]),
                        vgs: Some(vec![ManifestLvmVg {
                            name: "myvg".into(),
                            pvs: vec![
                                "./test_assets/mock_devs/sda2".into(),
                                "./test_assets/mock_devs/sdb1".into(),
                                "/dev/fake1p1".into(),
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
                                size: Some("10GB".into()),
                            },
                            ManifestLvmLv {
                                name: "mydata".into(),
                                vg: "myvg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/myvg/mylv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/fake1p2".into(),
                            fs_type: "xfs".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: Some(vec![
                        ManifestMountpoint {
                            device: "/dev/myvg/mydata".into(),
                            dest: "/mydata".into(),
                            mnt_opts: None,
                        },
                        ManifestMountpoint {
                            device: "/dev/fake1p2".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        },
                    ]),
                    swap: Some(vec!["/dev/myvg/myswap".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root, Data, and Swap on manifest LVs from the same VG, with 2 LV mounts".into(),
                context: Some("3 LVs on 2 VGs on 4 PVs, and 2 LV mounts, but 1 LV is missing FS".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART)],
                )),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ],
                        },
                    ]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p1".into(),
                            "/dev/fake1p2".into(),
                        ]),
                        vgs: Some(vec![
                            ManifestLvmVg {
                                name: "datavg".into(),
                                pvs: vec![
                                    "./test_assets/mock_devs/sda2".into(),
                                    "./test_assets/mock_devs/sdb1".into(),
                                ],
                            },
                            ManifestLvmVg {
                                name: "sysvg".into(),
                                pvs: vec![
                                    "/dev/fake1p1".into(),
                                    "/dev/fake1p2".into(),
                                ],
                            },
                        ]),
                        lvs: Some(vec![
                            ManifestLvmLv {
                                name: "swaplv".into(),
                                vg: "sysvg".into(),
                                size: Some("8G".into()),
                            },
                            ManifestLvmLv {
                                name: "rootlv".into(),
                                vg: "sysvg".into(),
                                size: None,
                            },
                            ManifestLvmLv {
                                name: "data".into(),
                                vg: "datavg".into(),
                                size: Some("200GB".into()),
                            },
                            ManifestLvmLv {
                                name: "mydata".into(),
                                vg: "datavg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs {
                        device: "/dev/sysvg/rootlv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/datavg/data".into(),
                            fs_type: "ext4".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: Some(vec![
                        ManifestMountpoint {
                            device: "/dev/datavg/data".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        },
                        ManifestMountpoint {
                            device: "/dev/datavg/mydata".into(),
                            dest: "/mydata".into(),
                            mnt_opts: None,
                        },
                    ]),
                    swap: Some(vec!["/dev/sysvg/swaplv".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },

            TestValidateBlockDev {
                case: "Root, Data, and Swap on manifest LVs from the same VG, with 2 LV mounts".into(),
                context: Some("3 LVs on 2 VGs on 4 PVs, and 2 LV mounts, but duplicate mountpoints".into()),
                sys_fs_ready_devs: Some(HashMap::from([
                    ("/dev/fake1p1".into(), TYPE_PART),
                    ("/dev/fake1p2".into(), TYPE_PART)],
                )),
                sys_fs_devs: None,
                sys_lvms: None,

                manifest: Manifest {
                    location: None,
                    disks: Some(vec![
                        ManifestDisk {
                            device: "./test_assets/mock_devs/sda".into(),
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
                            device: "./test_assets/mock_devs/sdb".into(),
                            table: PartitionTable::Mbr,
                            partitions: vec![
                                ManifestPartition {
                                    label: "PART_PV2".into(),
                                    size: None,
                                    part_type: "8e".into(),
                                }
                            ],
                        },
                    ]),
                    device_mappers: Some(vec![Dm::Lvm(ManifestLvm {
                        pvs: Some(vec![
                            "./test_assets/mock_devs/sda2".into(),
                            "./test_assets/mock_devs/sdb1".into(),
                            "/dev/fake1p1".into(),
                            "/dev/fake1p2".into(),
                        ]),
                        vgs: Some(vec![
                            ManifestLvmVg {
                                name: "datavg".into(),
                                pvs: vec![
                                    "./test_assets/mock_devs/sda2".into(),
                                    "./test_assets/mock_devs/sdb1".into(),
                                ],
                            },
                            ManifestLvmVg {
                                name: "sysvg".into(),
                                pvs: vec![
                                    "/dev/fake1p1".into(),
                                    "/dev/fake1p2".into(),
                                ],
                            },
                        ]),
                        lvs: Some(vec![
                            ManifestLvmLv {
                                name: "swaplv".into(),
                                vg: "sysvg".into(),
                                size: Some("8G".into()),
                            },
                            ManifestLvmLv {
                                name: "rootlv".into(),
                                vg: "sysvg".into(),
                                size: None,
                            },
                            ManifestLvmLv {
                                name: "data".into(),
                                vg: "datavg".into(),
                                size: Some("200GB".into()),
                            },
                            ManifestLvmLv {
                                name: "mydata".into(),
                                vg: "datavg".into(),
                                size: None,
                            },
                        ]),
                    })]),
                    rootfs: ManifestRootFs{
                        device: "/dev/sysvg/rootlv".into(),
                        fs_type: "btrfs".into(),
                        fs_opts: None,
                        mnt_opts: None,
                    },
                    filesystems: Some(vec![
                        ManifestFs {
                            device: "/dev/datavg/data".into(),
                            fs_type: "ext4".into(),
                            fs_opts: None,
                        },
                        ManifestFs {
                            device: "/dev/datavg/mydata".into(),
                            fs_type: "xfs".into(),
                            fs_opts: None,
                        },
                    ]),
                    mountpoints: Some(vec![
                        ManifestMountpoint {
                            device: "/dev/datavg/data".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        },
                        ManifestMountpoint {
                            device: "/dev/datavg/mydata".into(),
                            dest: "/data".into(),
                            mnt_opts: None,
                        },
                    ]),
                    swap: Some(vec!["/dev/sysvg/swaplv".into()]),
                    pacstraps: None,
                    chroot: None,
                    postinstall: None,
                    hostname: None,
                    timezone: None,
                    rootpasswd: None,
                },
            },
        ];

        for (i, test) in should_ok.iter().enumerate() {
            let result = validate_blockdev(
                &test.manifest,
                &test.sys_fs_devs.clone().unwrap_or(HashMap::new()),
                test.sys_fs_ready_devs.clone().unwrap_or_default(),
                test.sys_lvms.clone().unwrap_or_default(),
            );

            if let Err(ref err) = result {
                eprintln!(
                    "Unexpected error from test case {}: {}",
                    i + 1,
                    test.case
                );

                if let Some(ref ctx) = test.context {
                    eprintln!("\nCONTEXT: {ctx}\n");
                }

                eprintln!("Test structure: {test:?}");
                eprintln!("Error: {err:?}");
            }

            assert!(result.is_ok());
        }

        for (i, test) in should_err.iter().enumerate() {
            let result = validate_blockdev(
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
