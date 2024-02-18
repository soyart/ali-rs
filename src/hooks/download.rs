use std::io::{
    stdout,
    Write,
};

use super::utils::download;
use super::{
    extract_key_and_parts,
    wrap_hook_parse_help,
    ActionHook,
    Hook,
    ModeHook,
    ParseError,
    KEY_DOWNLOAD,
    KEY_DOWNLOAD_DEBUG,
};
use crate::errors::AliError;

const USAGE: &str = "<URL> <OUTFILE>";

struct HookDownload {
    url: String,
    outfile: String,
    mode_hook: ModeHook,
}

pub(super) fn parse(k: &str, cmd: &str) -> Result<Box<dyn Hook>, ParseError> {
    match k {
        KEY_DOWNLOAD | KEY_DOWNLOAD_DEBUG => {
            match HookDownload::try_from(cmd) {
                Ok(hook) => Ok(Box::new(hook)),
                Err(err) => Err(wrap_hook_parse_help(err, USAGE)),
            }
        }

        key => panic!("unexpected key {key}"),
    }
}

impl TryFrom<&str> for HookDownload {
    type Error = AliError;

    fn try_from(cmd: &str) -> Result<Self, Self::Error> {
        let (hook_key, parts) = extract_key_and_parts(cmd)?;

        let mode = match hook_key.as_str() {
            KEY_DOWNLOAD => ModeHook::Normal,
            KEY_DOWNLOAD_DEBUG => ModeHook::Debug,
            key => {
                panic!("unexpected key {key}");
            }
        };

        let l = parts.len();
        if l != 3 {
            return Err(AliError::HookParse(format!(
                "{hook_key}: expecting 3 arguments, got {l}"
            )));
        }

        Ok(Self {
            mode_hook: mode,
            url: parts[1].to_string(),
            outfile: parts[2].to_string(),
        })
    }
}

impl Hook for HookDownload {
    fn base_key(&self) -> &'static str {
        KEY_DOWNLOAD
    }

    fn usage(&self) -> &'static str {
        USAGE
    }

    fn mode(&self) -> super::ModeHook {
        self.mode_hook.clone()
    }

    fn should_chroot(&self) -> bool {
        false
    }

    fn prefer_caller(&self, _caller: &super::Caller) -> bool {
        true
    }

    fn abort_if_no_mount(&self) -> bool {
        false
    }

    // @TODO: Use param caller and root_location
    fn run_hook(
        &self,
        _caller: &super::Caller,
        _root_location: &str,
    ) -> Result<super::ActionHook, AliError> {
        let downloader = download::Downloader::new_from_url(&self.url)?;
        let bytes = downloader.get_bytes()?;

        match self.mode_hook {
            ModeHook::Normal => {
                let result = std::fs::write(&self.outfile, bytes);
                if let Err(err) = result {
                    let err_msg = format!(
                        "failed to write downloaded file to {}",
                        self.outfile
                    );

                    return Err(AliError::FileError(err, err_msg));
                }
            }

            ModeHook::Debug => {
                let mut out = stdout();
                if let Err(err) = out.write_all(&bytes) {
                    return Err(AliError::HookApply(format!(
                        "failed to write downloaded bytes to stdout: {err}"
                    )));
                }
            }
        }

        Ok(ActionHook::Download(format!(
            "{} -> {}",
            self.url, self.outfile
        )))
    }
}
