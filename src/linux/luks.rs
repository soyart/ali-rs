use crate::errors::AliError;
use crate::utils::shell;

// libcryptsetup bindings: https://github.com/stratis-storage/libcryptsetup-rs/

pub fn format(device: &str, key: Option<&str>) -> Result<(), AliError> {
    let mut format_cmd = format!("cryptsetup luksFormat {device}");

    if let Some(passphrase) = key {
        check_passphrase(passphrase)?;

        format_cmd = format!("echo '{passphrase}' | {format_cmd}");
    }

    shell::sh_c(&format_cmd)
}

pub fn open(
    device: &str,
    key: Option<&str>,
    name: &str,
) -> Result<(), AliError> {
    let mut open_cmd = format!("cryptsetup luksOpen {device} {name}");

    if let Some(passphrase) = key {
        check_passphrase(passphrase)?;

        open_cmd = format!("echo '{passphrase}' | {open_cmd}")
    }

    shell::sh_c(&open_cmd)
}

pub fn close(name: &str) -> Result<(), AliError> {
    let close_cmd = format!("cryptsetup luksClose {name}");

    shell::sh_c(&close_cmd)
}

fn check_passphrase(pass: &str) -> Result<(), AliError> {
    match pass {
        "" => Err(AliError::BadManifest("empty luks passphrase".to_string())),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        close,
        format,
        open,
    };
    use crate::linux::user;
    use crate::utils::shell::{
        in_path,
        test_utils,
    };

    #[test]
    fn test_luks() {
        if !in_path("cryptsetup") {
            println!("WARN: skipping luks tests - no cryptsetup in path");
            return;
        }

        let fname = "./fake-luks.img";
        let passphrase = "pass1234";
        let opened_name = "fakeluks";

        if let Err(err) = test_utils::dd("/dev/zero", fname, "100M", 2) {
            panic!(
                "dd command failed to create zeroed dummy device {fname} with size 100Mx5: {err}"
            );
        }

        if !user::is_root() {
            println!("WARN: only testing luksFormat because user is not root");

            format(fname, Some(passphrase)).expect("luksFormat failed");
            return;
        }

        format(fname, Some(passphrase)).expect("luksFormat failed");
        open(fname, Some(passphrase), opened_name).expect("luksOpen failed");
        close(opened_name).expect("luksClose failed");
    }
}
