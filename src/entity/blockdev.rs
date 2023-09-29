use serde::{Deserialize, Serialize};
use std::collections::LinkedList;

#[derive(Debug, PartialEq, Eq, std::hash::Hash, Clone, Serialize, Deserialize)]
pub struct BlockDev {
    // Full path to device, with hard-coded format:
    // Disk => /dev/dev_name
    // PV   => /dev/pv_name          (same as disk)
    // VG   => /dev/vg_name
    // LV   => /dev/vg_name/lv_name
    // LUKS => /dev/mapper/luks_name
    pub device: String,
    pub device_type: BlockDevType,
}

#[derive(Debug, PartialEq, Eq, std::hash::Hash, Clone, Serialize, Deserialize)]
pub enum DmType {
    Luks,
    LvmPv,
    LvmVg,
    LvmLv,
}

#[derive(Debug, PartialEq, Eq, std::hash::Hash, Clone, Serialize, Deserialize)]
pub enum BlockDevType {
    // Disks
    Disk,

    // Disk partitions (GPT/MS-DOS)
    Partition,

    // UnknownBlock is anything that be can build filesystem, LUKS, and LVM PV on
    UnknownBlock,

    // Software-defined storage
    Dm(DmType),

    // Filesystems
    Fs(String),
}

// Type aliases
pub const TYPE_DISK: BlockDevType = BlockDevType::Disk;
pub const TYPE_PART: BlockDevType = BlockDevType::Partition;
pub const TYPE_UNKNOWN: BlockDevType = BlockDevType::UnknownBlock;
pub const TYPE_LUKS: BlockDevType = BlockDevType::Dm(DmType::Luks);
pub const TYPE_PV: BlockDevType = BlockDevType::Dm(DmType::LvmPv);
pub const TYPE_VG: BlockDevType = BlockDevType::Dm(DmType::LvmVg);
pub const TYPE_LV: BlockDevType = BlockDevType::Dm(DmType::LvmLv);

// Block device building blocks are modeled as linked list
pub type BlockDevPath = LinkedList<BlockDev>;

// If the path forks, e.g. a VG has multiple LVs, then a separate list is created.
// For example, if we have 2 LVs foolv and barlv on myvg, itself on PVs /dev/sda1 and /dev/sdb2,
// then the paths will look like this:
//
// 1. [/dev/sda -> /dev/sda1 -> /dev/sda1(pv) -> /dev/myvg -> /dev/myvg/foolv]
// 3. [/dev/sdb -> /dev/sdb2 -> /dev/sdb2(pv) -> /dev/myvg -> /dev/myvg/foolv]
// 2. [/dev/sda -> /dev/sda1 -> /dev/sda1(pv) -> /dev/myvg -> /dev/myvg/barlv]
// 4. [/dev/sdb -> /dev/sdb2 -> /dev/sdb2(pv) -> /dev/myvg -> /dev/myvg/barlv]
pub type BlockDevPaths = Vec<BlockDevPath>;
