use crate::errors::AliError;
use crate::manifest;
use crate::utils::shell;

pub fn mount_fs(fs: manifest::ManifestFs) -> Result<(), AliError> {
    let cmd_mount = match fs.fs_opts {
        Some(opts) => format!("mount -o {opts} {} {}", fs.device, fs.mnt),
        None => format!("mount {} {}", fs.device, fs.mnt),
    };

    shell::exec("sh", &["-c", &cmd_mount])
}
