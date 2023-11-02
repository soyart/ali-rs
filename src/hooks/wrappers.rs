use super::wrap_bad_hook_cmd;
use crate::errors::AliError;
use crate::hooks::{
    self,
    ActionHook,
    Caller,
    Hook,
    ModeHook,
    ParseError,
    KEY_WRAPPER_MNT,
    KEY_WRAPPER_NO_MNT,
};

const USAGE_MNT: &str = "<MOUNTPOINT> <HOOK_CMD>";
const USAGE_NO_MNT: &str = "<HOOK_CMD>";

struct Wrapper {
    inner: Box<dyn Hook>,
}

impl Wrapper {
    #[inline(always)]
    fn unwrap_inner(&self) -> &dyn Hook {
        self.inner.as_ref()
    }
}

/// Wraps another HookMetadata and enforce mountpoint to manifest mountpoint
struct WrapperMnt(Wrapper, String);

/// Force mountpoint value to "/"
struct WrapperNoMnt(Wrapper);

pub(super) fn parse(k: &str, cmd: &str) -> Result<Box<dyn Hook>, ParseError> {
    match k {
        KEY_WRAPPER_MNT => {
            match WrapperMnt::try_from(cmd) {
                Ok(hook) => Ok(Box::new(hook)),
                Err(err) => Err(wrap_bad_hook_cmd(err, USAGE_MNT)),
            }
        }

        KEY_WRAPPER_NO_MNT => {
            match WrapperNoMnt::try_from(cmd) {
                Ok(hook) => Ok(Box::new(hook)),
                Err(err) => Err(wrap_bad_hook_cmd(err, USAGE_NO_MNT)),
            }
        }

        key => panic!("unknown key {key}"),
    }
}

impl Hook for WrapperMnt {
    fn base_key(&self) -> &'static str {
        KEY_WRAPPER_MNT
    }

    fn usage(&self) -> &'static str {
        USAGE_MNT
    }

    fn mode(&self) -> ModeHook {
        self.unwrap_inner().mode()
    }

    fn should_chroot(&self) -> bool {
        self.unwrap_inner().should_chroot()
    }

    fn prefer_caller(&self, _caller: &Caller) -> bool {
        true
    }

    fn abort_if_no_mount(&self) -> bool {
        self.unwrap_inner().abort_if_no_mount()
    }

    fn run_hook(
        &self,
        caller: &Caller,
        root_location: &str,
    ) -> Result<ActionHook, AliError> {
        let mnt = self.1.clone();

        if mnt == "/" {
            self.eprintln_warn(&format!(
                "got mountpoint /, which makes it no different from just caller the inner hook {}",
                self.unwrap_inner().base_key(),
            ));
        }

        if root_location != mnt {
            self.eprintln_warn(&format!(
                "difference in mountpoint for hook {}",
                self.unwrap_inner().base_key()
            ));

            self.eprintln_warn(&format!(
                "mountpoint from {}: {}, mountpoint from ali-rs: {root_location}",
                self.base_key(), mnt
            ));

            self.eprintln_warn(&format!(
                "using mountpoint {} from {}",
                mnt,
                self.base_key()
            ));
        }

        self.unwrap_inner().run_hook(caller, &mnt)
    }
}

impl Hook for WrapperNoMnt {
    fn base_key(&self) -> &'static str {
        KEY_WRAPPER_NO_MNT
    }

    fn usage(&self) -> &'static str {
        "<HOOK_CMD>"
    }

    fn mode(&self) -> ModeHook {
        self.unwrap_inner().mode()
    }

    fn should_chroot(&self) -> bool {
        self.unwrap_inner().should_chroot()
    }

    fn prefer_caller(&self, _caller: &Caller) -> bool {
        true
    }

    fn abort_if_no_mount(&self) -> bool {
        self.unwrap_inner().abort_if_no_mount()
    }

    fn run_hook(
        &self,
        caller: &Caller,
        _root_location: &str,
    ) -> Result<ActionHook, AliError> {
        self.unwrap_inner().run_hook(caller, "/")
    }
}

impl TryFrom<&str> for WrapperMnt {
    type Error = AliError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let (hook_key, parts) = super::extract_key_and_parts(s)?;
        if hook_key != KEY_WRAPPER_MNT {
            return Err(AliError::AliRsBug(format!(
                "{KEY_WRAPPER_MNT}: bad key {hook_key}",
            )));
        }

        let l = parts.len();
        if l < 3 {
            return Err(AliError::BadHookCmd(format!(
                "{hook_key}: expected at least 2 arguments, got {l}",
            )));
        }

        let mountpoint = parts.get(1).unwrap();

        if !mountpoint.starts_with('/') {
            return Err(AliError::BadHookCmd(format!(
            "{hook_key}: mountpoint must be absolute, got relative path {mountpoint}",
        )));
        }
        if hooks::is_hook(mountpoint) {
            return Err(AliError::BadHookCmd(format!(
                "{hook_key}: expected mountpoint, found hook key {mountpoint}",
            )));
        }

        let inner_cmd = parts[2..].join(" ");

        let (inner_key, _) = hooks::extract_key_and_parts(&inner_cmd)?;
        let inner_hook = hooks::parse_hook(&inner_key, &inner_cmd)?;

        Ok(WrapperMnt(
            Wrapper { inner: inner_hook },
            mountpoint.to_string(),
        ))
    }
}

impl TryFrom<&str> for WrapperNoMnt {
    type Error = AliError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let (hook_key, parts) = hooks::extract_key_and_parts(s)?;
        if hook_key.as_str() != KEY_WRAPPER_NO_MNT {
            return Err(AliError::AliRsBug(format!(
                "{KEY_WRAPPER_MNT}: bad key {hook_key}",
            )));
        }

        let l = parts.len();
        if l < 1 {
            return Err(AliError::AliRsBug(format!(
                "{hook_key}: got no inner hook",
            )));
        }

        let inner_cmd_parts = &parts[1..];
        let inner_cmd = parts[1..].join(" ");
        let inner_key = inner_cmd_parts.first();

        if inner_key.is_none() {
            return Err(AliError::BadHookCmd(format!(
                "{hook_key}: missing inner hook key",
            )));
        }

        let inner_hook = hooks::parse_hook(inner_key.unwrap(), &inner_cmd)?;

        Ok(WrapperNoMnt(Wrapper { inner: inner_hook }))
    }
}

impl std::ops::Deref for WrapperMnt {
    type Target = Wrapper;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for WrapperMnt {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::ops::Deref for WrapperNoMnt {
    type Target = Wrapper;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for WrapperNoMnt {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        WrapperMnt,
        WrapperNoMnt,
    };
    use crate::errors::AliError;
    use crate::hooks::Hook;

    fn test_parse<'a, T: Hook + TryFrom<&'a str, Error = AliError>>(
        should_pass: Vec<&'a str>,
        should_err: Vec<&'a str>,
    ) {
        for s in should_pass {
            let w = T::try_from(s);

            if let Err(err) = w {
                eprintln!("got error from {s}");
                panic!("unexpected error: {err:?}");
            }
        }

        for s in should_err {
            let result = T::try_from(s);

            if result.is_ok() {
                eprintln!("unexpected ok result from {s}");
                panic!("unexpected ok result");
            }
        }
    }

    #[test]
    fn test_parse_wrapper_mnt() {
        let should_pass = vec![
            "@mnt /mnt @quicknet ens3 dns 1.1.1.1",
            "@mnt /foo @uncomment PORT /etc/ssh",
            "@mnt /foo @uncomment-print PORT /etc/ssh",
        ];

        let should_err = vec![
            "@mnt",                // Missing arg
            "@mnt @foo",           // Bad mountpoint @foo
            "@mnt baz @foo",       // Bad mountpoint baz
            "@mnt /mnt @quicknet", // Bad inner hook
        ];

        test_parse::<WrapperMnt>(should_pass, should_err);
    }

    #[test]
    fn test_parse_wrapper_no_mnt() {
        let should_pass = vec![
            "@no-mnt @uncomment PORT /mnt/etc/sshd_conf",
            "@no-mnt @replace-token 22 2222 /mnt/etc/ssh/sshd_conf",
            "@no-mnt @replace-token 22 2222 /mnt/etc/ssh/sshd_conf /mnt/etc/ssh/sshd_conf.new",
        ];

        let should_err = vec![
            "@no-mnt",                               // Missing arg
            "@no-mnt /mnt",                          // Found non-hook
            "@no-mnt foo @bar",                      // Found non-hook
            "@no-mnt @uncomment /mnt/etc/sshd_conf", // Bad @uncomment arg
        ];

        test_parse::<WrapperNoMnt>(should_pass, should_err);
    }
}
