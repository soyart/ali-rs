use crate::constants::defaults;
use crate::errors::AliError;
use crate::manifest::apply::Action;
use crate::manifest::Manifest;
use crate::utils::shell;

pub fn apply_routine(manifest: &Manifest, install_location: &str) -> Result<Vec<Action>, AliError> {
    let mut actions = Vec::new();

    let action_genfstab = Action::GenFstab;
    if let Err(err) = genfstab_uuid(install_location) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: Box::new(action_genfstab),
            actions_performed: actions,
        });
    }
    actions.push(action_genfstab);

    let action_set_hostname = Action::SetHostname;
    if let Err(err) = hostname(&manifest.hostname, install_location) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: Box::new(action_set_hostname),
            actions_performed: actions,
        });
    }
    actions.push(action_set_hostname);

    let action_locale_conf = Action::LocaleConf;
    if let Err(err) = locale_conf(install_location) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: Box::new(action_locale_conf),
            actions_performed: actions,
        });
    }
    actions.push(action_locale_conf);

    Ok(actions)
}

fn genfstab_uuid(install_location: &str) -> Result<(), AliError> {
    let cmd = cmd_genfstab(install_location);
    shell::exec("sh", &["-c", &format!("'{cmd}'")])
}

fn hostname(hostname: &Option<String>, install_location: &str) -> Result<(), AliError> {
    let hostname = hostname.clone().unwrap_or(defaults::HOSTNAME.to_string());

    let etc_hostname = format!("{install_location}/etc/hostname");

    std::fs::write(&etc_hostname, hostname).map_err(|err| {
        AliError::FileError(err, format!("failed to write hostname to {etc_hostname}"))
    })
}

fn locale_conf(install_location: &str) -> Result<(), AliError> {
    let dst = format!("{install_location}/etc/locale.conf");

    std::fs::write(&dst, defaults::LOCALE_CONF)
        .map_err(|err| AliError::FileError(err, format!("failed to create new locale.conf {dst}")))
}

#[inline(always)]
fn cmd_genfstab(install_location: &str) -> String {
    format!("genfstab -U {install_location} >> {install_location}/etc/fstab")
}
