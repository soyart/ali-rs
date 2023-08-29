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

pub fn in_path(program: &str) -> bool {
    if let Ok(path) = env::var("PATH") {
        for p in path.split(":") {
            let p_str = format!("{}/{}", p, program);
            if fs::metadata(p_str).is_ok() {
                return true;
            }
        }
    }

    false
}

#[test]
fn test_exec() {
    exec("echo", &["hello, world!"]).expect("failed to execute `echo \"hello, world!\"` command");
    exec("echo", &["hello", " world!"])
        .expect("failed to execute `echo \"hello\" \" world!\"` command");
}
