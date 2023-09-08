pub const QUICKNET_DHCP_FILENAME: &str = "00-dhcp_{{inf}}-quicknet.conf";

pub const QUICKNET_DHCP: &str = r#"# Installed by ali-rs hook #quicknet
[Match]
Name={{inf}}

[Network]
DHCP=yes
"#;

pub const QUICKNET_DNS: &str = r#"# Installed by ali-rs hook #quicknet
DNS={{dns_upstream}}
"#;
