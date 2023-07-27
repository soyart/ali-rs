use std::process::Command;

use crate::errors::AyiError;

pub fn exec(cmd: &str, args: &[&str]) -> Result<std::process::Output, AyiError> {
    Command::new(cmd)
        .args(args)
        .output()
        .map_err(|err| AyiError::CmdFailed(err, format!("command {} failed", cmd)))
}

#[test]
fn test_exec() {
    exec("ls", &["-a", "-l"]).expect("failed to execute `ls -a -l` command");
    exec("ls", &["-al"]).expect("failed to execute `ls -al` command");
}
