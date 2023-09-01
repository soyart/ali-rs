pub const DEFAULT_TIMEZONE: &str = "America/Los_Angeles";

// Use programs instead of bindings to avoid API dependencies
pub const REQUIRED_COMMANDS: [&str; 12] = [
    "arch-chroot",
    "fdisk",
    "blkid",
    "pvs",
    "lvs",
    "vgs",
    "cryptsetup",
    "pvcreate",
    "vgcreate",
    "lvcreate",
    "genfstab",
    "echo",
];
