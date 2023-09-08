use crate::ali::Manifest;
use crate::constants::defaults;
use crate::entity::action::{ActionChrootAli, ActionChrootUser};
use crate::errors::AliError;
use crate::utils::shell;

use super::map_err::*;

// @TODO: root password
pub fn chroot_ali(manifest: &Manifest, location: &str) -> Result<Vec<ActionChrootAli>, AliError> {
    let mut actions = Vec::new();

    let (action_tz, cmd_tz) = cmd_link_timezone(&manifest.timezone);
    if let Err(err) = shell::arch_chroot(location, &cmd_tz) {
        return Err(map_err_chroot_ali(err, action_tz, actions));
    }
    actions.push(action_tz);

    let (action_locale_gen, cmd_locale_gen) = cmd_locale_gen();
    if let Err(err) = shell::arch_chroot(location, &cmd_locale_gen) {
        return Err(map_err_chroot_ali(err, action_locale_gen, actions));
    }
    actions.push(action_locale_gen);

    Ok(actions)
}

pub fn chroot_user<'a, I>(cmds: I, location: &str) -> Result<Vec<ActionChrootUser>, AliError>
where
    I: Iterator<Item = &'a String>,
{
    let mut actions = Vec::new();

    for cmd in cmds {
        let action_user_cmd = ActionChrootUser::UserArchChrootCmd(cmd.to_string());
        if let Err(err) = shell::arch_chroot(location, cmd) {
            return Err(map_err_chroot_user(err, action_user_cmd, actions));
        }

        actions.push(action_user_cmd);
    }

    Ok(actions)
}

fn cmd_link_timezone(tz: &Option<String>) -> (ActionChrootAli, String) {
    let tz = tz.clone().unwrap_or(defaults::TIMEZONE.to_string());
    let tz_cmd = format!("ln -s /usr/share/zoneinfo/{} /etc/localtime", tz);

    (ActionChrootAli::LinkTimezone(tz), tz_cmd)
}

// Appends defaults::DEFAULT_LOCALE_GEN to /etc/locale.gen
fn cmd_locale_gen() -> (ActionChrootAli, String) {
    (
        ActionChrootAli::LocaleGen,
        format!(
            "echo {} >> /etc/locale.gen && locale-gen",
            defaults::LOCALE_GEN
        ),
    )
}
