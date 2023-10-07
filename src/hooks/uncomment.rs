use serde_json::json;

use super::ActionHook;
use crate::errors::AliError;

pub(super) enum Mode {
    All,
    Once,
}

struct Uncomment {
    marker: String,
    pattern: String,
    file: String,
    mode: Mode,
    print_only: bool,
}

pub(super) fn uncomment(cmd: &str) -> Result<ActionHook, AliError> {
    let uc = parse_uncomment(cmd)?;

    // @TODO: Read from remote template
    let original = std::fs::read_to_string(&uc.file).map_err(|err| {
        AliError::FileError(
            err,
            format!("@uncomment: read original file to uncomment: {}", uc.file),
        )
    })?;

    let uncommented = match uc.mode {
        Mode::All => uncomment_text_all(&original, &uc.marker, &uc.pattern),
        Mode::Once => uncomment_text_once(&original, &uc.marker, &uc.pattern),
    }?;

    match uc.print_only {
        true => {
            println!("{}", uncommented);
        }
        false => {
            std::fs::write(&uc.file, uncommented).map_err(|err| {
                AliError::FileError(err, format!("@uncomment: write uncommented to {}", uc.file))
            })?;
        }
    }

    Ok(ActionHook::Uncomment(uc.to_string()))
}

fn uncomment_text_all(original: &str, marker: &str, key: &str) -> Result<String, AliError> {
    let mut c = 0;
    let uncommented = loop {
        let whitespace = " ".repeat(c);
        let pattern = format!("{}{whitespace}{}", marker, key);

        let uncommented = original.replace(&pattern, key);

        if original != uncommented {
            break uncommented;
        }

        c += 1
    };

    Ok(uncommented)
}

fn uncomment_text_once(original: &str, marker: &str, key: &str) -> Result<String, AliError> {
    let lines: Vec<&str> = original.lines().collect();
    for line in lines {
        for i in 0..5 {
            let whitespace = " ".repeat(i);
            let pattern = format!("{marker}{whitespace}{key}");

            if line.contains(&pattern) {
                let line_uncommented = line.replacen(&pattern, key, 1);
                return Ok(original.replacen(line, &line_uncommented, 1));
            }
        }
    }

    Err(AliError::HookError(format!(
        "@uncomment: no such comment pattern '{marker} {key}'"
    )))
}

/// @uncomment <PATTERN> [marker <COMMENT_MARKER="#">] FILE
/// Uncomments lines starting with PATTERN in FILE. Default comment marker is "#",
/// although alternative marker can be provided after keyword `marker`, e.g. "//", "--", or "!".
///
/// Examples:
/// @uncomment PubkeyAuthentication /etc/ssh/sshd_config
/// => Uncomments key PubkeyAuthentication in /etc/ssh/sshd_config
fn parse_uncomment(hook_cmd: &str) -> Result<Uncomment, AliError> {
    let parts = shlex::split(hook_cmd);
    if parts.is_none() {
        return Err(AliError::BadHookCmd(format!(
            "@uncomment: bad cmd {hook_cmd}"
        )));
    }

    let parts = parts.unwrap();
    if parts.len() < 3 {
        return Err(AliError::BadHookCmd(
            "@uncomment: expect at least 3 arguments".to_string(),
        ));
    }

    let cmd = parts.first().unwrap();

    if !matches!(
        cmd.as_str(),
        "@uncomment" | "@uncomment-print" | "@uncomment-all" | "@uncomment-all-print"
    ) {
        return Err(AliError::BadHookCmd(format!("@uncomment: bad cmd: {cmd}")));
    }

    let print_only = matches!(cmd.as_str(), "@uncomment-print" | "@uncomment-all-print");

    let mode = match cmd.as_str() {
        "@uncomment" | "@uncomment-print" => Mode::Once,
        "@uncomment-all" | "@uncomment-all-print" => Mode::All,
        _ => return Err(AliError::AliRsBug(format!("got bad hook cmd: {cmd}"))),
    };

    let l = parts.len();
    match l {
        3 => Ok(Uncomment {
            marker: "#".to_string(),
            pattern: parts[1].to_string(),
            file: parts[2].to_string(),
            mode,
            print_only,
        }),
        5 => {
            if parts[2] != "marker" {
                return Err(AliError::BadHookCmd(format!(
                    "@uncomment: unexpected argument {}, expecting 2nd argument to be `marker`",
                    parts[2],
                )));
            }

            Ok(Uncomment {
                pattern: parts[1].clone(),
                marker: parts[3].clone(),
                file: parts.last().unwrap().clone(),
                mode,
                print_only,
            })
        }
        _ => Err(AliError::BadHookCmd(format!(
            "@uncomment: bad cmd parts: {l}"
        ))),
    }
}

impl ToString for Uncomment {
    fn to_string(&self) -> String {
        json!({
            "comment_marker": self.marker,
            "pattern": self.pattern,
            "file": self.file
        })
        .to_string()
    }
}

#[test]
fn test_uncomment_text_all() {
    let originals = [
        r#"#Port 22
#PubkeyAuthentication no"#,
        r#"# Port 22
#  PubkeyAuthentication no"#,
    ];

    let expected = r#"Port 22
PubkeyAuthentication no"#;

    for original in originals {
        let uncommented_port =
            uncomment_text_all(original, "#", "Port").expect("failed to uncomment Port");

        if original == uncommented_port {
            panic!("'# Port' not uncommented");
        }

        let uncommented_all = uncomment_text_all(&uncommented_port, "#", "PubkeyAuthentication")
            .expect("failed to uncomment PubkeyAuthentication");

        if original == uncommented_all {
            panic!("'# PubkeyAuthentication not uncommented'");
        }

        assert_eq!(expected, uncommented_all);
    }
}

#[test]
fn test_uncomment_text_once() {
    let originals = [
        r#"#Port 22
#PubkeyAuthentication no"#,
        r#"# Port 22
#  PubkeyAuthentication no"#,
    ];

    let expected = r#"Port 22
PubkeyAuthentication no"#;

    for original in originals {
        let uncommented_port =
            uncomment_text_once(original, "#", "Port").expect("failed to uncomment Port");

        let uncommented_all = uncomment_text_once(&uncommented_port, "#", "PubkeyAuthentication")
            .expect("failed to uncomment PubkeyAuthentication");

        assert_ne!(expected, uncommented_port);
        assert_ne!(original, uncommented_all);
        assert_eq!(expected, uncommented_all);
    }
}
