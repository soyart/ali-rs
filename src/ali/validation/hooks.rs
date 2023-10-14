use crate::ali::Manifest;
use crate::errors::AliError;
use crate::hooks;

pub fn validate(manifest: &Manifest) -> Result<(), AliError> {
    let mountpoint = match &manifest.rootfs.0.mnt {
        Some(mountpoint) => mountpoint.as_str(),
        None => "/",
    };

    if let Some(cmds) = &manifest.chroot {
        if manifest.rootfs.0.mnt.is_none() {
            return Err(AliError::BadManifest(
                "got none rootfs mountpoint in manifest, but got chroot hooks"
                    .to_string(),
            ));
        }

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
