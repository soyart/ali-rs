use serde_json::json;

use super::constants::quicknet::*;
use super::{
    extract_key_and_parts,
    ActionHook,
    Caller,
    Hook,
    ModeHook,
    KEY_QUICKNET,
    KEY_QUICKNET_PRINT,
};
use crate::errors::AliError;
use crate::utils::shell;

#[derive(Debug, Clone, PartialEq)]
struct QuickNet {
    interface: String,
    dns_upstream: Option<String>,
}

struct HookQuickNet {
    qn: Option<QuickNet>,
    mode_hook: ModeHook,
}

pub(super) fn init_from_key(key: &str) -> Box<dyn Hook> {
    Box::new(HookQuickNet {
        qn: None,
        mode_hook: match key {
            KEY_QUICKNET => ModeHook::Normal,
            KEY_QUICKNET_PRINT => ModeHook::Print,
            key => panic!("unexpected key {key}"),
        },
    })
}

impl super::Hook for HookQuickNet {
    fn base_key(&self) -> &'static str {
        KEY_QUICKNET
    }

    /// `@quicknet [dns <DNS_UPSTREAM>] <INTERFACE>`
    ///
    /// Examples:
    ///
    /// 1. Setup simple DHCP for ens3
    ///
    /// ```txt
    /// @quicknet ens3
    /// ```
    ///
    /// 2. Setup simple DHCP and DNS upstream 1.1.1.1 for ens3
    ///
    /// ```txt
    /// @quicknet dns 1.1.1.1 ens3
    /// ```
    fn usage(&self) -> &'static str {
        "interface [dns <DNS_STREAM>]"
    }

    fn mode(&self) -> ModeHook {
        self.mode_hook.clone()
    }

    fn should_chroot(&self) -> bool {
        true
    }

    fn prefer_caller(&self, caller: &Caller) -> bool {
        matches!(caller, Caller::ManifestChroot | Caller::Cli)
    }

    fn abort_if_no_mount(&self) -> bool {
        true
    }

    fn parse_cmd(&mut self, s: &str) -> Result<(), AliError> {
        let result = parse_quicknet(&self.hook_key(), s)?;
        self.qn = Some(result);

        Ok(())
    }

    fn run_hook(
        &self,
        _caller: &Caller,
        root_location: &str,
    ) -> Result<ActionHook, AliError> {
        apply_quicknet(
            &self.hook_key(),
            &self.mode_hook,
            self.qn.as_ref().unwrap(),
            root_location,
        )
    }
}

impl TryFrom<&str> for HookQuickNet {
    type Error = AliError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let (hook_key, parts) = extract_key_and_parts(s)?;
        let mut hook = HookQuickNet {
            qn: None,
            mode_hook: match hook_key.as_str() {
                KEY_QUICKNET => ModeHook::Normal,
                KEY_QUICKNET_PRINT => ModeHook::Print,
                key => panic!("unexpected key {key}"),
            },
        };

        let qn = match parts.len() {
            2 => {
                let interface = parts.get(1).unwrap();
                if interface == "dns" {
                    return Err(AliError::BadHookCmd(format!(
                        "{hook_key}: got only keyword `dns`"
                    )));
                }

                QuickNet {
                    interface: interface.to_string(),
                    dns_upstream: None,
                }
            }

            4 => {
                let mut dns_keyword_idx = None;
                for (i, word) in parts.iter().enumerate() {
                    if *word == "dns" {
                        dns_keyword_idx = Some(i);

                        break;
                    }
                }

                if dns_keyword_idx.is_none() {
                    return Err(AliError::BadHookCmd(format!(
                        "{hook_key}: missing argument keyword \"dns\""
                    )));
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
                        return Err(AliError::BadHookCmd(format!(
                        "{hook_key}: \"dns\" keyword in bad position: {dns_keyword_idx}"
                    )));
                    }
                };

                QuickNet {
                    interface: parts[interface_idx].to_string(),
                    dns_upstream: Some(parts[dns_keyword_idx + 1].to_string()),
                }
            }

            l => {
                return Err(AliError::BadHookCmd(format!(
                    "{hook_key}: unexpected cmd parts length: {l}"
                )));
            }
        };

        hook.qn = Some(qn);
        Ok(hook)
    }
}

fn parse_quicknet(hook_key: &str, cmd: &str) -> Result<QuickNet, AliError> {
    let (key, parts) = super::extract_key_and_parts(cmd)?;
    if !matches!(key.as_str(), KEY_QUICKNET | KEY_QUICKNET_PRINT,) {
        return Err(AliError::BadHookCmd(format!(
            "{hook_key}: bad cmd: 1st part does not start with \"@quicknet\""
        )));
    }

    match parts.len() {
        2 => {
            let interface = parts.get(1).unwrap();
            if interface == "dns" {
                return Err(AliError::BadHookCmd(format!(
                    "{hook_key}: got only keyword `dns`"
                )));
            }

            Ok(QuickNet {
                interface: interface.to_string(),
                dns_upstream: None,
            })
        }

        4 => {
            let mut dns_keyword_idx = None;
            for (i, word) in parts.iter().enumerate() {
                if *word == "dns" {
                    dns_keyword_idx = Some(i);

                    break;
                }
            }

            if dns_keyword_idx.is_none() {
                return Err(AliError::BadHookCmd(format!(
                    "{hook_key}: missing argument keyword \"dns\""
                )));
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
                    return Err(AliError::BadHookCmd(format!(
                        "{hook_key}: \"dns\" keyword in bad position: {dns_keyword_idx}"
                    )));
                }
            };

            Ok(QuickNet {
                interface: parts[interface_idx].to_string(),
                dns_upstream: Some(parts[dns_keyword_idx + 1].to_string()),
            })
        }

        l => {
            Err(AliError::BadHookCmd(format!(
                "{hook_key}: unexpected cmd parts length: {l}"
            )))
        }
    }
}

/// Creates directory "{root_location}/etc/systemd/network/"
/// and write networkd quicknet config file for it
fn apply_quicknet(
    hook_key: &str,
    mode_hook: &ModeHook,
    qn: &QuickNet,
    root_location: &str,
) -> Result<ActionHook, AliError> {
    // Formats filename and string output
    let filename = FILENAME_TPL.replace(TOKEN_INTERFACE, &qn.interface);
    let filename = format!("{root_location}/{filename}");
    let conf_str = qn.encode_to_string();

    match mode_hook {
        ModeHook::Print => {
            println!("{conf_str}");
        }
        ModeHook::Normal => {
            // Extends to include systemd path
            let root_location = format!("{root_location}/etc/systemd/network");
            shell::exec("mkdir", &["-p", &root_location])?;

            std::fs::write(&filename, conf_str).map_err(|err| {
                AliError::FileError(
                    err,
                    format!("{hook_key}: writing file {filename}"),
                )
            })?;
        }
    }

    Ok(ActionHook::QuickNet(qn.to_string()))
}

impl ToString for QuickNet {
    fn to_string(&self) -> String {
        json!({
            "interface": self.interface,
            "dns_upstream": self.dns_upstream,
        })
        .to_string()
    }
}

impl QuickNet {
    fn encode_to_string(&self) -> String {
        let mut s = NETWORKD_DHCP.replace(TOKEN_INTERFACE, &self.interface);
        if let Some(ref upstream) = self.dns_upstream {
            let dns_conf = NETWORKD_DNS.replace(TOKEN_DNS, upstream);

            s = format!("{s}\n{dns_conf}");
        }

        s
    }
}

#[test]
fn test_parse_quicknet() {
    let should_pass = vec![
        (
            "@quicknet eth0",
            QuickNet {
                interface: "eth0".into(),
                dns_upstream: None,
            },
        ),
        (
            "@quicknet inf",
            QuickNet {
                interface: "inf".into(),
                dns_upstream: None,
            },
        ),
        (
            "@quicknet dns 1.1.1.1 eth0",
            QuickNet {
                interface: "eth0".into(),
                dns_upstream: Some("1.1.1.1".into()),
            },
        ),
        (
            "@quicknet eth0 dns 1.1.1.1",
            QuickNet {
                interface: "eth0".into(),
                dns_upstream: Some("1.1.1.1".into()),
            },
        ),
    ];

    let should_err =
        vec!["@quicknet", "@quicknet dns", "@quicknet eth0 1.1.1.1 dns"];

    for (cmd, expected_qn) in should_pass {
        let hook_result = HookQuickNet::try_from(cmd);
        if let Err(ref err) = hook_result {
            eprintln!("unexpected error result from {cmd}: {err}");
        }

        let qn = hook_result.unwrap().qn.unwrap();
        assert_eq!(expected_qn, qn);
    }

    for cmd in &should_err {
        let hook_result = HookQuickNet::try_from(*cmd);
        if let Ok(hook) = hook_result {
            panic!(
                "unexpected ok result from bad arg {cmd}: {}",
                hook.qn.unwrap().to_string()
            );
        }
    }
}

#[test]
fn test_quicknet_encode() {
    use std::collections::HashMap;

    let tests = HashMap::from([
        (
            "@quicknet eth0",
            r#"# Installed by ali-rs hook @quicknet
[Match]
Name=eth0

[Network]
DHCP=yes
"#,
        ),
        (
            "@quicknet eth0 dns 9.9.9.9",
            r#"# Installed by ali-rs hook @quicknet
[Match]
Name=eth0

[Network]
DHCP=yes

# Installed by ali-rs hook @quicknet
DNS=9.9.9.9
"#,
        ),
        (
            "@quicknet dns 8.8.8.8 ens3",
            r#"# Installed by ali-rs hook @quicknet
[Match]
Name=ens3

[Network]
DHCP=yes

# Installed by ali-rs hook @quicknet
DNS=8.8.8.8
"#,
        ),
    ]);

    for (cmd, expected) in tests {
        let hook = HookQuickNet::try_from(cmd).unwrap();
        let s = hook.qn.unwrap().encode_to_string();

        assert_eq!(expected, s);
    }
}
