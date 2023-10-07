pub mod quicknet {
    pub const TOKEN_INTERFACE: &str = "{{ inf }}";

    pub const TOKEN_DNS: &str = "{{ dns_upstream }}";

    pub const FILENAME: &str = "00-dhcp_{{ inf }}-quicknet.conf";

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
        assert!(FILENAME.contains(TOKEN_INTERFACE));
        assert!(NETWORKD_DHCP.contains(TOKEN_INTERFACE));
        assert!(NETWORKD_DNS.contains(TOKEN_DNS));
    }
}

pub mod mkinitcpio {
    pub const MKINITCPIO_HOOKS_LVM_ROOT: &str =
        "base udev autodetect modconf kms keyboard keymap consolefont block lvm2 filesystems fsck";
    pub const MKINITCPIO_HOOKS_LUKS_ROOT: &str = "@TODO-luks";
    pub const MKINITCPIO_HOOKS_LVM_ON_LUKS_ROOT: &str = "@TODO-lvm-on-luks";
    pub const MKINITCPIO_HOOKS_LUKS_ON_LVM_ROOT: &str = "@TODO-luks-on-lvm";
}
