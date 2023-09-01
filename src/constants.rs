pub mod defaults {
    pub const TIMEZONE: &str = "America/Los_Angeles";
    pub const INSTALL_LOCATION: &str = "/alitarget";
    pub const HOSTNAME: &str = "arch-ali";
    pub const LOCALE_GEN: &str = "en_US.UTF-8 UTF-8";
    pub const LOCALE_CONF: &str = "LANG=en_US.UTF-8";
}

pub const ENV_ALI_LOC: &str = "ALI_LOC";

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
