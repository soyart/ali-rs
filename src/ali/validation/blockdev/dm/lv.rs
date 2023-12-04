use std::collections::{
    HashMap,
    LinkedList,
};

use super::*;
use crate::ali::ManifestLvmLv;
use crate::entity::blockdev::*;
use crate::errors::AliError;

// Collect valid LV device path(s) into valids
#[inline]
pub(super) fn collect_valid(
    lv: &ManifestLvmLv,
    sys_fs_devs: &HashMap<String, BlockDevType>,
    sys_lvms: &mut HashMap<String, BlockDevPaths>,
    valids: &mut BlockDevPaths,
) -> Result<(), AliError> {
    let (vg_name, lv_name) = vg_lv_name(lv);

    let msg = "lvm lv validation failed";
    if let Some(fs) = sys_fs_devs.get(&lv_name) {
        return Err(AliError::BadManifest(format!(
            "{msg}: another lv with matching name {lv_name} was already used as filesystem {fs}"
        )));
    }

    let target_vg = BlockDev {
        device: vg_name.clone(),
        device_type: TYPE_VG,
    };

    let lv_dev = BlockDev {
        device: lv_name.clone(),
        device_type: TYPE_LV,
    };

    // A VG can host multiple LVs, so we will need to copy the LV
    // to all paths leading to it. This means that we must leave the
    // matching VG path in-place before we can
    let mut lv_vgs = Vec::new();

    let msg = "lvm lv validation failed";
    for sys_lvm_list in sys_lvms.values().flatten() {
        for node in sys_lvm_list {
            if *node != target_vg {
                continue;
            }

            let sys_list = sys_lvm_list.clone();
            let mut list = LinkedList::new();

            for list_node in sys_list {
                list.push_back(list_node.clone());
                if list_node == target_vg {
                    break;
                }
            }

            list.push_back(lv_dev.clone());

            lv_vgs.push(list);
        }
    }

    for old_list in valids.iter_mut() {
        let top_most = old_list
            .back()
            .expect("no back node for linked list in manifest_devs");

        // Skip path from different VG
        if *top_most == lv_dev {
            continue;
        }

        if top_most.device != vg_name {
            continue;
        }

        if !is_lv_base(&top_most.device_type) {
            return Err(AliError::BadManifest(format!(
                "{msg}: lv {lv_name} vg base {vg_name} cannot have type {}",
                top_most.device_type
            )));
        }

        let mut list = old_list.clone();
        list.push_back(lv_dev.clone());
        lv_vgs.push(list);
    }

    if lv_vgs.is_empty() {
        return Err(AliError::BadManifest(format!(
            "{msg}: lv {lv_name} no vg device matching {vg_name} in manifest or in the system"
        )));
    }

    valids.extend_from_slice(&lv_vgs);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_collect_lv_error() {
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
                    ("/dev/fda2".into(), BlockDevType::Fs("ext4".into())),
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
            // // 6
            // TestCollectLv {
            //     lv: ManifestLvmLv {
            //         name: "mylv".into(),
            //         vg: "myvg".into(),
            //         size: None,
            //     },
            //     sys_fs_devs: HashMap::from([
            //         ("/dev/fda2".into(), BlockDevType::Fs("ext4".into())),
            //         ("/dev/vda1".into(), BlockDevType::Fs("swap".into())),
            //     ]),
            //     sys_lvms: HashMap::from([
            //         (
            //             "/dev/fda1".into(),
            //             vec![
            //                 //
            //                 LinkedList::from([
            //                     BlockDev {
            //                         device: "/dev/fda1".into(),
            //                         device_type: TYPE_PV,
            //                     },
            //                     BlockDev {
            //                         device: "/dev/myvg".into(),
            //                         device_type: TYPE_VG,
            //                     },
            //                     BlockDev {
            //                         device: "/dev/somelv".into(),
            //                         device_type: TYPE_LV,
            //                     },
            //                 ]),
            //             ],
            //         ),
            //         (
            //             "/dev/fda2".into(),
            //             vec![LinkedList::from([BlockDev {
            //                 device: "/dev/fda2".into(),
            //                 device_type: TYPE_PV,
            //             }])],
            //         ),
            //     ]),
            //     valids: BlockDevPaths::from([
            //         //
            //         LinkedList::from([
            //             BlockDev {
            //                 device: "/dev/fda2".into(),
            //                 device_type: TYPE_PV,
            //             },
            //             BlockDev {
            //                 device: "/dev/myvg".into(),
            //                 device_type: TYPE_VG,
            //             },
            //             BlockDev {
            //                 device: "/dev/somelv".into(),
            //                 device_type: TYPE_LV,
            //             },
            //         ]),
            //     ]),
            //     count: 2u8,
            // },
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
