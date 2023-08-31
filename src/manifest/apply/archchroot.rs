use crate::defaults;
use crate::errors::AliError;
use crate::manifest::Manifest;
use crate::run::apply::Action;
use crate::utils::shell;

pub fn ali(manifest: &Manifest, install_location: &str) -> Result<Vec<Action>, AliError> {
    let mut actions = Vec::new();

    let (action_tz, cmd_tz) = cmd_link_timezone(&manifest.timezone);
    if let Err(err) = shell::chroot(install_location, &cmd_tz) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: action_tz,
            actions_performed: actions,
        });
    }
    actions.push(action_tz);

    let (action_locale_gen, cmd_locale_gen) = cmd_locale_gen();
    if let Err(err) = shell::chroot(install_location, &cmd_locale_gen) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: action_locale_gen,
            actions_performed: actions,
        });
    }
    actions.push(action_locale_gen);

    let (action_locale_conf, cmd_locale_conf) = cmd_locale_conf();
    if let Err(err) = shell::chroot(install_location, &cmd_locale_conf) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: action_locale_conf,
            actions_performed: actions,
        });
    }
    actions.push(action_locale_conf);

    Ok(actions)
}

fn cmd_link_timezone(tz: &Option<String>) -> (Action, String) {
    let tz = tz.clone().unwrap_or(defaults::DEFAULT_TIMEZONE.to_string());
    let tz_cmd = format!("ln -s /usr/share/zoneinfo/{} /etc/localtime", tz);

    (Action::SetTimezone(tz), tz_cmd)
}

// Appends defaults::DEFAULT_LOCALE_GEN to /etc/locale.gen
fn cmd_locale_gen() -> (Action, String) {
    (
        Action::LocaleGen,
        format!(
            "echo {} >> /etc/locale.gen && locale-gen",
            defaults::DEFAULT_LOCALE_GEN
        ),
    )
}

fn cmd_locale_conf() -> (Action, String) {
    (
        Action::LocaleConf,
        format!("echo {} > /etc/locale.conf", defaults::DEFAULT_LOCALE_CONF),
    )
}
