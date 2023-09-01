use crate::ali::ManifestFs;
use crate::errors::AliError;
use crate::utils::shell;

/// Executes:
/// ```shell
/// mkfs.{fs.fs_type} {fs.fs_opts} {fs.device}
/// ```
pub fn create_fs(fs: &ManifestFs) -> Result<(), AliError> {
    let cmd_mkfs = match &fs.fs_opts {
        Some(opts) => format!("'mkfs.{} {opts} {}'", fs.fs_type, fs.device),
        None => format!("'mkfs.{} {}'", fs.fs_type, fs.device),
    };

    shell::sh_c(&cmd_mkfs)
}
