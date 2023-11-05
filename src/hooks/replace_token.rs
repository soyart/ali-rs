use serde_json::json;

use super::utils::ReplaceToken;
use super::{
    wrap_bad_hook_cmd,
    ActionHook,
    Caller,
    Hook,
    ModeHook,
    ParseError,
    KEY_REPLACE_TOKEN,
    KEY_REPLACE_TOKEN_PRINT,
};
use crate::errors::AliError;

const USAGE: &str = "<TOKEN> <VALUE> <TEMPLATE> [OUTPUT]";

struct HookReplaceToken {
    rp: ReplaceToken,
    mode_hook: ModeHook,
}

pub(super) fn parse(k: &str, cmd: &str) -> Result<Box<dyn Hook>, ParseError> {
    match k {
        KEY_REPLACE_TOKEN | KEY_REPLACE_TOKEN_PRINT => {
            match HookReplaceToken::try_from(cmd) {
                Err(err) => Err(wrap_bad_hook_cmd(err, USAGE)),
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
            &self.hook_key(),
            &self.mode_hook,
            &self.rp,
            root_location,
        )
    }
}

impl TryFrom<&str> for HookReplaceToken {
    type Error = AliError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let (hook_key, parts) = super::extract_key_and_parts_shlex(s)?;
        let mode_hook = match hook_key.as_str() {
            KEY_REPLACE_TOKEN => ModeHook::Normal,
            KEY_REPLACE_TOKEN_PRINT => ModeHook::Print,
            key => {
                return Err(AliError::BadHookCmd(format!(
                    "unexpected key {key}"
                )))
            }
        };

        if parts.len() < 3 {
            return Err(AliError::BadHookCmd(format!(
                "{hook_key}: expect at least 2 arguments"
            )));
        }

        let l = parts.len();
        if l != 4 && l != 5 {
            return Err(AliError::BadHookCmd(format!(
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
            rp: ReplaceToken {
                token,
                value,
                template,
                output,
            },
        })
    }
}

/// @replace-token <TOKEN> <VALUE> <TEMPLATE> [OUTPUT]
/// TOKEN must exist in TEMPLATE file, as {{ TOKEN }},
/// e.g. TOKEN=foo, then there exists {{ foo }} in TEMPLATE file
///
/// If OUTPUT is not given, output is written to TEMPLATE file
///
/// Examples:
///
/// @replace-token PORT 2222 /etc_templates/ssh/sshd_config /etc/ssh/sshd_config
/// => Replace key PORT value with "2222", using /etc_templates/ssh/sshd_config as template and writes output to /etc/ssh/sshd_config
fn apply_replace_token(
    hook_key: &str,
    mode_hook: &ModeHook,
    r: &ReplaceToken,
    root_location: &str,
) -> Result<ActionHook, AliError> {
    // @TODO: Read from remote template, e.g. with https or ssh
    let template = std::fs::read_to_string(&r.template).map_err(|err| {
        AliError::HookError(format!(
            "{hook_key}: read template {}: {err}",
            r.template
        ))
    })?;

    let result = r.replace(&template)?;
    match mode_hook {
        ModeHook::Print => {
            println!("{}", result);
        }
        ModeHook::Normal => {
            let output_location = match root_location {
                "/" => r.output.clone(),
                _ => format!("/{root_location}/{}", r.output),
            };

            std::fs::write(output_location, result).map_err(|err| {
                AliError::HookError(format!(
                    "{hook_key}: failed to write to output to {}: {err}",
                    r.output
                ))
            })?;
        }
    }

    Ok(ActionHook::ReplaceToken(r.to_string()))
}

impl ToString for ReplaceToken {
    fn to_string(&self) -> String {
        json!({
            "token": self.token,
            "value": self.value,
            "template": self.template,
            "output": self.output,
        })
        .to_string()
    }
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
        "PORT 3322 /etc/ssh/sshd",
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
        if let Ok(HookReplaceToken {
            rp: qn,
            mode_hook: _,
        }) = result
        {
            panic!("got ok result from bad arg {cmd}: {}", qn.to_string());
        }
    }

    let tests = HashMap::from([
        (
            "@replace-token-print PORT 3322 /etc/ssh/sshd",
            ReplaceToken{
                token: String::from("PORT"),
                value: String::from("3322"),
                template: String::from("/etc/ssh/sshd"),
                output: String::from("/etc/ssh/sshd"),
            }
        ),
        (
            "@replace-token linux_boot \"loglevel=3 quiet root=/dev/archvg/archlv ro\" /etc/default/grub",
            ReplaceToken{
                token: String::from("linux_boot"),
                value: String::from("loglevel=3 quiet root=/dev/archvg/archlv ro"),
                template: String::from("/etc/default/grub"),
                output: String::from("/etc/default/grub"),
            },
        ),
        (
            "@replace-token-print \"linux boot\" \"loglevel=3 quiet root=/dev/archvg/archlv ro\" /some/template /etc/default/grub",
            ReplaceToken{
                token: String::from("linux boot"),
                value: String::from("loglevel=3 quiet root=/dev/archvg/archlv ro"),
                template: String::from("/some/template"),
                output: String::from("/etc/default/grub"),
            },
        ),
    ]);

    for (cmd, expected) in tests {
        let actual = HookReplaceToken::try_from(cmd).unwrap();
        assert_eq!(expected, actual.rp);
    }
}
