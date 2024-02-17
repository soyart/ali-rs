use serde_json::json;

use super::utils::download;
use super::{
    wrap_bad_hook_cmd,
    ActionHook,
    Caller,
    Hook,
    ModeHook,
    ParseError,
    KEY_UNCOMMENT,
    KEY_UNCOMMENT_ALL,
    KEY_UNCOMMENT_ALL_DEBUG,
    KEY_UNCOMMENT_DEBUG,
};
use crate::errors::AliError;

const USAGE: &str = "<PATTERN> [marker <COMMENT_MARKER=\"#\">] FILE";

#[derive(Clone)]
pub(super) enum Mode {
    All,
    Once,
}

#[derive(Clone)]
struct Uncomment {
    marker: String,
    pattern: String,
    source: String,
}

struct HookUncomment {
    mode_hook: ModeHook,
    mode: Mode,
    uc: Uncomment,
}

pub(super) fn parse(k: &str, cmd: &str) -> Result<Box<dyn Hook>, ParseError> {
    if matches!(
        k,
        KEY_UNCOMMENT
            | KEY_UNCOMMENT_DEBUG
            | KEY_UNCOMMENT_ALL
            | KEY_UNCOMMENT_ALL_DEBUG
    ) {
        match HookUncomment::try_from(cmd) {
            Err(err) => Err(wrap_bad_hook_cmd(err, USAGE)),
            Ok(hook) => Ok(Box::new(hook)),
        }
    } else {
        panic!("unknown key {k}");
    }
}

impl Hook for HookUncomment {
    fn base_key(&self) -> &'static str {
        super::KEY_UNCOMMENT
    }

    fn usage(&self) -> &'static str {
        USAGE
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
        caller: &Caller,
        root_location: &str,
    ) -> Result<ActionHook, AliError> {
        apply_uncomment(
            &self.hook_key(),
            &self.mode_hook,
            &self.mode,
            &self.uc,
            caller,
            root_location,
        )
    }
}

/// Synopsis
/// ```txt
/// @uncomment <PATTERN> [marker <COMMENT_MARKER="#">] FILE
/// ```
/// Uncomments lines starting with PATTERN in FILE. Default comment marker is "#",
/// although alternative marker can be provided after keyword `marker`, e.g. "//", "--", or "!".
///
/// Examples:
/// ```txt
/// @uncomment PubkeyAuthentication /etc/ssh/sshd_config
///
/// => Uncomments key PubkeyAuthentication in /etc/ssh/sshd_config
/// ```
impl TryFrom<&str> for HookUncomment {
    type Error = AliError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let (hook_key, parts) = super::extract_key_and_parts_shlex(s)?;

        let mode_uncomment = match hook_key.as_str() {
            KEY_UNCOMMENT | KEY_UNCOMMENT_DEBUG => Mode::Once,
            KEY_UNCOMMENT_ALL | KEY_UNCOMMENT_ALL_DEBUG => Mode::All,
            key => {
                return Err(AliError::BadHookCmd(format!(
                    "unexpected key {key}"
                )));
            }
        };
        let mode_hook = match hook_key.as_str() {
            KEY_UNCOMMENT | KEY_UNCOMMENT_ALL => ModeHook::Normal,
            KEY_UNCOMMENT_DEBUG | KEY_UNCOMMENT_ALL_DEBUG => ModeHook::Debug,
            key => panic!("unexpected key {key}"),
        };

        if parts.len() < 3 {
            return Err(AliError::BadHookCmd(format!(
                "{hook_key}: expect at least 2 arguments"
            )));
        }

        let uc = match parts.len() {
            3 => {
                Uncomment {
                    marker: "#".to_string(),
                    pattern: parts[1].to_string(),
                    source: parts[2].to_string(),
                }
            }
            5 => {
                if parts[2] != "marker" {
                    return Err(AliError::BadHookCmd(format!(
                        "{hook_key}: unexpected argument {}, expecting 2nd argument to be `marker`",
                        parts[2],
                    )));
                }

                Uncomment {
                    pattern: parts[1].clone(),
                    marker: parts[3].clone(),
                    source: parts.last().unwrap().clone(),
                }
            }
            l => {
                return Err(AliError::BadHookCmd(format!(
                    "{hook_key}: bad cmd parts: {l}"
                )));
            }
        };

        Ok(HookUncomment {
            mode_hook,
            mode: mode_uncomment,
            uc,
        })
    }
}

fn apply_uncomment(
    hook_key: &str,
    mode_hook: &ModeHook,
    mode: &Mode,
    uc: &Uncomment,
    caller: &Caller,
    root_location: &str,
) -> Result<ActionHook, AliError> {
    // Outfile, and maybe infile too if uc.source is not remote URL
    let target_file = match caller {
        Caller::ManifestPostInstall => {
            format!("{root_location}/{}", uc.source)
        }
        Caller::Cli => {
            format!("{root_location}/{}", uc.source)
        }
        _ => uc.source.clone(),
    };

    // Get original from remote location if source is remote URL
    let original = if let Ok(downloader) =
        download::Downloader::new_from_url(&uc.source)
    {
        downloader.get_string()

    // Else read from file `target`
    } else {
        std::fs::read_to_string(&target_file).map_err(|err| {
            AliError::FileError(
                err,
                format!(
                    "{hook_key}: read original file to uncomment: {target_file}"
                ),
            )
        })
    }?;

    let uncommented = match mode {
        Mode::All => {
            uncomment_text_all(hook_key, &original, &uc.marker, &uc.pattern)
        }

        Mode::Once => {
            uncomment_text_once(hook_key, &original, &uc.marker, &uc.pattern)
        }
    }?;

    match mode_hook {
        ModeHook::Debug => {
            println!("{}", uncommented);
        }

        ModeHook::Normal => {
            std::fs::write(&target_file, uncommented).map_err(|err| {
                AliError::FileError(
                    err,
                    format!("{hook_key} write uncommented to {target_file}"),
                )
            })?;
        }
    }

    Ok(ActionHook::Uncomment(uc.to_string()))
}

fn uncomment_text_all(
    _hook_key: &str,
    original: &str,
    marker: &str,
    key: &str,
) -> Result<String, AliError> {
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

fn uncomment_text_once(
    hook_key: &str,
    original: &str,
    marker: &str,
    key: &str,
) -> Result<String, AliError> {
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
        "{hook_key}: no such comment pattern '{marker} {key}'"
    )))
}

impl ToString for Uncomment {
    fn to_string(&self) -> String {
        json!({
            "comment_marker": self.marker,
            "pattern": self.pattern,
            "file": self.source
        })
        .to_string()
    }
}

#[test]
fn test_parse_uncomment() {
    let should_pass = vec![
        "@uncomment Port /etc/ssh/sshd_config",
        "@uncomment SomeKey /some_file",
        "@uncomment someKey marker '#' ./someFile",
        "@uncomment UseFoo marker '!!' ./someFile",
    ];

    let should_err = vec![
        "@uncomment foo bar baz",
        "@uncomment SomeKey",
        "@uncomment marker '#' someKey someFile",
        "@uncomment",
    ];

    for s in should_pass {
        let result = HookUncomment::try_from(s);
        if let Err(ref err) = result {
            eprintln!("unexpected error result for {s}: {err}");
        }

        assert!(result.is_ok());
    }

    for s in should_err {
        let result = HookUncomment::try_from(s);
        if result.is_ok() {
            eprintln!("unexpected ok result for {s}");
        }

        assert!(result.is_err());
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

    let hook_key = "@uncomment-all";
    for original in originals {
        let uncommented_port =
            uncomment_text_all(hook_key, original, "#", "Port")
                .expect("failed to uncomment Port");

        if original == uncommented_port {
            panic!("'# Port' not uncommented");
        }

        let uncommented_all = uncomment_text_all(
            hook_key,
            &uncommented_port,
            "#",
            "PubkeyAuthentication",
        )
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

    let hook_key = "@uncomment";
    for original in originals {
        let uncommented_port =
            uncomment_text_once(hook_key, original, "#", "Port")
                .expect("failed to uncomment Port");

        let uncommented_all = uncomment_text_once(
            hook_key,
            &uncommented_port,
            "#",
            "PubkeyAuthentication",
        )
        .expect("failed to uncomment PubkeyAuthentication");

        assert_ne!(expected, uncommented_port);
        assert_ne!(original, uncommented_all);
        assert_eq!(expected, uncommented_all);
    }
}
