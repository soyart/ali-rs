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
