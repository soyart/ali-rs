use std::env;
use std::fs;
use std::process::Command;

use crate::errors::NayiError;

pub fn exec(cmd: &str, args: &[&str]) -> Result<(), NayiError> {
    match Command::new(cmd).args(args).spawn() {
        Ok(mut result) => match result.wait() {
            Ok(r) => r.exit_ok().map_err(|err| {
                NayiError::CmdFailed(
                    None,
                    format!("command {cmd} exited with bad status {}", err.to_string()),
                )
            }),
            Err(err) => Err(NayiError::CmdFailed(
                Some(err),
                format!("command ${cmd} failed to run"),
            )),
        },
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
