use std::collections::HashSet;

use super::*;
use crate::ali::ManifestLvmLv;
use crate::entity::blockdev::*;
use crate::errors::AliError;

const MSG: &str = "lvm lv validation failed";

// Collect valid LV device path(s) into valids
#[inline]
pub(super) fn collect_valid(
    lv: &ManifestLvmLv,
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_lvms: &mut HashMap<String, BlockDevPaths>,
    valids: &mut BlockDevPaths,
) -> Result<(), AliError> {
    let (vg_name, lv_name) = vg_lv_name(lv);

    if let Some(fs) = sys_fs_devs.get(&lv_name) {
        return Err(AliError::BadManifest(format!(
            "{MSG}: another lv with matching name {lv_name} was already used as filesystem {fs}"
        )));
    }

    let target_vg = BlockDev {
        device: vg_name.clone(),
        device_type: TYPE_VG,
    };

    let target_lv = BlockDev {
        device: lv_name.clone(),
        device_type: TYPE_LV,
    };

    let lv_paths_sys = collect_from_sys(&target_vg, &target_lv, sys_lvms);
    let lv_paths_valids = collect_from_valids(&target_vg, &target_lv, valids);

    let mut lv_paths = HashSet::new();
    lv_paths.extend(lv_paths_sys);
    lv_paths.extend(lv_paths_valids);

    if lv_paths.is_empty() {
        return Err(AliError::BadManifest(format!(
            "{MSG}: lv {lv_name} no vg device matching {vg_name} in manifest or in the system"
        )));
    }

    valids.extend(lv_paths);

    Ok(())
}

fn collect_from_sys(
    target_vg: &BlockDev,
    target_lv: &BlockDev,
    sys_lvms: &HashMap<String, BlockDevPaths>,
) -> BlockDevPaths {
    let mut result = BlockDevPaths::new();

    for sys_lvm_list in sys_lvms.values().flatten() {
        let copied = copy_until(sys_lvm_list, target_vg);

        if copied.is_none() {
            continue;
        }

        let mut path = copied.unwrap();
        path.push_back(target_lv.clone());
        result.push(path);
    }

    result
}

fn collect_from_valids(
    target_vg: &BlockDev,
    target_lv: &BlockDev,
    valids: &BlockDevPaths,
) -> BlockDevPaths {
    let mut result = BlockDevPaths::new();

    for valid_list in valids {
        let copied = copy_until(valid_list, target_vg);

        if copied.is_none() {
            continue;
        }

        let mut path = copied.unwrap();
        path.push_back(target_lv.clone());
        result.push(path);
    }

    result
}

fn copy_until(list: &BlockDevPath, target: &BlockDev) -> Option<BlockDevPath> {
    if !list.contains(target) {
        return None;
    }

    let mut result = BlockDevPath::new();

    let copied = list.clone();
    for node in copied {
        result.push_back(node.clone());
        if node == *target {
            break;
        }
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestCollectFromSys {
        vg: BlockDev,
        lv: BlockDev,
        sys_lvms: HashMap<String, BlockDevPaths>,
        expected_result: BlockDevPaths,
    }

    #[derive(Debug)]
    struct TestCollectFromValids {
        vg: BlockDev,
        lv: BlockDev,
        valids: BlockDevPaths,
        expected_result: BlockDevPaths,
    }

    #[derive(Debug)]
    struct TestCollectLv {
        lv: ManifestLvmLv,
        sys_fs_devs: HashMap<String, BlockDevType>,
        sys_lvms: HashMap<String, BlockDevPaths>,
        valids: BlockDevPaths,

        // counts how many times lv should appear in valids
        count: u8,
    }

    #[test]
    fn test_collect_from_sys() {
        let should_ok = vec![
            // 1
            TestCollectFromSys {
                vg: BlockDev {
                    device: "/dev/vg".into(),
                    device_type: TYPE_VG,
                },
                lv: BlockDev {
                    device: "/dev/vg/lv".into(),
                    device_type: TYPE_LV,
                },
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
                                device: "/dev/vg".into(),
                                device_type: TYPE_VG,
                            },
                        ]),
                    ],
                )]),
                expected_result: vec![
                    //
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/vg".into(),
                            device_type: TYPE_VG,
                        },
                        BlockDev {
                            device: "/dev/vg/lv".into(),
                            device_type: TYPE_LV,
                        },
                    ]),
                ],
            },
            // 2
            TestCollectFromSys {
                vg: BlockDev {
                    device: "/dev/vg".into(),
                    device_type: TYPE_VG,
                },
                lv: BlockDev {
                    device: "/dev/vg/lv".into(),
                    device_type: TYPE_LV,
                },
                sys_lvms: HashMap::from([
                    (
                        "/dev/fda1".into(),
                        vec![
                            //
                            LinkedList::from([
                                BlockDev {
                                    device: "/dev/fda1".into(),
                                    device_type: TYPE_PV,
                                },
                                BlockDev {
                                    device: "/dev/vg".into(),
                                    device_type: TYPE_VG,
                                },
                            ]),
                        ],
                    ),
                    (
                        "/dev/fda2".into(),
                        vec![
                            //
                            LinkedList::from([
                                BlockDev {
                                    device: "/dev/fda2".into(),
                                    device_type: TYPE_PV,
                                },
                                BlockDev {
                                    device: "/dev/vg".into(),
                                    device_type: TYPE_VG,
                                },
                            ]),
                        ],
                    ),
                ]),
                expected_result: vec![
                    //
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/vg".into(),
                            device_type: TYPE_VG,
                        },
                        BlockDev {
                            device: "/dev/vg/lv".into(),
                            device_type: TYPE_LV,
                        },
                    ]),
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda2".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/vg".into(),
                            device_type: TYPE_VG,
                        },
                        BlockDev {
                            device: "/dev/vg/lv".into(),
                            device_type: TYPE_LV,
                        },
                    ]),
                ],
            },
            // 3
            TestCollectFromSys {
                vg: BlockDev {
                    device: "/dev/vg".into(),
                    device_type: TYPE_VG,
                },
                lv: BlockDev {
                    device: "/dev/vg/lv".into(),
                    device_type: TYPE_LV,
                },
                sys_lvms: HashMap::from([
                    (
                        "/dev/fda1".into(),
                        vec![
                            //
                            LinkedList::from([
                                BlockDev {
                                    device: "/dev/fda1".into(),
                                    device_type: TYPE_PV,
                                },
                                BlockDev {
                                    device: "/dev/vg".into(),
                                    device_type: TYPE_VG,
                                },
                                BlockDev {
                                    device: "/dev/somelv".into(),
                                    device_type: TYPE_LV,
                                },
                            ]),
                        ],
                    ),
                    (
                        "/dev/fda2".into(),
                        vec![
                            //
                            LinkedList::from([
                                BlockDev {
                                    device: "/dev/fda2".into(),
                                    device_type: TYPE_PV,
                                },
                                BlockDev {
                                    device: "/dev/vg".into(),
                                    device_type: TYPE_VG,
                                },
                                BlockDev {
                                    device: "/dev/somelv".into(),
                                    device_type: TYPE_LV,
                                },
                            ]),
                        ],
                    ),
                ]),
                expected_result: vec![
                    //
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/vg".into(),
                            device_type: TYPE_VG,
                        },
                        BlockDev {
                            device: "/dev/vg/lv".into(),
                            device_type: TYPE_LV,
                        },
                    ]),
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda2".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/vg".into(),
                            device_type: TYPE_VG,
                        },
                        BlockDev {
                            device: "/dev/vg/lv".into(),
                            device_type: TYPE_LV,
                        },
                    ]),
                ],
            },
        ];

        for (_i, t) in should_ok.iter().enumerate() {
            let result = collect_from_sys(&t.vg, &t.lv, &t.sys_lvms);

            let mut count = 0;
            for expected_list in &t.expected_result {
                for result_list in &result {
                    if expected_list == result_list {
                        count += 1;
                        break;
                    }
                }
            }

            assert_eq!(count, t.expected_result.len());
        }
    }

    #[test]
    fn test_collect_from_valids() {
        let should_ok = vec![
            // 1
            TestCollectFromValids {
                vg: BlockDev {
                    device: "/dev/vg".into(),
                    device_type: TYPE_VG,
                },
                lv: BlockDev {
                    device: "/dev/vg/lv".into(),
                    device_type: TYPE_LV,
                },
                valids: vec![
                    //
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/vg".into(),
                            device_type: TYPE_VG,
                        },
                    ]),
                ],
                expected_result: vec![
                    //
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/vg".into(),
                            device_type: TYPE_VG,
                        },
                        BlockDev {
                            device: "/dev/vg/lv".into(),
                            device_type: TYPE_LV,
                        },
                    ]),
                ],
            },
            // 2
            TestCollectFromValids {
                vg: BlockDev {
                    device: "/dev/vg".into(),
                    device_type: TYPE_VG,
                },
                lv: BlockDev {
                    device: "/dev/vg/lv".into(),
                    device_type: TYPE_LV,
                },
                valids: vec![
                    //
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/vg".into(),
                            device_type: TYPE_VG,
                        },
                        BlockDev {
                            device: "/dev/somelv".into(),
                            device_type: TYPE_VG,
                        },
                    ]),
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fdb2".into(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: "/dev/fdb2".into(),
                            device_type: TYPE_LUKS,
                        },
                        BlockDev {
                            device: "/dev/mapper/cryptfdb2".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/somevg".into(),
                            device_type: TYPE_VG,
                        },
                    ]),
                ],
                expected_result: vec![
                    //
                    LinkedList::from([
                        //
                        BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_UNKNOWN,
                        },
                        BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_PV,
                        },
                        BlockDev {
                            device: "/dev/vg".into(),
                            device_type: TYPE_VG,
                        },
                        BlockDev {
                            device: "/dev/vg/lv".into(),
                            device_type: TYPE_LV,
                        },
                    ]),
                ],
            },
        ];

        for (_i, t) in should_ok.iter().enumerate() {
            let result = collect_from_valids(&t.vg, &t.lv, &t.valids);

            let mut count = 0;
            for expected_list in &t.expected_result {
                for result_list in &result {
                    if expected_list == result_list {
                        count += 1;
                        break;
                    }
                }
            }

            assert_eq!(count, t.expected_result.len());
        }
    }

    #[test]
    fn test_collect_lv() {
        let mut should_ok = vec![
            // 1
            TestCollectLv {
                lv: ManifestLvmLv {
                    name: "mylv".into(),
                    vg: "myvg".into(),
                    size: None,
                },
                sys_fs_devs: HashMap::from([
                    ("/dev/fda2".into(), BlockDevType::Fs("ext4".into())),
                    ("/dev/vda1".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_lvms: HashMap::from([(
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
                )]),
                valids: BlockDevPaths::new(),
                count: 1u8,
            },
            // 2
            TestCollectLv {
                lv: ManifestLvmLv {
                    name: "mylv".into(),
                    vg: "myvg".into(),
                    size: None,
                },
                sys_fs_devs: HashMap::from([
                    ("/dev/fda2".into(), BlockDevType::Fs("ext4".into())),
                    ("/dev/vda1".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_lvms: HashMap::from([(
                    "/dev/fda1".into(),
                    vec![
                        //
                        LinkedList::from([BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_PV,
                        }]),
                    ],
                )]),
                valids: BlockDevPaths::from([
                    //
                    LinkedList::from([BlockDev {
                        device: "/dev/fda1".into(),
                        device_type: TYPE_PV,
                    }]),
                    LinkedList::from([BlockDev {
                        device: "/dev/myvg".into(),
                        device_type: TYPE_VG,
                    }]),
                ]),
                count: 1u8,
            },
            // 3
            TestCollectLv {
                lv: ManifestLvmLv {
                    name: "mylv".into(),
                    vg: "myvg".into(),
                    size: None,
                },
                sys_fs_devs: HashMap::from([
                    ("/dev/fda2".into(), BlockDevType::Fs("ext4".into())),
                    ("/dev/vda1".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_lvms: HashMap::from([(
                    "/dev/fda1".into(),
                    vec![
                        //
                        LinkedList::from([BlockDev {
                            device: "/dev/fda1".into(),
                            device_type: TYPE_PV,
                        }]),
                        LinkedList::from([BlockDev {
                            device: "/dev/myvg".into(),
                            device_type: TYPE_VG,
                        }]),
                        LinkedList::from([BlockDev {
                            device: "/dev/somelv".into(),
                            device_type: TYPE_LV,
                        }]),
                    ],
                )]),
                valids: BlockDevPaths::from([]),
                count: 1u8,
            },
            // 4
            TestCollectLv {
                lv: ManifestLvmLv {
                    name: "mylv".into(),
                    vg: "myvg".into(),
                    size: None,
                },
                sys_fs_devs: HashMap::from([
                    ("/dev/fda2".into(), BlockDevType::Fs("ext4".into())),
                    ("/dev/vda1".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_lvms: HashMap::from([("/dev/fda1".into(), vec![])]),
                valids: BlockDevPaths::from([
                    //
                    LinkedList::from([BlockDev {
                        device: "/dev/fda1".into(),
                        device_type: TYPE_PV,
                    }]),
                    LinkedList::from([BlockDev {
                        device: "/dev/myvg".into(),
                        device_type: TYPE_VG,
                    }]),
                    LinkedList::from([BlockDev {
                        device: "/dev/somelv".into(),
                        device_type: TYPE_LV,
                    }]),
                ]),
                count: 1u8,
            },
            // 5
            TestCollectLv {
                lv: ManifestLvmLv {
                    name: "mylv".into(),
                    vg: "myvg".into(),
                    size: None,
                },
                sys_fs_devs: HashMap::from([
                    //
                    ("/dev/vda1".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_lvms: HashMap::from([
                    (
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
                    ),
                    (
                        "/dev/fda2".into(),
                        vec![
                            //
                            LinkedList::from([
                                BlockDev {
                                    device: "/dev/fda2".into(),
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
                    ),
                ]),
                valids: BlockDevPaths::from([]),
                count: 2u8,
            },
            // 6
            TestCollectLv {
                lv: ManifestLvmLv {
                    name: "mylv".into(),
                    vg: "myvg".into(),
                    size: None,
                },
                sys_fs_devs: HashMap::from([
                    //
                    ("/dev/vda1".into(), BlockDevType::Fs("swap".into())),
                ]),
                sys_lvms: HashMap::from([
                    (
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
                    ),
                    (
                        "/dev/fda2".into(),
                        vec![
                            //
                            LinkedList::from([
                                //
                                BlockDev {
                                    device: "/dev/fda2".into(),
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
                    ),
                    (
                        "/dev/fdd1".into(),
                        vec![
                            //
                            LinkedList::from([
                                //
                                BlockDev {
                                    device: "/dev/fdd1".into(),
                                    device_type: TYPE_PV,
                                },
                                BlockDev {
                                    device: "/dev/somevg".into(),
                                    device_type: TYPE_VG,
                                },
                            ]),
                        ],
                    ),
                ]),
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
                ]),
                count: 2u8,
            },
        ];

        for (i, t) in should_ok.iter_mut().enumerate() {
            if i != 5 {
                continue;
            }

            let result = collect_valid(
                &t.lv,
                &t.sys_fs_devs,
                &mut t.sys_lvms,
                &mut t.valids,
            );

            let (_, lv_name) = vg_lv_name(&t.lv);
            let target_lv = BlockDev {
                device: lv_name,
                device_type: TYPE_LV,
            };

            let mut count_list = 0u8;
            for list in t.valids.iter() {
                for node in list {
                    if *node == target_lv {
                        count_list += 1;
                    }
                }
            }

            let mut count_nodes = 0u8;
            for node in t.valids.iter().flatten() {
                if *node == target_lv {
                    count_nodes += 1;
                }
            }

            let count_ok = count_nodes == t.count && count_list == t.count;
            let result_ok = result.is_ok();

            if !result_ok {
                eprintln!("error: {}", result.err().unwrap());
            }

            if !count_ok {
                eprintln!(
                    "unexpected count: expecting count_list {}, got {}",
                    t.count, count_list
                );
                eprintln!(
                    "unexpected count: expecting count_nodes {}, got {}",
                    t.count, count_nodes
                );
            }

            if !(result_ok && count_ok) {
                eprintln!("test case number {}", i + 1);
                eprintln!("valids {:?}", t.valids);

                panic!("unexpected values")
            }
        }
    }
}
