use serde_json::json;

use super::{
    ActionHook,
    Caller,
    Hook,
    ModeHook,
    KEY_UNCOMMENT,
    KEY_UNCOMMENT_ALL,
    KEY_UNCOMMENT_ALL_PRINT,
    KEY_UNCOMMENT_PRINT,
};
use crate::errors::AliError;

#[derive(Clone)]
pub(super) enum Mode {
    All,
    Once,
}

#[derive(Clone)]
struct Uncomment {
    marker: String,
    pattern: String,
    file: String,
}

struct HookUncomment {
    mode_hook: ModeHook,
    mode: Mode,
    uc: Option<Uncomment>,
}

pub(super) fn init_from_key(key: &str) -> Box<dyn Hook> {
    Box::new(HookUncomment {
        uc: None,
        mode: match key {
            KEY_UNCOMMENT | KEY_UNCOMMENT_PRINT => Mode::Once,
            KEY_UNCOMMENT_ALL | KEY_UNCOMMENT_ALL_PRINT => Mode::All,
            key => panic!("unexpected key {key}"),
        },
        mode_hook: match key {
            KEY_UNCOMMENT | KEY_UNCOMMENT_ALL => ModeHook::Normal,
            KEY_UNCOMMENT_PRINT | KEY_UNCOMMENT_ALL_PRINT => ModeHook::Print,
            key => panic!("unexpected key {key}"),
        },
    })
}

impl Hook for HookUncomment {
    fn base_key(&self) -> &'static str {
        super::KEY_UNCOMMENT
    }

    fn usage(&self) -> &'static str {
        "<PATTERN> [marker <COMMENT_MARKER=\"#\">] FILE"
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

    fn parse_cmd(&mut self, s: &str) -> Result<(), AliError> {
        let uc = parse_uncomment(s)?;
        self.uc = Some(uc);

        Ok(())
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
            self.uc.as_ref().unwrap(),
            caller,
            root_location,
        )
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
    let target = match caller {
        Caller::ManifestPostInstall => {
            format!("{root_location}/{}", uc.file)
        }
        Caller::Cli => {
            format!("{root_location}/{}", uc.file)
        }
        _ => uc.file.clone(),
    };

    // @TODO: Read from remote template
    let original = std::fs::read_to_string(&target).map_err(|err| {
        AliError::FileError(
            err,
            format!("{hook_key}: read original file to uncomment: {target}"),
        )
    })?;

    let uncommented = match mode {
        Mode::All => uncomment_text_all(&original, &uc.marker, &uc.pattern),
        Mode::Once => uncomment_text_once(&original, &uc.marker, &uc.pattern),
    }?;

    match mode_hook {
        ModeHook::Print => {
            println!("{}", uncommented);
        }
        ModeHook::Normal => {
            std::fs::write(&target, uncommented).map_err(|err| {
                AliError::FileError(
                    err,
                    format!("{hook_key} write uncommented to {target}"),
                )
            })?;
        }
    }

    Ok(ActionHook::Uncomment(uc.to_string()))
}

fn uncomment_text_all(
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
        "{KEY_UNCOMMENT}: no such comment pattern '{marker} {key}'"
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
            "{KEY_UNCOMMENT}: bad cmd {hook_cmd}"
        )));
    }

    let parts = parts.unwrap();
    if parts.len() < 3 {
        return Err(AliError::BadHookCmd(format!(
            "{KEY_UNCOMMENT}: expect at least 2 arguments"
        )));
    }

    let l = parts.len();
    match l {
        3 => {
            Ok(Uncomment {
                marker: "#".to_string(),
                pattern: parts[1].to_string(),
                file: parts[2].to_string(),
            })
        }
        5 => {
            if parts[2] != "marker" {
                return Err(AliError::BadHookCmd(format!(
                    "{KEY_UNCOMMENT}: unexpected argument {}, expecting 2nd argument to be `marker`",
                    parts[2],
                )));
            }

            Ok(Uncomment {
                pattern: parts[1].clone(),
                marker: parts[3].clone(),
                file: parts.last().unwrap().clone(),
            })
        }
        _ => {
            Err(AliError::BadHookCmd(format!(
                "{KEY_UNCOMMENT}: bad cmd parts: {l}"
            )))
        }
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
        let uncommented_port = uncomment_text_all(original, "#", "Port")
            .expect("failed to uncomment Port");

        if original == uncommented_port {
            panic!("'# Port' not uncommented");
        }

        let uncommented_all =
            uncomment_text_all(&uncommented_port, "#", "PubkeyAuthentication")
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
        let uncommented_port = uncomment_text_once(original, "#", "Port")
            .expect("failed to uncomment Port");

        let uncommented_all =
            uncomment_text_once(&uncommented_port, "#", "PubkeyAuthentication")
                .expect("failed to uncomment PubkeyAuthentication");

        assert_ne!(expected, uncommented_port);
        assert_ne!(original, uncommented_all);
        assert_eq!(expected, uncommented_all);
    }
}
