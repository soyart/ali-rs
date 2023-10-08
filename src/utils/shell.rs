use std::process::{
    Command,
    Stdio,
};
use std::{
    env,
    fs,
};

use crate::errors::AliError;

pub enum CmdError {
    /// Command spawned, but returned non-0 exit code
    ErrRun {
        code: Option<i32>,
        stdout: Option<Vec<u8>>,
        stderr: Option<Vec<u8>>,
    },

    /// Command failed to spawn
    ErrSpawn { error: std::io::Error },
}

/// Executes command `cmd` with arguments `args`.
/// Output is discarded (printed to console) and not used.
/// Throw an error if `cmd` fails to spawn or exit code != 0
pub fn exec(cmd: &str, args: &[&str]) -> Result<(), AliError> {
    match Command::new(cmd).args(args).spawn() {
        Ok(mut result) => {
            match result.wait() {
                // Spawned but may still fail
                Ok(r) => {
                    match r.code() {
                        Some(0) => Ok(()),
                        Some(code) => {
                            Err(AliError::CmdFailed {
                                error: CmdError::ErrRun {
                                    code: Some(code),
                                    stdout: None,
                                    stderr: None,
                                },
                                context: format!(
                            "command {cmd} exited with non-zero status {code}"
                        ),
                            })
                        }
                        None => {
                            Err(AliError::CmdFailed {
                                error: CmdError::ErrRun {
                                    code: None,
                                    stdout: None,
                                    stderr: None,
                                },
                                context: format!(
                                    "command {cmd} terminated by signal"
                                ),
                            })
                        }
                    }
                }
                Err(error) => {
                    Err(AliError::CmdFailed {
                        error: CmdError::ErrSpawn { error },
                        context: format!("command ${cmd} failed to run"),
                    })
                }
            }
        }

        // Failed to spawn
        Err(error) => {
            Err(AliError::CmdFailed {
                error: CmdError::ErrSpawn { error },
                context: format!("command {cmd} failed to spawn"),
            })
        }
    }
}

/// Executes command `cmd` with arguments `args`,
/// capturing output and returning stdout output as bytes,
/// or stderr output as lossy UTF-8 strings.
///
/// Throws an error if command fails to spawn
#[allow(unused)]
pub fn exec_with_output(cmd: &str, args: &[&str]) -> Result<Vec<u8>, AliError> {
    let output = Command::new(cmd).args(args).output().map_err(|err| {
        AliError::CmdFailed {
            error: CmdError::ErrSpawn { error: err },
            context: format!("command {cmd} failed to spawn"),
        }
    })?;

    if !output.status.success() {
        let stdout = Some(output.stdout);
        let stderr = Some(output.stderr);

        return Err(AliError::CmdFailed {
            error: CmdError::ErrRun {
                code: output.status.code(),
                stdout,
                stderr,
            },
            context: format!(
                "command {cmd} {} exited with non-zero status",
                args.join(" ")
            ),
        });
    }

    Ok(output.stdout)
}

/// Pipe stdout of `producer_cmd` to stdin of `consumer_cmd`,
/// and waits for `consumer_cmd` to finish.
/// Akin to:
/// ```shell
/// producer_cmd | consume_cmd
/// ```
/// The structure of both argument tuples is (cmd, &[arg1, arg2, ..])
pub fn pipe(
    producer_cmd: (&str, &[&str]),
    consumer_cmd: (&str, &[&str]),
) -> Result<(), AliError> {
    let producer = Command::new(producer_cmd.0)
        .args(producer_cmd.1)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| {
            panic!(
                "failed to spawn producer {} {}",
                consumer_cmd.0,
                consumer_cmd.1.join(" ")
            )
        });

    // Ignore fdisk stderr - it will be inherited from ali-rs
    let consumer = Command::new(consumer_cmd.0)
        .args(consumer_cmd.1)
        .stdin(producer.stdout.unwrap())
        .spawn()
        .unwrap_or_else(|_| {
            panic!(
                "failed to spawn consumer {} {}",
                consumer_cmd.0,
                consumer_cmd.1.join(" ")
            )
        });

    match consumer.wait_with_output() {
        Ok(result) => {
            match result.status.success() {
                false => {
                    Err(AliError::CmdFailed {
                        error: CmdError::ErrRun {
                            code: result.status.code(),
                            stdout: None,
                            stderr: Some(result.stderr),
                        },
                        context: format!(
                            "consumer {} command exited with bad status: {}",
                            consumer_cmd.0,
                            result
                                .status
                                .code()
                                .expect("failed to get exit code"),
                        ),
                    })
                }
                _ => Ok(()),
            }
        }
        Err(error) => {
            Err(AliError::CmdFailed {
                error: CmdError::ErrRun {
                    code: None,
                    stdout: None,
                    stderr: None,
                },
                context: format!(
                    "consumer {} command failed to run: {error}",
                    consumer_cmd.0
                ),
            })
        }
    }
}

// Executes cmd_str with `sh -c`:
/// ```shell
/// sh -c {cmd_str}
/// ```
///
/// cmd_str MUST NOT be surrounded beforehand
pub fn sh_c(cmd_str: &str) -> Result<(), AliError> {
    exec("sh", &["-c", cmd_str])
}

/// Executes cmd with `arch-chroot` to `location`
/// Example: location: `"/mnt"`, cmd: `"cat /etc/fstab"`:
/// ```shell
/// sh -c arch-chroot /mnt cat /etc/fstab
/// ```
pub fn arch_chroot(location: &str, cmd: &str) -> Result<(), AliError> {
    sh_c(&format!("arch-chroot {location} {cmd}"))
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

impl std::fmt::Debug for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ErrRun {
                code,
                stdout,
                stderr,
            } => {
                let code = match code {
                    Some(c) => c.to_string(),
                    None => "non-zero".to_string(),
                };

                let stdout = match stdout {
                    Some(ref bytes) => {
                        String::from_utf8(bytes.clone())
                            .unwrap_or("binary output".to_string())
                    }
                    None => "ali-rs discarded stdout output".to_string(),
                };

                let stderr = match stderr {
                    Some(err) => String::from_utf8_lossy(err).into(),
                    None => "ali-rs discarded stderr output".to_string(),
                };

                write!(
                    f,
                    "run error\nexit code: {code}\nstdout: {stdout}\nstderr: {stderr}",
                )
            }
            Self::ErrSpawn { error } => write!(f, "spawn error: {error:?}"),
        }
    }
}

impl std::fmt::Display for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "self:?")
    }
}

#[ignore]
#[test]
fn test_shell_fns() {
    use super::fs::file_exists;

    exec("echo", &["hello, world!"])
        .expect("failed to execute `echo \"hello, world!\"` command");
    exec("echo", &["hello,", "world!"])
        .expect("failed to execute `echo \"hello,\" \" world!\"` command");

    exec("ls", &["-al", "./src"]).expect("failed to execute `ls -al ./src`");
    exec("sh", &["-c", "ls -al ./src"])
        .expect("failed to execute `sh -c \"ls -al ./src\"`");

    sh_c("ls -al ./src").expect("failed to use sh_c to execute `ls -al ./src`");
    sh_c("touch ./boobs").expect("failed to use sh_c to execute `touch boobs`");
    assert!(file_exists("./boobs"));

    sh_c("touch ./boobs && rm ./boobs")
        .expect("failed to use sh_c to execute `touch boobs && rm boobs`");

    assert!(!file_exists("./boobs"));
}

#[cfg(test)]
#[allow(unused)]
pub mod test_utils {
    use super::exec;
    use crate::errors::AliError;
    use humanize_rs::bytes::Bytes;

    pub fn dd(
        infile: &str,
        outfile: &str,
        bs: &str,
        count: usize,
    ) -> Result<(), AliError> {
        // Check if bs is valid block size string
        bs.parse::<Bytes>().map_err(|err| {
            AliError::AliRsBug(format!("bad bs {bs} for dd: {err}"))
        })?;

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
