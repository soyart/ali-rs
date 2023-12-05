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

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCollectValidPv {
        pv: String,
        sys_fs_devs: HashMap<String, BlockDevType>,
        sys_fs_ready_devs: HashMap<String, BlockDevType>,
        sys_lvms: HashMap<String, BlockDevPaths>,
        valids: BlockDevPaths,
    }

    #[test]
    fn test_collect_valid() {
        let mut should_ok = vec![
            //
            TestCollectValidPv {
                pv: "/dev/fda2".into(),
                sys_fs_devs: HashMap::from([
                    //
                    ("/dev/vda1".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_fs_ready_devs: HashMap::from([
                    //
                    ("/dev/fda2".into(), BlockDevType::Partition),
                ]),
                sys_lvms: HashMap::from([(
                    "/dev/fda1".into(),
                    vec![
                        //
                        LinkedList::from([
                            BlockDev {
                                device: "/dev/fda1".into(),
                                device_type: TYPE_PV,
                            },
                            BlockDev {
                                device: "/dev/myvg".into(),
                                device_type: TYPE_VG,
                            },
                            BlockDev {
                                device: "/dev/somelv".into(),
                                device_type: TYPE_LV,
                            },
                        ]),
                    ],
                )]),
                valids: BlockDevPaths::new(),
            },
            TestCollectValidPv {
                pv: "/dev/fda2".into(),
                sys_fs_devs: HashMap::from([
                    //
                    ("/dev/vda1".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_fs_ready_devs: HashMap::from([]),
                sys_lvms: HashMap::from([(
                    "/dev/fda1".into(),
                    vec![
                        //
                        LinkedList::from([
                            BlockDev {
                                device: "/dev/fda1".into(),
                                device_type: TYPE_PV,
                            },
                            BlockDev {
                                device: "/dev/myvg".into(),
                                device_type: TYPE_VG,
                            },
                            BlockDev {
                                device: "/dev/somelv".into(),
                                device_type: TYPE_LV,
                            },
                        ]),
                    ],
                )]),
                valids: BlockDevPaths::from(vec![
                    //
                    LinkedList::from([
                        BlockDev {
                            device: "/dev/fdb1".into(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: "/dev/fdb1".into(),
                            device_type: BlockDevType::Fs("ext3".into()),
                        },
                    ]),
                    //
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda2".into(),
                            device_type: TYPE_PART,
                        },
                    ]),
                ]),
            },
            TestCollectValidPv {
                pv: "/dev/fda2".into(),
                sys_fs_devs: HashMap::from([
                    //
                    ("/dev/vda1".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_fs_ready_devs: HashMap::from([]),
                sys_lvms: HashMap::from([(
                    "/dev/fda1".into(),
                    vec![
                        //
                        LinkedList::from([
                            BlockDev {
                                device: "/dev/fda1".into(),
                                device_type: TYPE_PV,
                            },
                            BlockDev {
                                device: "/dev/myvg".into(),
                                device_type: TYPE_VG,
                            },
                            BlockDev {
                                device: "/dev/somelv".into(),
                                device_type: TYPE_LV,
                            },
                        ]),
                    ],
                )]),
                valids: BlockDevPaths::from(vec![
                    //
                    LinkedList::from([
                        BlockDev {
                            device: "/dev/fdb1".into(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: "/dev/fdb1".into(),
                            device_type: BlockDevType::Fs("ext3".into()),
                        },
                    ]),
                    //
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda2".into(),
                            device_type: TYPE_PART,
                        },
                    ]),
                ]),
            },
            TestCollectValidPv {
                pv: "/dev/mapper/foo".into(),
                sys_fs_devs: HashMap::from([
                    //
                    ("/dev/vda1".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_fs_ready_devs: HashMap::from([]),
                sys_lvms: HashMap::from([(
                    "/dev/fda1".into(),
                    vec![
                        //
                        LinkedList::from([
                            BlockDev {
                                device: "/dev/fda1".into(),
                                device_type: TYPE_PV,
                            },
                            BlockDev {
                                device: "/dev/myvg".into(),
                                device_type: TYPE_VG,
                            },
                            BlockDev {
                                device: "/dev/somelv".into(),
                                device_type: TYPE_LV,
                            },
                        ]),
                    ],
                )]),
                valids: BlockDevPaths::from(vec![
                    //
                    LinkedList::from([
                        BlockDev {
                            device: "/dev/fdb1".into(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: "/dev/fdb1".into(),
                            device_type: BlockDevType::Fs("ext3".into()),
                        },
                    ]),
                    //
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda2".into(),
                            device_type: TYPE_PART,
                        },
                        BlockDev {
                            device: "/dev/mapper/foo".into(),
                            device_type: TYPE_LUKS,
                        },
                    ]),
                ]),
            },
        ];

        let mut should_err = vec![
            //
            TestCollectValidPv {
                pv: "/dev/fda2".into(),
                sys_fs_devs: HashMap::from([
                    //
                    ("/dev/fda2".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_fs_ready_devs: HashMap::from([]),
                sys_lvms: HashMap::from([(
                    "/dev/fda1".into(),
                    vec![
                        //
                        LinkedList::from([
                            BlockDev {
                                device: "/dev/fda1".into(),
                                device_type: TYPE_PV,
                            },
                            BlockDev {
                                device: "/dev/myvg".into(),
                                device_type: TYPE_VG,
                            },
                            BlockDev {
                                device: "/dev/somelv".into(),
                                device_type: TYPE_LV,
                            },
                        ]),
                    ],
                )]),
                valids: BlockDevPaths::new(),
            },
            TestCollectValidPv {
                pv: "/dev/fda2".into(),
                sys_fs_devs: HashMap::from([]),
                sys_fs_ready_devs: HashMap::from([]),
                sys_lvms: HashMap::from([(
                    "/dev/fda1".into(),
                    vec![
                        //
                        LinkedList::from([
                            BlockDev {
                                device: "/dev/fda1".into(),
                                device_type: TYPE_PV,
                            },
                            BlockDev {
                                device: "/dev/myvg".into(),
                                device_type: TYPE_VG,
                            },
                            BlockDev {
                                device: "/dev/somelv".into(),
                                device_type: TYPE_LV,
                            },
                        ]),
                    ],
                )]),
                valids: BlockDevPaths::from([
                    //
                    LinkedList::from([
                        BlockDev {
                            device: "/dev/fdb1".into(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: "/dev/fdb1".into(),
                            device_type: BlockDevType::Fs("ext3".into()),
                        },
                    ]),
                    //
                    LinkedList::from([
                        BlockDev {
                            device: "/dev/fda".into(),
                            device_type: TYPE_DISK,
                        },
                        BlockDev {
                            device: "/dev/fda2".into(),
                            device_type: BlockDevType::Fs("ext3".into()),
                        },
                    ]),
                ]),
            },
            TestCollectValidPv {
                pv: "/dev/fda2".into(),
                sys_fs_devs: HashMap::from([]),
                sys_fs_ready_devs: HashMap::from([]),
                sys_lvms: HashMap::from([]),
                valids: BlockDevPaths::from([
                    //
                    LinkedList::from([
                        BlockDev {
                            device: "/dev/fdb1".into(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: "/dev/fdb1".into(),
                            device_type: BlockDevType::Fs("ext3".into()),
                        },
                    ]),
                    //
                    LinkedList::from([
                        BlockDev {
                            device: "/dev/fda".into(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: "/dev/fda2".into(),
                            device_type: TYPE_PART,
                        },
                        BlockDev {
                            device: "/dev/mapper/foo".into(),
                            device_type: TYPE_LUKS,
                        },
                        BlockDev {
                            device: "/dev/mapper/foo".into(),
                            device_type: BlockDevType::Fs("btrfs".into()),
                        },
                    ]),
                ]),
            },
        ];

        for (i, t) in should_ok.iter_mut().enumerate() {
            let result = collect_valid(
                &t.pv,
                &t.sys_fs_devs,
                &mut t.sys_fs_ready_devs,
                &mut t.sys_lvms,
                &mut t.valids,
            );

            if let Err(ref err) = result {
                eprintln!("unexpected error for case {}: {err}", i + 1);
            }

            assert!(result.is_ok());
        }

        for (i, t) in should_err.iter_mut().enumerate() {
            let result = collect_valid(
                &t.pv,
                &t.sys_fs_devs,
                &mut t.sys_fs_ready_devs,
                &mut t.sys_lvms,
                &mut t.valids,
            );

            if result.is_ok() {
                eprintln!("unexpected ok result for case {}", i + 1);
            }

            assert!(result.is_err());
        }
    }
}
