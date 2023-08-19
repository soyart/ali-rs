use std::env;
use std::fs;
use std::process::Command;

use crate::errors::NayiError;

pub fn exec(cmd: &str, args: &[&str]) -> Result<(), NayiError> {
    match Command::new(cmd).args(args).spawn() {
        Ok(mut result) => match result.wait() {
            // Spawned but may still fail
            Ok(r) => match r.code() {
                Some(code) => {
                    if code != 0 {
                        return Err(NayiError::CmdFailed(
                            None,
                            format!("command {cmd} exited with non-zero status {code}"),
                        ));
                    }

                    Ok(())
                }
                None => Err(NayiError::CmdFailed(
                    None,
                    format!("command {cmd} terminated by signal"),
                )),
            },
            Err(err) => Err(NayiError::CmdFailed(
                Some(err),
                format!("command ${cmd} failed to run"),
            )),
        },

        // Failed to spawn
        Err(err) => Err(NayiError::CmdFailed(
            Some(err),
            format!("command ${cmd} failed to spawn"),
        )),
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
