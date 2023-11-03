use crate::ali::ManifestMountpoint;
use crate::errors::AliError;
use crate::utils::shell;

/// Executes:
/// ```shell
/// mount <mnt.device> [mnt.mnt_opts] /base/<mnt.dest>
/// ```
pub fn mount(mnt: &ManifestMountpoint, base: &str) -> Result<(), AliError> {
    let mountpoint = prepend_base(base, &mnt.dest);
    let cmd_mount = match mnt.mnt_opts {
        Some(ref opts) => {
            format!("mount -o {opts} {} {mountpoint}", mnt.device)
        }
        None => format!("mount {} {mountpoint}", mnt.device),
    };

    shell::sh_c(&cmd_mount)
}

pub fn prepend_base(base: &str, mountpoint: &str) -> String {
    // e.g. base /data on manifest /foo => /data/foo
    format!("{base}{mountpoint}")
}
