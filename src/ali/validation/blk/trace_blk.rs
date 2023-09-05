use std::collections::{HashMap, LinkedList};
use std::process::Command;

use serde::{Deserialize, Serialize};
use toml;

use crate::utils::shell::CmdError;

use super::*;

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

pub(super) fn run_blkid(cmd_blkid: &str) -> Result<String, AliError> {
    let cmd = Command::new(cmd_blkid)
        .output()
        .map_err(|err| AliError::CmdFailed {
            error: CmdError::ErrSpawn { error: err },
            context: "blkid command failed".to_string(),
        })?;

    String::from_utf8(cmd.stdout)
        .map_err(|err| AliError::AliRsBug(format!("blkid output not string: {err}")))
}

pub(super) fn sys_fs_ready(output_blkid: &str) -> HashMap<String, BlockDevType> {
    let lines_blkid: Vec<&str> = output_blkid.lines().collect();

    let mut fs_ready = HashMap::new();
    for line in lines_blkid {
        if line.is_empty() {
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
pub(super) fn sys_fs(output_blkid: &str) -> HashMap<String, BlockDevType> {
    let lines_blkid: Vec<&str> = output_blkid.lines().collect();

    let mut fs = HashMap::new();
    for line in lines_blkid {
        if line.is_empty() {
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
pub(super) fn sys_lvms(lvs_cmd: &str, pvs_cmd: &str) -> HashMap<String, BlockDevPaths> {
    let cmd_lvs = Command::new(lvs_cmd).output().expect("failed to run `lvs`");
    let output_lvs = String::from_utf8(cmd_lvs.stdout).expect("output is not utf-8");
    let lines_lvs: Vec<&str> = output_lvs.lines().skip(1).collect();

    // # Collect VG leading to LV
    // For example, if we have 2 VGs - vg1 and vg2
    // and vg1 has 2 LVs: vg1/lv1 and vg1/lv2
    // while vg2 has 1 LV: vg2/lv3
    //
    // Then the collected result would be:
    // [{vg1 -> lv1}, {vg1 -> lv2}, {vg2 -> lv3}]
    let mut lv_paths = BlockDevPaths::new();

    for line in lines_lvs {
        if line.is_empty() {
            continue;
        }

        let line: Vec<&str> = line.split_whitespace().collect();

        if line.len() < 2 {
            continue;
        }

        let first_col = line.first().unwrap();
        if first_col != &"LV" {
            continue;
        }

        let lv_name = *first_col;
        let vg_name = *line
            .get(1)
            .expect("missing 2nd string on command `lvs` output");

        lv_paths.push(BlockDevPath::from([
            BlockDev {
                device: format!("/dev/{vg_name}"),
                device_type: TYPE_VG,
            },
            BlockDev {
                device: format!("{vg_name}/{lv_name}"),
                device_type: TYPE_LV,
            },
        ]));
    }

    let cmd_pvs = Command::new(pvs_cmd).output().expect("failed to run `pvs`");

    let output_pvs = String::from_utf8(cmd_pvs.stdout).expect("output is not utf-8");
    let lines_pvs: Vec<&str> = output_pvs.lines().skip(1).collect();

    let mut lvms = HashMap::new();

    // Collect all PVs leading to VG.
    // One PV can only be mapped to one VG.
    for line in lines_pvs {
        if line.is_empty() {
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
            .first()
            .expect("missing 1st string on pvs output")
            .to_string();

        // Construct VG based on info from output of command `pvs`
        let vg_name = line.get(1).expect("missing 2nd string on pvs output");
        let vg_name = format!("/dev/{vg_name}");
        let vg = BlockDev {
            device: vg_name.to_string(),
            device_type: TYPE_VG,
        };

        // Create a template from this PV leading up to its VG,
        // to be appended with its LVs from `lv_paths.
        let pv_template = BlockDevPath::from([
            BlockDev {
                device: pv_name.clone(),
                device_type: TYPE_UNKNOWN,
            },
            BlockDev {
                device: pv_name.clone(),
                device_type: TYPE_PV,
            },
            vg.clone(),
        ]);

        // `paths` collects all paths from this PV -1-> VG -many-> LVs
        // So we would have to iterate lv_paths and copy all paths for this PV
        let mut paths = Vec::new();
        for lv_path in &mut lv_paths.clone() {
            let vg_tmp = lv_path.pop_front().expect("None vg_tmp");

            if vg_tmp == vg {
                let mut path = LinkedList::new();
                let lv_tmp = lv_path.pop_back().expect("None lv_tmp");

                path.extend(pv_template.clone());
                path.push_back(lv_tmp);

                paths.push(path);
            }
        }

        lvms.insert(pv_name.clone(), paths);
    }

    lvms
}

#[test]
fn test_trace_existing_fs_ready() {
    let mut expected_results = HashMap::new();
    expected_results.insert("/dev/vda2".to_string(), TYPE_UNKNOWN);

    let output_blkid = run_blkid("./mock_cmd/blkid").expect("run_blkid failed");
    let traced = sys_fs_ready(&output_blkid);
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
    let traced = sys_fs(&output_blkid);
    for (k, v) in traced.into_iter() {
        let expected = expected_results.get(&k);
        assert!(expected.is_some());

        assert_eq!(expected.unwrap().clone(), v);
    }
}

#[test]
fn test_trace_existing_lvms() {
    // Hard-coded expected values from ./mock_cmd/{lvs,pvs}
    let traced = sys_lvms("./mock_cmd/lvs", "./mock_cmd/pvs");

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
