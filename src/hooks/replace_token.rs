use serde_json::json;

use crate::errors::AliError;

use super::{
    ActionHook,
    Caller,
    HookWrapper,
    ModeHook,
    KEY_REPLACE_TOKEN,
    KEY_REPLACE_TOKEN_PRINT,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReplaceToken {
    token: String,
    value: String,
    template: String,
    output: String,
    print_only: bool,
}

struct MetaReplaceToken {
    rp: Option<ReplaceToken>,
    mode_hook: ModeHook,
}

pub(super) fn new(key: &str) -> Box<dyn HookWrapper> {
    Box::new(MetaReplaceToken {
        rp: None,
        mode_hook: match key {
            KEY_REPLACE_TOKEN => ModeHook::Normal,
            KEY_REPLACE_TOKEN_PRINT => ModeHook::Print,
            _ => panic!("unexpected key {key}"),
        },
    })
}

impl HookWrapper for MetaReplaceToken {
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

    fn preferred_callers(&self) -> std::collections::HashSet<super::Caller> {
        super::all_callers()
    }

    fn abort_if_no_mount(&self) -> bool {
        false
    }

    fn try_parse(&mut self, s: &str) -> Result<(), AliError> {
        let rp = parse_replace_token(s)?;
        self.rp = Some(rp);

        Ok(())
    }

    fn run_hook(
        &self,
        caller: &Caller,
        root_location: &str,
    ) -> Result<ActionHook, AliError> {
        self.rp.as_ref().unwrap().run(caller, root_location)
    }
}

impl ReplaceToken {
    fn run(
        &self,
        _caller: &Caller,
        root_location: &str,
    ) -> Result<ActionHook, AliError> {
        replace_token(self, root_location)
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
fn replace_token(
    r: &ReplaceToken,
    root_location: &str,
) -> Result<ActionHook, AliError> {
    // @TODO: Read from remote template, e.g. with https or ssh
    let template = std::fs::read_to_string(&r.template).map_err(|err| {
        AliError::HookError(format!(
            "{KEY_REPLACE_TOKEN}: read template {}: {err}",
            r.template
        ))
    })?;

    let result = r.replace(&template)?;

    if r.print_only {
        println!("{}", result);
    } else {
        let output_location = match root_location {
            "/" => r.output.clone(),
            _ => format!("/{root_location}/{}", r.output),
        };

        std::fs::write(output_location, result).map_err(|err| {
            AliError::HookError(format!(
                "{KEY_REPLACE_TOKEN}: failed to write to output to {}: {err}",
                r.output
            ))
        })?;
    }

    Ok(ActionHook::ReplaceToken(r.to_string()))
}

fn parse_replace_token(cmd: &str) -> Result<ReplaceToken, AliError> {
    // shlex will return empty array if 1st word starts with '#'
    let parts = shlex::split(cmd);
    if parts.is_none() {
        return Err(AliError::BadHookCmd(format!(
            "{KEY_REPLACE_TOKEN}: bad cmd: {cmd}"
        )));
    }

    let parts = parts.unwrap();
    if parts.len() < 3 {
        return Err(AliError::BadHookCmd(format!(
            "{KEY_REPLACE_TOKEN}: expect at least 2 arguments"
        )));
    }

    let cmd = parts.first().unwrap();

    if !matches!(cmd.as_str(), KEY_REPLACE_TOKEN | KEY_REPLACE_TOKEN_PRINT) {
        return Err(AliError::BadHookCmd(format!(
            "{KEY_REPLACE_TOKEN}: bad cmd: {cmd}"
        )));
    }

    let l = parts.len();

    if l != 4 && l != 5 {
        return Err(AliError::BadHookCmd(format!(
            "{KEY_REPLACE_TOKEN}: bad cmd parts (expecting 3-4): {l}"
        )));
    }

    let (token, value, template) =
        (parts[1].clone(), parts[2].clone(), parts[3].clone());

    // If not given, then use template as output
    let output = parts
        .last()
        .map(|s| s.to_owned())
        .unwrap_or(template.clone());

    let print_only = parts[0].as_str() == KEY_REPLACE_TOKEN_PRINT;

    Ok(ReplaceToken {
        token,
        value,
        template,
        output,
        print_only,
    })
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

impl ReplaceToken {
    fn replace(&self, s: &str) -> Result<String, AliError> {
        let token = &format!("{} {} {}", "{{", self.token, "}}");

        if !s.contains(token) {
            return Err(AliError::BadHookCmd(format!(
                "template {} does not contains token \"{token}\"",
                self.template
            )));
        }

        Ok(s.replace(token, &self.value))
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
        let result = parse_replace_token(cmd);
        if let Err(err) = result {
            panic!("got error from cmd {cmd}: {err}");
        }
    }

    for cmd in should_err {
        let result = parse_replace_token(cmd);
        if let Ok(qn) = result {
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
                print_only: true,
            }
        ),
        (
            "@replace-token linux_boot \"loglevel=3 quiet root=/dev/archvg/archlv ro\" /etc/default/grub",
            ReplaceToken{
                token: String::from("linux_boot"),
                value: String::from("loglevel=3 quiet root=/dev/archvg/archlv ro"),
                template: String::from("/etc/default/grub"),
                output: String::from("/etc/default/grub"),
                print_only: false,
            },
        ),
        (
            "@replace-token-print \"linux boot\" \"loglevel=3 quiet root=/dev/archvg/archlv ro\" /some/template /etc/default/grub",
            ReplaceToken{
                token: String::from("linux boot"),
                value: String::from("loglevel=3 quiet root=/dev/archvg/archlv ro"),
                template: String::from("/some/template"),
                output: String::from("/etc/default/grub"),
                print_only: true,
            },
        ),
    ]);

    for (cmd, expected) in tests {
        let actual = parse_replace_token(cmd).unwrap();

        assert_eq!(expected, actual);
    }
}

#[test]
fn test_uncomment() {
    use std::collections::HashMap;

    let print_only = true;
    let tests = HashMap::from([
        (
            ReplaceToken {
                token: String::from("PORT"),
                value: String::from("3322"),
                template: String::from("/etc/ssh/sshd"),
                output: String::from("/etc/ssh/sshd"),
                print_only,
            },
            ("{{ PORT }} foo bar {{PORT}}", "3322 foo bar {{PORT}}"),
        ),
        (
            ReplaceToken {
                token: String::from("foo"),
                value: String::from("bar"),
                template: String::from("/etc/ssh/sshd"),
                output: String::from("/etc/ssh/sshd"),
                print_only,
            },
            (
                "{{ bar }} {{ foo }} {{ bar }} foo <{{ foo }}>",
                "{{ bar }} bar {{ bar }} foo <bar>",
            ),
        ),
    ]);

    for (replace, (template, expected)) in tests {
        let actual = replace
            .replace(template)
            .expect("failed to replace template {template}");

        assert_eq!(expected, actual);
    }
}
