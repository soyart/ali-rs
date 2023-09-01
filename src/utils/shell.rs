use std::env;
use std::fs;
use std::process::Command;

use crate::errors::AliError;

pub fn exec(cmd: &str, args: &[&str]) -> Result<(), AliError> {
    match Command::new(cmd).args(args).spawn() {
        Ok(mut result) => match result.wait() {
            // Spawned but may still fail
            Ok(r) => match r.code() {
                Some(code) => {
                    if code != 0 {
                        return Err(AliError::CmdFailed {
                            error: None,
                            context: format!("command {cmd} exited with non-zero status {code}"),
                        });
                    }

                    Ok(())
                }
                None => Err(AliError::CmdFailed {
                    error: None,
                    context: format!("command {cmd} terminated by signal"),
                }),
            },
            Err(err) => Err(AliError::CmdFailed {
                error: Some(err),
                context: format!("command ${cmd} failed to run"),
            }),
        },

        // Failed to spawn
        Err(err) => Err(AliError::CmdFailed {
            error: Some(err),
            context: format!("command ${cmd} failed to spawn"),
        }),
    }
}

// @TODO: test chroot on Arch
pub fn chroot(location: &str, cmd: &str) -> Result<(), AliError> {
    exec("arch-chroot", &[location, cmd])
}

// Surrounds `cmd_str` with single quotes to execute:
/// ```shell
/// sh -c '{cmd_str}'
/// ```
///
/// cmd_str MUST NOT be surrounded beforehand
pub fn sh_c(cmd_str: &str) -> Result<(), AliError> {
    exec("sh", &["-c", &format!("'{cmd_str}'")])
}

#[ignore]
#[test]
fn test_shell_fns() {
    use super::fs::file_exists;

    exec("echo", &["hello, world!"]).expect("failed to execute `echo \"hello, world!\"` command");
    exec("echo", &["hello", " world!"])
        .expect("failed to execute `echo \"hello\" \" world!\"` command");

    exec("ls", &["-al", "./src"]).expect("failed to execute `ls -al ./src`");
    exec("sh", &["-c", "ls -al ./src"]).expect("failed to execute `sh -c \"ls -al ./src\"`");

    sh_c("ls -al ./src").expect("failed to use sh_c to execute `ls -al ./src`");
    sh_c("touch boobs").expect("failed to use sh_c to execute `touch boobs`");
    assert!(file_exists("boobs"));

    sh_c("touch ./boobs && rm ./boobs")
        .expect("failed to use sh_c to execute `touch boobs && rm boobs`");

    assert!(!file_exists("./boobs"));
}

pub fn in_path(program: &str) -> bool {
    if let Ok(path) = env::var("PATH") {
        for p in path.split(':') {
            let p_str = format!("{}/{}", p, program);
            if fs::metadata(p_str).is_ok() {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
#[allow(unused)]
pub mod test_utils {
    use super::exec;
    use crate::errors::AliError;
    use humanize_rs::bytes::Bytes;

    pub fn dd(infile: &str, outfile: &str, bs: &str, count: usize) -> Result<(), AliError> {
        // Check if bs is valid block size string
        bs.parse::<Bytes>()
            .map_err(|err| AliError::AliRsBug(format!("bad bs {bs} for dd: {err}")))?;

        exec(
            "dd",
            &[
                &format!("if={infile}"),
                &format!("of={outfile}"),
                &format!("bs={bs}"),
                &format!("count={count}"),
            ],
        )
    }

    pub fn rm<'a, P: AsRef<&'a str>>(fname: P) -> Result<(), AliError> {
        exec("rm", &[fname.as_ref()])
    }
}
