pub mod hook_keys {
    pub const WRAPPER_MNT: &str = "@mnt";
    pub const WRAPPER_NO_MNT: &str = "@no-mnt";
    pub const QUICKNET: &str = "@quicknet";
    pub const QUICKNET_PRINT: &str = "@quicknet-print";
    pub const MKINITCPIO: &str = "@mkinitcpio";
    pub const MKINITCPIO_PRINT: &str = "@mkinitcpio-print";
    pub const UNCOMMENT: &str = "@uncomment";
    pub const UNCOMMENT_PRINT: &str = "@uncomment-print";
    pub const UNCOMMENT_ALL: &str = "@uncomment-all";
    pub const UNCOMMENT_ALL_PRINT: &str = "@uncomment-all-print";
    pub const REPLACE_TOKEN: &str = "@replace-token";
    pub const REPLACE_TOKEN_PRINT: &str = "@replace-token-print";
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
