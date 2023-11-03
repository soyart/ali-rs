use crate::ali::Manifest;
use crate::errors::AliError;
use crate::hooks;

pub fn validate(manifest: &Manifest, mountpoint: &str) -> Result<(), AliError> {
    if let Some(cmds) = &manifest.chroot {
        for cmd in cmds {
            if hooks::is_hook(cmd) {
                hooks::validate_hook(
                    cmd,
                    &hooks::Caller::ManifestChroot,
                    mountpoint,
                )?;
            }
        }
    }

    if let Some(cmds) = &manifest.postinstall {
        for cmd in cmds {
            if hooks::is_hook(cmd) {
                hooks::validate_hook(
                    cmd,
                    &hooks::Caller::ManifestPostInstall,
                    mountpoint,
                )?;
            }
        }
    }

    Ok(())
}
