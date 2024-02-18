use crate::ali::Manifest;
use crate::errors::AliError;
use crate::{
    cli,
    hooks,
};

pub fn run(
    manifest: &String,
    cli_args: cli::ArgsHooks,
) -> Result<(), AliError> {
    let hooks = collect_hooks(manifest, &cli_args)?;
    let mountpoint = extract_mountpoint(&cli_args);

    if cli_args.dry_run {
        return validate(hooks, mountpoint);
    }

    for hook in hooks {
        hooks::apply_hook(&hook, &hooks::Caller::Cli, &mountpoint)?;
    }

    Ok(())
}

fn validate(hooks: Vec<String>, mountpoint: String) -> Result<(), AliError> {
    for hook in hooks {
        hooks::validate_hook(&hook, &hooks::Caller::Cli, &mountpoint)?;
    }

    Ok(())
}

fn collect_hooks(
    manifest_file: &String,
    cli_args: &cli::ArgsHooks,
) -> Result<Vec<String>, AliError> {
    match cli_args.use_manifest {
        true => {
            let manifest_yaml = std::fs::read_to_string(manifest_file)
                .map_err(|err| {
                    AliError::FileError(err, manifest_file.to_string())
                })?;

            let manifest = Manifest::from_yaml(&manifest_yaml)?;
            let mut manifest_hooks = vec![];

            if let Some(cmds) = manifest.chroot {
                for s in cmds {
                    if hooks::is_hook(&s) {
                        manifest_hooks.push(s);
                    }
                }
            }

            if let Some(cmds) = manifest.postinstall {
                for s in cmds {
                    if hooks::is_hook(&s) {
                        manifest_hooks.push(s);
                    }
                }
            }

            Ok(manifest_hooks)
        }

        false => Ok(cli_args.hooks.clone()),
    }
}

fn extract_mountpoint(cli_args: &cli::ArgsHooks) -> String {
    cli_args.mountpoint.clone().unwrap_or(String::from("/"))
}
