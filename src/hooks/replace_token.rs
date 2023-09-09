use serde_json::json;

use crate::errors::AliError;

use super::ActionHook;

#[derive(Debug, PartialEq)]
struct ReplaceToken {
    token: String,
    value: String,
    template: String,
    output: String,
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
pub(super) fn replace_token(cmd: &str) -> Result<ActionHook, AliError> {
    let r = parse_replace_token(cmd)?;

    let template = std::fs::read_to_string(&r.template).map_err(|err| {
        AliError::FileError(err, format!("@replace-token: read template {}", r.template))
    })?;

    let result = r.replace(&template)?;

    std::fs::write(&r.output, result).map_err(|err| {
        AliError::FileError(
            err,
            format!("@replace-token: failed to write to output to {}", r.output),
        )
    })?;

    Ok(ActionHook::ReplaceToken(r.to_string()))
}

fn parse_replace_token(cmd: &str) -> Result<ReplaceToken, AliError> {
    // shlex will return empty array if 1st word starts with '#'
    let parts = shlex::split(cmd);
    if parts.is_none() {
        return Err(AliError::BadArgs(format!(
            "@replace-token: bad args: {cmd}"
        )));
    }

    let parts = parts.unwrap();
    if parts[0] != "@replace-token" {
        return Err(AliError::BadArgs(format!(
            "@replace-token: cmd does not start with `@replace-token`: {cmd}"
        )));
    }

    let l = parts.len();

    if l != 4 && l != 5 {
        return Err(AliError::BadArgs(format!(
            "@replace-token: bad cmd parts (expecting 3-4): {l}"
        )));
    }

    let (token, value, template) = (parts[1].clone(), parts[2].clone(), parts[3].clone());
    let output = parts
        .last()
        .map(|s| s.to_owned())
        .unwrap_or(template.clone());

    Ok(ReplaceToken {
        token,
        value,
        template,
        output: output.clone(),
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
        let token = &format!("{{ {} }}", self.token);
        if !s.contains(token) {
            return Err(AliError::BadArgs(format!(
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
            "@replace-token PORT 3322 /etc/ssh/sshd",
            ReplaceToken{
                token: String::from("PORT"),
                value: String::from("3322"),
                template: String::from("/etc/ssh/sshd"),
                output: String::from("/etc/ssh/sshd")
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
            "@replace-token \"linux boot\" \"loglevel=3 quiet root=/dev/archvg/archlv ro\" /some/template /etc/default/grub",
            ReplaceToken{
                token: String::from("linux boot"),
                value: String::from("loglevel=3 quiet root=/dev/archvg/archlv ro"),
                template: String::from("/some/template"),
                output: String::from("/etc/default/grub"),
            },
        ),
    ]);

    for (cmd, expected) in tests {
        let actual = parse_replace_token(cmd).unwrap();

        assert_eq!(expected, actual);
    }
}
