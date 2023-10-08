pub mod defaults {

    pub const TIMEZONE: &str = "America/Los_Angeles";
    pub const INSTALL_LOCATION: &str = "/alitarget";
    pub const HOSTNAME: &str = "arch-ali";
    pub const LOCALE_GEN: &str = "en_US.UTF-8 UTF-8";
    pub const LOCALE_CONF: &str = "LANG=en_US.UTF-8";

    const ROOT_PASSWD: &str = "archalirs";

    pub fn hashed_password() -> String {
        let h = pwhash::bcrypt::hash(ROOT_PASSWD)
            .expect("failed to generate default bcrypt hashed password");

        if !pwhash::unix::verify(ROOT_PASSWD, &h) {
            panic!(
                "ali-rs bug: failed to verify default bcrypt hashed password"
            )
        }

        h
    }

    #[test]
    fn test_hashed_password() {
        hashed_password();
    }
}

pub const ENV_ALI_LOC: &str = "ALI_LOC";

// Use programs instead of bindings to avoid API dependencies
pub const REQUIRED_COMMANDS: [&str; 15] = [
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
    "printf",
    "openssl",
    "chpasswd",
];
