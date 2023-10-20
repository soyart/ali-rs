use crate::errors::AliError;
use crate::hooks::{
    self,
    ActionHook,
    Caller,
    Hook,
    ModeHook,
    KEY_WRAPPER_MNT,
    KEY_WRAPPER_NO_MNT,
};

#[derive(Default)]
struct Wrapper {
    inner: Option<Box<dyn Hook>>,
}

impl Wrapper {
    #[inline(always)]
    fn unwrap_inner(&self) -> &dyn Hook {
        self.inner.as_ref().unwrap().as_ref()
    }
}

/// Wraps another HookMetadata and enforce mountpoint to manifest mountpoint
#[derive(Default)]
struct WrapperMnt(Wrapper, Option<String>);

/// Force mountpoint value to "/"
#[derive(Default)]
struct WrapperNoMnt(Wrapper);

pub(super) fn init_from_key(key: &str) -> Box<dyn Hook> {
    match key {
        KEY_WRAPPER_MNT => Box::<WrapperMnt>::default(),
        KEY_WRAPPER_NO_MNT => Box::<WrapperNoMnt>::default(),
        _ => panic!("unknown key {key}"),
    }
}

impl Hook for WrapperMnt {
    fn base_key(&self) -> &'static str {
        KEY_WRAPPER_MNT
    }

    fn usage(&self) -> &'static str {
        "<MOUNTPOINT> <HOOK_CMD>"
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

    fn parse_cmd(&mut self, s: &str) -> Result<(), AliError> {
        parse_wrapper_mnt(self, s)
    }

    fn run_hook(
        &self,
        caller: &Caller,
        root_location: &str,
    ) -> Result<ActionHook, AliError> {
        if self.1.is_none() {
            panic!("none mountpoint for WrapperMnt")
        }

        let mnt = self.1.clone().unwrap();

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

    fn parse_cmd(&mut self, s: &str) -> Result<(), AliError> {
        parse_wrapper_no_mnt(self, s)
    }

    fn run_hook(
        &self,
        caller: &Caller,
        _root_location: &str,
    ) -> Result<ActionHook, AliError> {
        self.unwrap_inner().run_hook(caller, "/")
    }
}

fn parse_wrapper_mnt(w: &mut WrapperMnt, cmd: &str) -> Result<(), AliError> {
    let (key, parts) = hooks::extract_key_and_parts(cmd)?;
    if key != KEY_WRAPPER_MNT {
        return Err(AliError::AliRsBug(format!(
            "{}: bad key {key}",
            w.base_key()
        )));
    }

    let l = parts.len();
    if l < 3 {
        return Err(AliError::BadHookCmd(format!(
            "{}: expected at least 2 arguments, got {l}",
            w.base_key()
        )));
    }

    let mountpoint = parts.get(1).unwrap();

    if !mountpoint.starts_with('/') {
        return Err(AliError::BadHookCmd(format!(
            "{}: mountpoint must be absolute, got relative path {mountpoint}",
            w.base_key()
        )));
    }
    if hooks::is_hook(mountpoint) {
        return Err(AliError::BadHookCmd(format!(
            "{}: expected mountpoint, found hook key {mountpoint}",
            w.base_key()
        )));
    }

    let inner_cmd = parts[2..].join(" ");

    let (inner_key, _) = hooks::extract_key_and_parts(&inner_cmd)?;
    let mut inner_meta = hooks::init_blank_hook(&inner_key)?;

    inner_meta.parse_cmd(&inner_cmd)?;

    w.inner = Some(inner_meta);
    w.1 = Some(mountpoint.to_owned());

    Ok(())
}

fn parse_wrapper_no_mnt(w: &mut WrapperNoMnt, s: &str) -> Result<(), AliError> {
    let (key, parts) = hooks::extract_key_and_parts(s)?;
    if key.as_str() != KEY_WRAPPER_NO_MNT {
        return Err(AliError::AliRsBug(format!(
            "{}: bad key {key}",
            w.base_key()
        )));
    }

    let l = parts.len();
    if l < 1 {
        return Err(AliError::AliRsBug(format!(
            "{}: got no inner hook",
            w.base_key()
        )));
    }

    let inner_cmd_parts = &parts[1..];
    let inner_cmd = parts[1..].join(" ");
    let inner_key = inner_cmd_parts.first();

    if inner_key.is_none() {
        return Err(AliError::BadHookCmd(format!(
            "{}: missing inner hook key",
            w.base_key()
        )));
    }

    let mut inner_meta = hooks::init_blank_hook(inner_key.unwrap())?;
    inner_meta.parse_cmd(&inner_cmd)?;

    w.inner = Some(inner_meta);

    Ok(())
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
    use crate::hooks::Hook;

    fn test_parse<T: Hook>(
        f: fn() -> T,
        should_pass: Vec<&str>,
        should_err: Vec<&str>,
    ) {
        for s in should_pass {
            let mut w = f();
            let result = w.parse_cmd(s);

            if let Err(err) = result {
                eprintln!("got error from {s}");
                panic!("unexpected error: {err}");
            }
        }

        for s in should_err {
            let mut w = f();
            let result = w.parse_cmd(s);

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

        test_parse(WrapperMnt::default, should_pass, should_err);
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

        test_parse(WrapperNoMnt::default, should_pass, should_err);
    }
}
