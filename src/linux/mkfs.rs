use crate::errors::AliError;
use crate::manifest;
use crate::utils::shell::exec;

pub fn create_fs(fs: manifest::ManifestFs) -> Result<(), AliError> {
    let mkfs_cmd = format!("mkfs.{}", fs.fs_type);
    match fs.fs_opts {
        None => exec(&mkfs_cmd, &[fs.device.as_str()]),
        // @TODO: find ways to spread opts
        Some(opts) => exec(&mkfs_cmd, &[&opts, fs.device.as_str()]),
    }
}
