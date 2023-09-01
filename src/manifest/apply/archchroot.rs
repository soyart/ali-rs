use crate::constants::defaults;
use crate::errors::AliError;
use crate::manifest::Manifest;
use crate::run::apply::Action;
use crate::utils::shell;

// @TODO: root password
pub fn ali(manifest: &Manifest, location: &str) -> Result<Vec<Action>, AliError> {
    let mut actions = Vec::new();

    let (action_tz, cmd_tz) = cmd_link_timezone(&manifest.timezone);
    if let Err(err) = shell::chroot(location, &cmd_tz) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: Box::new(action_tz),
            actions_performed: actions,
        });
    }
    actions.push(action_tz);

    let (action_locale_gen, cmd_locale_gen) = cmd_locale_gen();
    if let Err(err) = shell::chroot(location, &cmd_locale_gen) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: Box::new(action_locale_gen),
            actions_performed: actions,
        });
    }
    actions.push(action_locale_gen);

    Ok(actions)
}

pub fn user_chroot<'a, I>(cmds: I, location: &str) -> Result<Vec<Action>, AliError>
where
    I: Iterator<Item = &'a String>,
{
    let mut actions = Vec::new();
    for cmd in cmds {
        let action_user_cmd = Action::UserArchChrootCmd(cmd.to_string());
        if let Err(err) = shell::chroot(location, cmd) {
            return Err(AliError::InstallError {
                error: Box::new(err),
                action_failed: Box::new(action_user_cmd),
                actions_performed: actions,
            });
        }

        actions.push(action_user_cmd);
    }

    Ok(actions)
}

fn cmd_link_timezone(tz: &Option<String>) -> (Action, String) {
    let tz = tz.clone().unwrap_or(defaults::TIMEZONE.to_string());
    let tz_cmd = format!("ln -s /usr/share/zoneinfo/{} /etc/localtime", tz);

    (Action::SetTimezone(tz), tz_cmd)
}

// Appends defaults::DEFAULT_LOCALE_GEN to /etc/locale.gen
fn cmd_locale_gen() -> (Action, String) {
    (
        Action::LocaleGen,
        format!(
            "echo {} >> /etc/locale.gen && locale-gen",
            defaults::LOCALE_GEN
        ),
    )
}
