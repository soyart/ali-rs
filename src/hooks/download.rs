use super::utils::download;
use super::{
    wrap_bad_hook_cmd,
    ActionHook,
    Hook,
    ModeHook,
    ParseError,
    KEY_DOWNLOAD,
    KEY_DOWNLOAD_PRINT,
};
use crate::errors::AliError;

const USAGE: &str = "<url> <outfile>";

struct HookDownload {
    url: String,
    outfile: String,
    mode_hook: ModeHook,
}

pub(super) fn parse(k: &str, cmd: &str) -> Result<Box<dyn Hook>, ParseError> {
    match k {
        KEY_DOWNLOAD | KEY_DOWNLOAD_PRINT => {
            match HookDownload::try_from(cmd) {
                Err(err) => Err(wrap_bad_hook_cmd(err, USAGE)),
                Ok(hook) => Ok(Box::new(hook)),
            }
        }

        key => panic!("unexpected key {key}"),
    }
}

impl TryFrom<&str> for HookDownload {
    type Error = AliError;
    fn try_from(cmd: &str) -> Result<Self, Self::Error> {
        let cmd = cmd.trim();
        let parts: Vec<_> = cmd.split_whitespace().collect();

        let l = parts.len();
        if parts.len() != 3 {
            return Err(AliError::BadHookCmd(format!(
                "expecting 3 arguments, got {l}"
            )));
        }

        Ok(Self {
            mode_hook: match parts[0] {
                KEY_DOWNLOAD => ModeHook::Normal,
                KEY_DOWNLOAD_PRINT => ModeHook::Print,
                key => panic!("unexpected key {key}"),
            },

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

    fn run_hook(
        &self,
        _caller: &super::Caller,
        _root_location: &str,
    ) -> Result<super::ActionHook, AliError> {
        let downloader = download::Downloader::new_from_url(&self.url)?;
        let bytes = downloader.get_bytes()?;

        if let Err(err) = std::fs::write(&self.outfile, bytes) {
            return Err(AliError::FileError(
                err,
                format!("failed to write downloaded file to {}", self.outfile),
            ));
        }

        Ok(ActionHook::Download(format!(
            "{} -> {}",
            self.url, self.outfile
        )))
    }
}
