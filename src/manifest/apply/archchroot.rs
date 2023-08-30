use crate::errors::AliError;
use crate::manifest::Manifest;
use crate::run::apply::Action;

pub fn archchroot_ali(manifest: &Manifest) -> Result<Vec<Action>, AliError> {
    let mut actions = Vec::new();

    let action_set_hostname = Action::SetHostname;
    if let Err(err) = hostname(&manifest.hostname) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: action_set_hostname,
            actions_performed: actions,
        });
    }

    actions.push(action_set_hostname);

    let action_set_tz = Action::SetTimezone;
    if let Err(err) = timezone(&manifest.timezone) {
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

fn hostname(hostname: &Option<String>) -> Result<(), AliError> {
    Err(AliError::NotImplemented)
}

fn timezone(tz: &Option<String>) -> Result<(), AliError> {
    Err(AliError::NotImplemented)
}

fn locale_gen() -> Result<(), AliError> {
    Err(AliError::NotImplemented)
}

fn locale_conf() -> Result<(), AliError> {
    Err(AliError::NotImplemented)
}
