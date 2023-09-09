// @TODO: Use shell expression to extract strings

use serde_json::json;

use crate::errors::AliError;

use super::ActionHook;

struct ReplaceToken<'a> {
    token: &'a str,
    value: &'a str,
    template: &'a str,
    output: &'a str,
}

/// #replace-token <TOKEN> <VALUE> <TEMPLATE> [OUTPUT]
/// TOKEN must exist in TEMPLATE file, as {{ TOKEN }},
/// e.g. TOKEN=foo, then there exists {{ foo }} in TEMPLATE file
///
/// If OUTPUT is not given, output is written to TEMPLATE file
///
/// Examples:
///
/// #replace-token PORT 2222 /etc_templates/ssh/sshd_config /etc/ssh/sshd_config
/// => Replace key PORT value with "2222", using /etc_templates/ssh/sshd_config as template and writes output to /etc/ssh/sshd_config
pub(super) fn replace_token(cmd: &str) -> Result<ActionHook, AliError> {
    let r = parse_replace_token(cmd)?;

    let template = std::fs::read_to_string(r.template).map_err(|err| {
        AliError::FileError(err, format!("#replace-token: read template {}", r.template))
    })?;

    let result = r.replace(&template)?;

    std::fs::write(r.output, result).map_err(|err| {
        AliError::FileError(
            err,
            format!("#replace-token: failed to write to output to {}", r.output),
        )
    })?;

    Ok(ActionHook::ReplaceToken(r.to_string()))
}

fn parse_replace_token(cmd: &str) -> Result<ReplaceToken, AliError> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let l = parts.len();

    if l <= 3 {
        return Err(AliError::BadArgs(format!("#replace-token: only {l} parts")));
    }

    if parts[0] != "#replace-token" {
        return Err(AliError::AliRsBug(
            "#replace-token: hook command does not starts with `#replace-token`".to_string(),
        ));
    }

    let (token, value, template) = (parts[1], parts[2], parts[3]);
    let output = parts.get(4).unwrap_or(&template).to_owned();

    Ok(ReplaceToken {
        token,
        value,
        template,
        output,
    })
}

impl<'a> ToString for ReplaceToken<'a> {
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

impl<'a> ReplaceToken<'a> {
    fn replace(&self, s: &str) -> Result<String, AliError> {
        let token = &format!("{{ {} }}", self.token);
        if !s.contains(token) {
            return Err(AliError::BadArgs(format!(
                "template {} does not contains token \"{token}\"",
                self.template
            )));
        }

        Ok(s.replace(token, self.value))
    }
}

#[test]
fn test_parse_replace_token() {
    let should_pass = vec![
        "#replace-token PORT 3322 /etc/ssh/sshd",
        "#replace-token foo bar https://example.com/template /some/file",
    ];

    let should_err = vec![
        "PORT 3322 /etc/ssh/sshd",
        "#replace-token PORT",
        "#replace-token PORT 3322",
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
}
