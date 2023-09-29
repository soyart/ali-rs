use crate::cli;
use crate::errors::AliError;
use crate::hooks;

pub fn run(cli_args: cli::ArgsHooks) -> Result<(), AliError> {
    for hook in cli_args.hooks {
        // If args.mountpoint is none, then assume it's not in chroot
        let (in_chroot, mountpoint) = cli_args
            .mountpoint
            .clone()
            .map_or((false, String::new()), |mnt_chroot| (true, mnt_chroot));

        hooks::apply_hook(&hook, in_chroot, &mountpoint)?;
    }

    Ok(())
}
