pub mod validation;

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::errors::AliError;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(alias = "name", alias = "host")]
    pub hostname: Option<String>,

    #[serde(alias = "tz")]
    pub timezone: Option<String>,

    #[serde(alias = "root")]
    pub rootfs: ManifestRootFs,

    pub disks: Option<Vec<ManifestDisk>>,

    #[serde(alias = "device-mappers", alias = "dm", alias = "dms")]
    pub device_mappers: Option<Vec<Dm>>,

    #[serde(alias = "fs", alias = "filesystem")]
    pub filesystems: Option<Vec<ManifestFs>>,

    pub swap: Option<Vec<String>>,

    #[serde(
        alias = "pacstrap",
        alias = "packages",
        alias = "install",
        alias = "installs"
    )]
    pub pacstraps: Option<HashSet<String>>,

    #[serde(alias = "arch-chroot")]
    pub chroot: Option<Vec<String>>,

    #[serde(alias = "post-install")]
    pub postinstall: Option<Vec<String>>,
}

impl Manifest {
    #[inline]
    pub fn from_yaml(manifest_yaml: &str) -> Result<Self, AliError> {
        parse(manifest_yaml)
    }
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

    #[serde(alias = "mount_point")]
    pub mnt: String,

    #[serde(alias = "fstype")]
    pub fs_type: String,

    #[serde(alias = "fsopts")]
    pub fs_opts: Option<String>,

    #[serde(alias = "mntopts")]
    pub mnt_opts: Option<String>,
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
pub struct ManifestLuks {
    pub device: String,
    pub name: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestLvmVg {
    pub name: String,
    pub pvs: Vec<String>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ManifestLvmLv {
    pub name: String,
    pub vg: String,
    pub size: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestLvm {
    pub pvs: Option<Vec<String>>,
    pub vgs: Option<Vec<ManifestLvmVg>>,
    pub lvs: Option<Vec<ManifestLvmLv>>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Dm {
    #[serde(rename = "luks")]
    Luks(ManifestLuks),

    #[serde(rename = "lvm")]
    Lvm(ManifestLvm),
}

#[inline]
pub fn parse(manifest: &str) -> Result<Manifest, AliError> {
    serde_yaml::from_str(manifest).map_err(|err| AliError::BadManifest(err.to_string()))
}

#[test]
fn test_parse() {
    let example_yaml = include_str!("./examples/uefi-root-on-lvm.yaml");
    let manifest: Manifest = parse(example_yaml).unwrap();

    println!("{:?}", manifest);
}
