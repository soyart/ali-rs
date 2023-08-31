use crate::defaults;
use crate::errors::AliError;
use crate::manifest::Manifest;
use crate::run::apply::Action;

pub fn ali(manifest: &Manifest, install_location: &str) -> Result<Vec<Action>, AliError> {
    let mut actions = Vec::new();

    let action_set_hostname = Action::SetHostname;
    if let Err(err) = hostname(&manifest.hostname, install_location) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: action_set_hostname,
            actions_performed: actions,
        });
    }

    actions.push(action_set_hostname);

    let action_set_tz = Action::SetTimezone;
    if let Err(err) = timezone(&manifest.timezone, install_location) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: action_set_tz,
            actions_performed: actions,
        });
    }

    actions.push(action_set_tz);

    let action_locale_gen = Action::LocaleGen;
    if let Err(err) = locale_gen() {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: action_locale_gen,
            actions_performed: actions,
        });
    }

    actions.push(action_locale_gen);

    let action_locale_conf = Action::LocaleConf;
    if let Err(err) = locale_conf() {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: action_locale_conf,
            actions_performed: actions,
        });
    }

    actions.push(action_locale_conf);

    Ok(actions)
}

fn hostname(hostname: &Option<String>, install_location: &str) -> Result<(), AliError> {
    let hostname = hostname
        .clone()
        .unwrap_or(defaults::DEFAULT_HOSTNAME.to_string());

    let etc_hostname = format!("{install_location}/etc/hostname");

    std::fs::write(&etc_hostname, hostname).map_err(|err| {
        AliError::FileError(err, format!("failed to write hostname to {etc_hostname}"))
    })
}

fn timezone(tz: &Option<String>, install_location: &str) -> Result<(), AliError> {
    let tz = tz.clone().unwrap_or(defaults::DEFAULT_TIMEZONE.to_string());

    let src = format!("{install_location}/usr/share/zoneinfo/{tz}");
    let dst = format!("{install_location}/etc/localtime");

    std::os::unix::fs::symlink(&src, &dst)
        .map_err(|err| AliError::FileError(err, format!("failed to link timezone {src} to {dst}")))
}

fn locale_gen() -> Result<(), AliError> {
    Err(AliError::NotImplemented)
}

fn locale_conf() -> Result<(), AliError> {
    Err(AliError::NotImplemented)
}
