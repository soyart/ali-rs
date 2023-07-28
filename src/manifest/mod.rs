use serde::{Deserialize, Serialize};

use crate::errors::AyiError;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub hostname: String,

    pub disks: Vec<ManifestDisk>,
    pub dm: Vec<Dm>,
    pub rootfs: ManifestRootFs,

    #[serde(rename = "fs")]
    pub filesystems: Vec<ManifestFs>,

    pub swap: ManifestSwap,

    #[serde(rename = "pacstrap")]
    pub pacstraps: Vec<String>,

    pub chroot: Vec<String>,
    pub postinstall: Vec<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum PartitionTable {
    #[serde(rename = "gpt")]
    Gpt,

    #[serde(rename = "mbr", alias = "dos", alias = "mbr-dos")]
    Mbr,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestDisk {
    pub device: String,
    pub table: PartitionTable,
    pub partitions: Vec<ManifestPartition>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestPartition {
    pub label: String,
    pub size: Option<String>, // e.g. 200M

    #[serde(rename = "type")]
    pub part_type: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestFs {
    pub device: String,

    #[serde(rename = "fstype")]
    pub fs_type: String,

    #[serde(rename = "fsopts")]
    pub fs_opts: String,

    #[serde(rename = "mntopts")]
    pub mnt_opts: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestRootFs(pub ManifestFs);

impl std::ops::Deref for ManifestRootFs {
    type Target = ManifestFs;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestSwap {
    pub device: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestLuks {
    pub device: String,
    pub name: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestLvmVg {
    pub name: String,
    pub pvs: Vec<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestLvmLv {
    pub name: String,
    pub vg: String,
    pub size: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestLvm {
    pub pvs: Vec<String>,
    pub vgs: Vec<ManifestLvmVg>,
    pub lvs: Vec<ManifestLvmLv>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Dm {
    #[serde(rename = "luks")]
    Luks(ManifestLuks),

    #[serde(rename = "lvm")]
    Lvm(ManifestLvm),
}

pub fn parse_manifest(manifest: &str) -> Result<Manifest, AyiError> {
    serde_yaml::from_str(manifest).map_err(|err| AyiError::BadManifest(err.to_string()))
}

#[test]
fn test_parse() {
    use crate::utils::fs::file_exists;

    let fname = "../../../examples/uefi-root-on-lvm.yaml";
    if file_exists(fname) {
        let example_yaml = include_str!("../../../examples/uefi-root-on-lvm.yaml");
        let manifest: Manifest = parse_manifest(example_yaml).unwrap();

        println!("{:?}", manifest);
    }

    println!("skipping test_parse - missing manifest {fname}");
}
