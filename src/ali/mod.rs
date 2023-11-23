pub mod apply;
pub mod validation;

use std::collections::HashSet;

use serde::{
    Deserialize,
    Serialize,
};

use crate::errors::AliError;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(alias = "location", alias = "install_location")]
    pub location: Option<String>,

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

    #[serde(alias = "mountpoint", alias = "mnt")]
    pub mountpoints: Option<Vec<ManifestMountpoint>>,

    pub swap: Option<Vec<String>>,

    #[serde(
        alias = "pacstrap",
        alias = "packages",
        alias = "install",
        alias = "installs"
    )]
    pub pacstraps: Option<HashSet<String>>,

    #[serde(
        alias = "password",
        alias = "passwd",
        alias = "root-password",
        alias = "root-passwd"
    )]
    pub rootpasswd: Option<String>,

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

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
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

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ManifestPartition {
    pub label: String,
    pub size: Option<String>, // e.g. 200M

    #[serde(rename = "type")]
    pub part_type: String,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ManifestFs {
    pub device: String,

    #[serde(alias = "fstype", alias = "filesystem")]
    pub fs_type: String,

    #[serde(alias = "fsopts", alias = "filesystem_options")]
    pub fs_opts: Option<String>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ManifestMountpoint {
    pub device: String,

    #[serde(alias = "mount", alias = "mount_point", alias = "location")]
    pub dest: String,

    #[serde(alias = "mntopts", alias = "mount_options")]
    pub mnt_opts: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestRootFs {
    pub device: String,

    #[serde(alias = "fstype", alias = "filesystem")]
    pub fs_type: String,

    #[serde(alias = "fsopts", alias = "filesystem_options")]
    pub fs_opts: Option<String>,

    #[serde(alias = "mntopts", alias = "mount_options")]
    pub mnt_opts: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestLuks {
    pub device: String,
    pub name: String,

    // If passphrase is None, let cryptsetup prompt user for password,
    // if it is Some(pass), pipe pass to cryptsetup
    #[serde(alias = "key")]
    pub passphrase: Option<String>,
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

impl From<ManifestRootFs> for ManifestFs {
    fn from(rootfs: ManifestRootFs) -> Self {
        ManifestFs {
            device: rootfs.device,
            fs_type: rootfs.fs_type,
            fs_opts: rootfs.fs_opts,
        }
    }
}

impl From<ManifestRootFs> for ManifestMountpoint {
    fn from(rootfs: ManifestRootFs) -> Self {
        ManifestMountpoint {
            device: rootfs.device,
            dest: "/".to_string(),
            mnt_opts: rootfs.mnt_opts,
        }
    }
}

#[inline]
pub fn parse(manifest: &str) -> Result<Manifest, AliError> {
    serde_yaml::from_str(manifest)
        .map_err(|err| AliError::BadManifest(err.to_string()))
}

#[test]
fn test_parse() {
    let example_yaml = include_str!("./examples/uefi-root-on-lvm.yaml");
    let manifest: Manifest = parse(example_yaml).unwrap();

    println!("{:?}", manifest);
}
