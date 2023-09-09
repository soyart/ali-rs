use serde::Serialize;

use super::{
    constants::{QUICKNET_DHCP, QUICKNET_DNS, QUICKNET_FILENAME},
    ActionHook,
};
use crate::errors::AliError;
use crate::utils::shell;

#[derive(Serialize)]
struct QuickNet<'a> {
    interface: &'a str,
    dns_upstream: Option<&'a str>,
}

pub(super) fn quicknet(cmd_string: &str, root_location: &str) -> Result<ActionHook, AliError> {
    let qn = parse_quicknet(cmd_string)?;

    apply_quicknet(qn, root_location)
}

// #quicknet [dns <DNS_UPSTREAM>] <INTERFACE>
// Examples:
// #quicknet ens3 ==> Setup simple DHCP for ens3
// #quicknet dns 1.1.1.1 ens3 => Setup simple DHCP and DNS upstream 1.1.1.1 for ens3
fn parse_quicknet(cmd: &str) -> Result<QuickNet, AliError> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let l = parts.len();
    if l <= 1 {
        return Err(AliError::BadArgs(
            "#quicknet: bad arguments: only 1 string is supplied".to_string(),
        ));
    }

    if parts[0] != "#quicknet" {
        return Err(AliError::BadArgs(
            "#quicknet: bad arguments: first part is not \"#quicknet\"".to_string(),
        ));
    }

    match l {
        4 => {
            let mut dns_keyword_idx = None;
            for (i, word) in parts.iter().enumerate() {
                if *word == "dns" {
                    // We skipped 1 in iter
                    dns_keyword_idx = Some(i);
                    break;
                }
            }

            if dns_keyword_idx.is_none() {
                return Err(AliError::BadArgs(
                    "#quicknet: missing argument keyword \"dns\"".to_string(),
                ));
            }
            // #cmd dns upstream inf  1
            // #cmd inf dns upstream  2
            let dns_keyword_idx = dns_keyword_idx.unwrap();
            let interface_idx = {
                if dns_keyword_idx == 1 {
                    3
                } else if dns_keyword_idx == 2 {
                    1
                } else {
                    return Err(AliError::BadArgs(format!(
                        "#quicknet: \"dns\" keyword in bad position: {dns_keyword_idx}"
                    )));
                }
            };

            Ok(QuickNet {
                interface: parts[interface_idx],
                dns_upstream: Some(parts[dns_keyword_idx + 1]),
            })
        }
        2 => {
            let interface = parts[1];
            if interface == "dns" {
                return Err(AliError::BadArgs("got only dns keyword".to_string()));
            }

            Ok(QuickNet {
                interface,
                dns_upstream: None,
            })
        }
        _ => Err(AliError::BadArgs(format!(
            "bad quicknet arguments length: {l}"
        ))),
    }
}

/// Creates directory "{root_location}/etc/systemd/network/"
/// and write networkd quicknet config file for it
fn apply_quicknet(qn: QuickNet, root_location: &str) -> Result<ActionHook, AliError> {
    // Extends to include systemd path
    let root_location = format!("{root_location}/etc/systemd/network");
    shell::exec("mkdir", &["-p", &root_location])?;

    let filename = QUICKNET_FILENAME.replace("{{inf}}", qn.interface);
    let filename = format!("{root_location}/{filename}");

    std::fs::write(&filename, qn.encode_to_string())
        .map_err(|err| AliError::FileError(err, format!("writing file {filename}")))?;

    Ok(ActionHook::QuickNet(qn.to_string()))
}

impl<'a> ToString for QuickNet<'a> {
    fn to_string(&self) -> String {
        serde_json::to_string(&self).expect("failed to serialize to JSON")
    }
}

impl<'a> QuickNet<'a> {
    fn encode_to_string(&self) -> String {
        serde_json::to_string(&self).expect("failed to serialize to JSON");
        let mut s = QUICKNET_DHCP.replace("{{inf}}", self.interface);
        if let Some(upstream) = self.dns_upstream {
            let dns_conf = QUICKNET_DNS.replace("{{dns_upstream}}", upstream);

            s = format!("{s}\n{dns_conf}");
        }

        s
    }
}

#[test]
fn test_parse_quicknet() {
    let should_pass = vec![
        "#quicknet eth0",
        "#quicknet inf",
        "#quicknet dns 1.1.1.1 eth0",
        "#quicknet eth0 dns 1.1.1.1",
    ];
    let should_err = vec![
        "eth0",
        "#quicknet",
        "#quicknet dns",
        "#quicknet eth0 1.1.1.1 dns",
        "#quickmet eth0 dns",
    ];

    for cmd in should_pass {
        let result = parse_quicknet(cmd);
        if let Err(err) = result {
            panic!("got error from cmd {cmd}: {err}");
        }
    }

    for cmd in should_err {
        let result = parse_quicknet(cmd);
        if result.is_ok() {
            let result = result.unwrap();
            panic!("got ok result from bad arg {cmd}: {}", result.to_string());
        }
    }
}

#[test]
fn test_quicknet_encode() {
    use std::collections::HashMap;

    let tests = HashMap::from([
        (
            "#quicknet eth0",
            r#"# Installed by ali-rs hook #quicknet
[Match]
Name=eth0

[Network]
DHCP=yes
"#,
        ),
        (
            "#quicknet eth0 dns 9.9.9.9",
            r#"# Installed by ali-rs hook #quicknet
[Match]
Name=eth0

[Network]
DHCP=yes

# Installed by ali-rs hook #quicknet
DNS=9.9.9.9
"#,
        ),
        (
            "#quicknet dns 8.8.8.8 ens3",
            r#"# Installed by ali-rs hook #quicknet
[Match]
Name=ens3

[Network]
DHCP=yes

# Installed by ali-rs hook #quicknet
DNS=8.8.8.8
"#,
        ),
    ]);

    for (cmd, expected) in tests {
        let qn = parse_quicknet(cmd).unwrap();
        let s = qn.encode_to_string();

        assert_eq!(expected, s);
    }
}