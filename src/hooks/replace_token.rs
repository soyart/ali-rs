use super::utils::{
    self,
    download,
};
use super::{
    wrap_hook_parse_help,
    ActionHook,
    Caller,
    Hook,
    ModeHook,
    ParseError,
    KEY_REPLACE_TOKEN,
    KEY_REPLACE_TOKEN_DEBUG,
};
use crate::errors::AliError;

const USAGE: &str = "<TOKEN> <VALUE> <TEMPLATE> [OUTPUT]";

#[derive(Debug, PartialEq)]
struct HookReplaceToken {
    mode_hook: ModeHook,
    output: String,
    rp: utils::ReplaceToken,
    template: String,
}

pub(super) fn parse(k: &str, cmd: &str) -> Result<Box<dyn Hook>, ParseError> {
    match k {
        KEY_REPLACE_TOKEN | KEY_REPLACE_TOKEN_DEBUG => {
            match HookReplaceToken::try_from(cmd) {
                Err(err) => Err(wrap_hook_parse_help(err, USAGE)),
                Ok(hook) => Ok(Box::new(hook)),
            }
        }

        key => panic!("unknown {key}"),
    }
}

impl Hook for HookReplaceToken {
    fn base_key(&self) -> &'static str {
        KEY_REPLACE_TOKEN
    }

    fn usage(&self) -> &'static str {
        "<TOKEN> <VALUE> <TEMPLATE> [OUTPUT]"
    }

    fn mode(&self) -> ModeHook {
        self.mode_hook.clone()
    }

    fn should_chroot(&self) -> bool {
        false
    }

    fn prefer_caller(&self, _c: &Caller) -> bool {
        true
    }

    fn abort_if_no_mount(&self) -> bool {
        false
    }

    fn run_hook(
        &self,
        _caller: &Caller,
        root_location: &str,
    ) -> Result<ActionHook, AliError> {
        apply_replace_token(
            &self.key(),
            &self.mode_hook,
            &self.rp,
            root_location,
            &self.template,
            &self.output,
        )
    }
}

/// Synopsis
///
/// ```txt
/// @replace-token <TOKEN> <VALUE> <TEMPLATE> [OUTPUT]`
/// ```
///
/// TOKEN must exist in TEMPLATE file, as {{ TOKEN }},
/// e.g. TOKEN=foo, then there exists {{ foo }} in TEMPLATE file
///
/// If OUTPUT is not given, output is written to TEMPLATE file
///
/// Examples:
/// ```txt
/// @replace-token PORT 2222 /etc_templates/ssh/sshd_config /etc/ssh/sshd_config
///
/// ==> Replace key PORT value with "2222", using /etc_templates/ssh/sshd_config as template and writes output to /etc/ssh/sshd_config
/// ```
impl TryFrom<&str> for HookReplaceToken {
    type Error = AliError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let (hook_key, parts) = super::extract_key_and_parts_shlex(s)?;
        let mode_hook = match hook_key.as_str() {
            KEY_REPLACE_TOKEN => ModeHook::Normal,
            KEY_REPLACE_TOKEN_DEBUG => ModeHook::Debug,
            key => {
                panic!("unexpected key '{key}'")
            }
        };

        if parts.len() < 3 {
            return Err(AliError::HookParse(format!(
                "{hook_key}: expect at least 2 arguments"
            )));
        }

        let l = parts.len();
        if l != 4 && l != 5 {
            return Err(AliError::HookParse(format!(
                "{hook_key}: bad cmd parts (expecting 3-4): {l}"
            )));
        }

        let (token, value, template) =
            (parts[1].clone(), parts[2].clone(), parts[3].clone());

        // If not given, then use template as output
        let output = parts
            .last()
            .map(|s| s.to_owned())
            .unwrap_or(template.clone());

        Ok(HookReplaceToken {
            mode_hook,
            template,
            output,
            rp: utils::ReplaceToken { token, value },
        })
    }
}

fn apply_replace_token(
    hook_key: &str,
    mode_hook: &ModeHook,
    r: &utils::ReplaceToken,
    root_location: &str,
    template: &str,
    output: &str,
) -> Result<ActionHook, AliError> {
    let template_string =
        // If the template is a valid remote URL, download it
        if let Ok(downloader) = download::Downloader::new_from_url(template) {
            downloader.get_string()

        // Otherwise read from file
        } else {
            std::fs::read_to_string(template).map_err(|err| {
                AliError::HookApply(format!(
                    "{hook_key}: read template {}: {err}",
                    template
                ))
            })
        }?;

    let replaced = r.replace(&template_string)?;

    match mode_hook {
        ModeHook::Debug => {
            println!("{replaced}")
        }

        ModeHook::Normal => {
            let output_location = match root_location {
                "/" => output.to_string(),
                _ => format!("/{root_location}/{output}"),
            };

            std::fs::write(output_location, replaced).map_err(|err| {
                AliError::HookApply(format!(
                    "{hook_key}: failed to write to output to {output}: {err}",
                ))
            })?;
        }
    }

    Ok(ActionHook::ReplaceToken(r.to_string()))
}

#[test]
fn test_parse_replace_token() {
    use std::collections::HashMap;

    let should_pass = vec![
        "@replace-token PORT 3322 /etc/ssh/sshd",
        "@replace-token foo bar https://example.com/template /some/file",
        "@replace-token linux_boot \"loglevel=3 quiet root=/dev/archvg/archlv ro\" /etc/default/grub",
        "@replace-token \"linux boot\" \"loglevel=3 quiet root=/dev/archvg/archlv ro\" /some/template /etc/default/grub",
    ];

    let should_err = vec![
        "@replace-token PORT",
        "@replace-token PORT 3322",
        "@replace-token PORT \"3322\"",
        "@replace-token PORT \"3322 /some/template",
        "@replace-token PORT \"3322 /some/template /some/output",
    ];

    for cmd in should_pass {
        let result = HookReplaceToken::try_from(cmd);
        if let Err(err) = result {
            panic!("got error from cmd {cmd}: {err}");
        }
    }

    for cmd in should_err {
        let result = HookReplaceToken::try_from(cmd);
        if let Ok(HookReplaceToken { rp: qn, .. }) = result {
            panic!("got ok result from bad arg {cmd}: {}", qn.to_string());
        }
    }

    let tests = HashMap::from([
        (
            "@replace-token-debug PORT 3322 /etc/ssh/sshd",
            HookReplaceToken {
                mode_hook: ModeHook::Debug,
                template: "/etc/ssh/sshd".to_string(),
                output: "/etc/ssh/sshd".to_string(),
                rp: utils::ReplaceToken {
                    token: "PORT".to_string(),
                    value: "3322".to_string(),
                },
            }
        ),
        (
            "@replace-token linux_boot \"loglevel=3 quiet root=/dev/archvg/archlv ro\" /etc/default/grub",
            HookReplaceToken {
                mode_hook: ModeHook::Normal,
                template: "/etc/default/grub".to_string(),
                output: "/etc/default/grub".to_string(),
                rp: utils::ReplaceToken {
                    token: "linux_boot".to_string(),
                    value: "loglevel=3 quiet root=/dev/archvg/archlv ro".to_string(),
                },
            }
        ),
        (
            "@replace-token-debug \"linux_boot\" \"loglevel=3 quiet root=/dev/archvg/archlv ro\" /some/template /etc/default/grub",
            HookReplaceToken {
                mode_hook: ModeHook::Debug,
                template: "/some/template".to_string(),
                output: "/etc/default/grub".to_string(),
                rp: utils::ReplaceToken {
                    token: "linux_boot".to_string(),
                    value: "loglevel=3 quiet root=/dev/archvg/archlv ro".to_string(),
                },
            }
        ),
    ]);

    for (cmd, expected) in tests {
        let actual = HookReplaceToken::try_from(cmd).unwrap();
        assert_eq!(expected, actual);
    }
}
