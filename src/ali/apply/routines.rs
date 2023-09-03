use crate::ali::Manifest;
use crate::constants::defaults;
use crate::entity::report::ActionRoutine;
use crate::errors::AliError;
use crate::utils::shell;

use super::map_err::map_err_routine;

pub fn ali_routines(
    manifest: &Manifest,
    install_location: &str,
) -> Result<Vec<ActionRoutine>, AliError> {
    let mut actions = Vec::new();

    let action_genfstab = ActionRoutine::GenFstab;
    if let Err(err) = genfstab_uuid(install_location) {
        return Err(map_err_routine(err, action_genfstab, actions));
    }
    actions.push(action_genfstab);

    let action_set_hostname = ActionRoutine::SetHostname;
    if let Err(err) = hostname(&manifest.hostname, install_location) {
        return Err(map_err_routine(err, action_set_hostname, actions));
    }
    actions.push(action_set_hostname);

    let action_locale_conf = ActionRoutine::LocaleConf;
    if let Err(err) = locale_conf(install_location) {
        return Err(map_err_routine(err, action_locale_conf, actions));
    }
    actions.push(action_locale_conf);

    Ok(actions)
}

fn genfstab_uuid(install_location: &str) -> Result<(), AliError> {
    shell::sh_c(&cmd_genfstab_uuid(install_location))
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
fn cmd_genfstab_uuid(install_location: &str) -> String {
    format!("genfstab -U {install_location} >> {install_location}/etc/fstab")
}
