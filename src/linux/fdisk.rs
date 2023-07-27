use crate::errors::AyiError;
use crate::manifest::{ManifestPartition, PartitionTable};
use crate::utils::shell;

pub fn create_table_cmd(device: &str, table: &PartitionTable) -> String {
    match table {
        PartitionTable::Gpt => "g\n".to_string(),
        PartitionTable::Mbr => "o\n".to_string(),
    }
}

pub fn create_partition_cmd(
    table: &PartitionTable,
    part_num: usize,
    part: &ManifestPartition,
) -> String {
    let size = match part.size {
        Some(ref s) => format!("+{s}"),
        None => "\n".to_string(),
    };

    match table {
        PartitionTable::Gpt => join_newlines(&[
            "n",
            &part_num.to_string(),
            "\n",
            &size,
            "t",
            &part.part_type,
        ]),
        PartitionTable::Mbr => join_newlines(&[
            "n",
            "p", // Only create primary msdos partition for now
            &part_num.to_string(),
            "\n",
            &size,
            "t",
            &part.part_type,
        ]),
    }
}

pub fn run_fdisk_cmd(device: &str, cmd: &str) -> Result<(), AyiError> {
    shell::exec("printf", &[cmd, &format!("| fdisk {device}")])?;

    Ok(())
}

fn join_newlines(slice: &[&str]) -> String {
    let mut joined = slice.join("\n");
    joined.push_str("\n");

    return joined;
}

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
            expected: "n\n1\n\n\n+200M\nt\n8e\n",
        },
        Test {
            table: PartitionTable::Mbr,
            num: 1,
            part: ManifestPartition {
                label: "foo".to_string(),
                size: None,
                part_type: "8e".to_string(),
            },
            expected: "n\np\n1\n\n\n\n\nt\n8e\n",
        },
    ];

    for test in tests {
        let result = create_partition_cmd(&test.table, test.num, &test.part);
        assert_eq!(test.expected, result);
    }
}
