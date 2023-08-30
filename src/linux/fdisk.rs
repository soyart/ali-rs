/// This modules formats block devices by simply piping printf output to fdisk binary
/// @TODO: Consider fdisk sys https://github.com/IBM/fdisk-sys
///
use std::process::{Command, Stdio};

use crate::errors::AliError;
use crate::manifest::{ManifestPartition, PartitionTable};

/// Returns fdisk cmd string for creating gpt/msdos partition table
pub fn create_table_cmd(table: &PartitionTable) -> String {
    match table {
        PartitionTable::Gpt => "g\nw\n".to_string(),
        PartitionTable::Mbr => "o\nw\n".to_string(),
    }
}

/// Returns fdisk cmd for creating new partition.
/// It assumes caller calls it from 1st to last partitions,
/// in that exact order, so no `start` sector will be used.
pub fn create_partition_cmd(
    table: &PartitionTable,
    part_num: usize,
    part: &ManifestPartition,
) -> String {
    let size = match part.size {
        Some(ref s) => format!("+{s}"),
        None => "".to_string(),
    };

    match table {
        PartitionTable::Gpt => assemble_and_w(&["n", &part_num.to_string(), "", &size]),
        PartitionTable::Mbr => assemble_and_w(&[
            "n",
            "p", // Only create primary msdos partition for now
            &part_num.to_string(),
            "",
            &size,
        ]),
    }
}

/// Returns fdisk cmd for changing partition type.
pub fn set_partition_type_cmd(part_num: usize, part: &ManifestPartition) -> String {
    match part_num {
        1 => assemble_and_w(&["t", &part.part_type]),
        _ => assemble_and_w(&["t", &part_num.to_string(), &part.part_type]),
    }
}

/// Pipe cmd with printf to fdisk:
/// ```shell
/// printf $cmd | fdisk $device
/// ```
pub fn run_fdisk_cmd(device: &str, cmd: &str) -> Result<(), AliError> {
    let printf_cmd = Command::new("printf")
        .arg(cmd)
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn printf");

    let mut fdisk_cmd = Command::new("fdisk")
        .arg(device)
        .stdin(printf_cmd.stdout.unwrap())
        .spawn()
        .expect("failed to spawn fdisk");

    match fdisk_cmd.wait() {
        Ok(result) => match result.success() {
            false => Err(AliError::CmdFailed {
                error: None,
                context: format!(
                    "fdisk command exited with bad status: {}",
                    result.code().expect("failed to get exit code"),
                ),
            }),
            _ => Ok(()),
        },
        Err(err) => Err(AliError::CmdFailed {
            error: None,
            context: format!("fdisk command failed to run: {}", err.to_string()),
        }),
    }
}

fn assemble_and_w(slice: &[&str]) -> String {
    let mut joined = slice.join("\n");
    joined.push_str("\nw\n");

    joined
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_part_cmd() {
        struct Test<'a> {
            table: PartitionTable,
            num: usize,
            part: ManifestPartition,
            expected: &'a str,
        }

        let tests: Vec<Test> = vec![
            Test {
                table: PartitionTable::Gpt,
                num: 1,
                part: ManifestPartition {
                    label: "foo".to_string(),
                    size: Some("200M".to_string()),
                    part_type: "8e".to_string(),
                },
                expected: "n\n1\n\n+200M\nw\n",
            },
            Test {
                table: PartitionTable::Mbr,
                num: 1,
                part: ManifestPartition {
                    label: "foo".to_string(),
                    size: None,
                    part_type: "8e".to_string(),
                },
                expected: "n\np\n1\n\n\nw\n",
            },
        ];

        for test in tests {
            let result = create_partition_cmd(&test.table, test.num, &test.part);
            assert_eq!(test.expected, result);
        }
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    #[cfg(not(target_os = "macos"))]
    fn test_run_fdisk_cmd() {
        use crate::utils::shell::test_utils;

        // Create a zeroed 500M file as fake block device
        let fname = "./fake-disk.img";
        if let Err(err) = test_utils::dd("/dev/zero", fname, "100M", 5) {
            panic!(
                "dd command failed to create zeroed dummy device {fname} with size 100Mx5: {err}"
            );
        }

        let create_gpt_table = create_table_cmd(&PartitionTable::Gpt);
        run_fdisk_cmd(fname, &create_gpt_table).expect("failed to create gpt table");

        let manifest_p1 = ManifestPartition {
            label: "efi".to_string(),
            size: Some("20M".to_string()),
            part_type: "1".to_string(),
        };

        let manifest_p2 = ManifestPartition {
            label: "root_part".to_string(),
            size: None,
            part_type: "8e".to_string(),
        };

        let create_gpt_p1 = create_partition_cmd(&PartitionTable::Gpt, 1, &manifest_p1);
        let create_gpt_p2 = create_partition_cmd(&PartitionTable::Gpt, 2, &manifest_p2);

        run_fdisk_cmd(fname, &create_gpt_p1).expect("failed to create p1");
        run_fdisk_cmd(fname, &create_gpt_p2).expect("failed to create p2");

        let set_type_p1 = set_partition_type_cmd(1, &manifest_p1);
        let set_type_p2 = set_partition_type_cmd(2, &manifest_p2);

        run_fdisk_cmd(fname, &set_type_p1).expect("failed to set p1 type");
        run_fdisk_cmd(fname, &set_type_p2).expect("failed to set p2 type");
    }
}
