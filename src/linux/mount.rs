use crate::ali::ManifestFs;
use crate::errors::AliError;
use crate::utils::shell;

/// Executes:
/// ```shell
/// mount {fs.mnt_opts} {fs.device} {fs.mnt}
/// ```
/// Returns error if fs.mnt is None
pub fn mount_fs(fs: &ManifestFs) -> Result<(), AliError> {
    if fs.mnt.is_none() {
        return Err(AliError::AliRsBug(
            "this manifest filesystem does not specify mountpoint".to_string(),
        ));
    }

    let mount_point = fs.mnt.clone().unwrap();
    let cmd_mount = match fs.fs_opts {
        Some(ref opts) => {
            format!("mount -o {opts} {} {mount_point}", fs.device)
        }
        None => format!("mount {} {mount_point}", fs.device),
    };

    shell::sh_c(&cmd_mount)
}
