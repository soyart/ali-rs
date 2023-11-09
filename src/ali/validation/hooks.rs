use crate::ali::Manifest;
use crate::errors::AliError;
use crate::hooks;

pub fn validate(manifest: &Manifest, mountpoint: &str) -> Result<(), AliError> {
    if let Some(cmds) = &manifest.chroot {
        validate_hooks(cmds, &hooks::Caller::ManifestChroot, mountpoint)?;
    }

    if let Some(cmds) = &manifest.postinstall {
        validate_hooks(cmds, &hooks::Caller::ManifestPostInstall, mountpoint)?;
    }

    Ok(())
}

fn validate_hooks(
    cmds: &Vec<String>,
    caller: &hooks::Caller,
    mountpoint: &str,
) -> Result<(), AliError> {
    for cmd in cmds {
        if !hooks::is_hook(cmd) {
            continue;
        }

        hooks::validate_hook(cmd, caller, mountpoint)?;
    }

    Ok(())
}
