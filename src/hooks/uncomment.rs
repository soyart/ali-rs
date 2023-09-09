use serde_json::json;

use super::ActionHook;
use crate::errors::AliError;

struct Uncomment {
    marker: String,
    pattern: String,
    file: String,
}

pub(super) fn uncomment(cmd: &str) -> Result<ActionHook, AliError> {
    let uc = parse_uncomment(cmd)?;
    let original = std::fs::read_to_string(&uc.file).map_err(|err| {
        AliError::FileError(
            err,
            format!("@uncomment: read original file to uncomment: {}", uc.file),
        )
    })?;

    let commented = format!("{}{}", uc.marker, uc.pattern);
    let uncommented = original.replace(&commented, &uc.pattern);

    std::fs::write(&uc.file, uncommented).map_err(|err| {
        AliError::FileError(err, format!("@uncomment: write uncommented to {}", uc.file))
    })?;

    Ok(ActionHook::Uncomment(uc.to_string()))
}

/// @uncomment <PATTERN> [marker <COMMENT_MARKER="#">] FILE
/// Uncomments lines starting with PATTERN in FILE. Default comment marker is "#",
/// although alternative marker can be provided after keyword `marker`, e.g. "//", "--", or "!".
///
/// Examples:
/// @uncomment PubkeyAuthentication /etc/ssh/sshd_config
/// => Uncomments key PubkeyAuthentication in /etc/ssh/sshd_config
fn parse_uncomment(cmd: &str) -> Result<Uncomment, AliError> {
    let parts = shlex::split(cmd);
    if parts.is_none() {
        return Err(AliError::BadArgs(format!("@uncomment: bad cmd {cmd}")));
    }

    let parts = parts.unwrap();
    if parts[0] != "@uncomment" {
        return Err(AliError::BadArgs(format!(
            "@uncomment: cmd does not start with `@uncomment`: {cmd}"
        )));
    }

    let l = parts.len();
    match l {
        3 => Ok(Uncomment {
            marker: "#".to_string(),
            pattern: parts[1].to_string(),
            file: parts[2].to_string(),
        }),
        5 => {
            if parts[2] != "marker" {
                return Err(AliError::BadArgs(format!(
                    "@uncomment: unexpected argument {}, expecting 2nd argument to be `marker`",
                    parts[2],
                )));
            }

            Ok(Uncomment {
                pattern: parts[1].clone(),
                marker: parts[3].clone(),
                file: parts.last().unwrap().clone(),
            })
        }
        _ => Err(AliError::BadArgs(format!("@uncomment: bad arg parts: {l}"))),
    }
}

impl ToString for Uncomment {
    fn to_string(&self) -> String {
        let j = json!({
            "comment_marker": self.marker,
            "pattern": self.pattern,
            "file": self.file
        });

        j.to_string()
    }
}
