pub mod hook_keys {
    pub const KEY_WRAPPER_MNT: &str = "@mnt";
    pub const KEY_WRAPPER_NO_MNT: &str = "@no-mnt";
    pub const KEY_QUICKNET: &str = "@quicknet";
    pub const KEY_QUICKNET_DEBUG: &str = "@quicknet-debug";
    pub const KEY_MKINITCPIO: &str = "@mkinitcpio";
    pub const KEY_MKINITCPIO_DEBUG: &str = "@mkinitcpio-debug";
    pub const KEY_UNCOMMENT: &str = "@uncomment";
    pub const KEY_UNCOMMENT_DEBUG: &str = "@uncomment-debug";
    pub const KEY_UNCOMMENT_ALL: &str = "@uncomment-all";
    pub const KEY_UNCOMMENT_ALL_DEBUG: &str = "@uncomment-all-debug";
    pub const KEY_REPLACE_TOKEN: &str = "@replace-token";
    pub const KEY_REPLACE_TOKEN_DEBUG: &str = "@replace-token-debug";
    pub const KEY_DOWNLOAD: &str = "@download";
    pub const KEY_DOWNLOAD_DEBUG: &str = "@download-debug";
}

pub mod quicknet {
    pub const TOKEN_INTERFACE: &str = "{{ inf }}";

    pub const TOKEN_DNS: &str = "{{ dns_upstream }}";

    pub const FILENAME_TPL: &str = "00-dhcp_{{ inf }}-quicknet.conf";

    pub const NETWORKD_DHCP: &str = r#"# Installed by ali-rs hook @quicknet
[Match]
Name={{ inf }}

[Network]
DHCP=yes
"#;

    pub const NETWORKD_DNS: &str = r#"# Installed by ali-rs hook @quicknet
DNS={{ dns_upstream }}
"#;

    #[test]
    fn test_tokens() {
        assert!(FILENAME_TPL.contains(TOKEN_INTERFACE));
        assert!(NETWORKD_DHCP.contains(TOKEN_INTERFACE));
        assert!(NETWORKD_DNS.contains(TOKEN_DNS));
    }
}

pub mod mkinitcpio {
    pub const MKINITCPIO_PRESET_LVM_ROOT: &str =
        "base udev autodetect modconf kms keyboard keymap consolefont block lvm2 filesystems fsck";
    pub const MKINITCPIO_PRESET_LUKS_ROOT: &str = "@TODO-luks";
    pub const MKINITCPIO_PRESET_LVM_ON_LUKS_ROOT: &str = "@TODO-lvm-on-luks";
    pub const MKINITCPIO_PRESET_LUKS_ON_LVM_ROOT: &str = "@TODO-luks-on-lvm";
}
