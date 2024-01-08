use crate::types::action::{
    Action,
    ActionChrootAli,
    ActionChrootUser,
    ActionMountpoints,
    ActionRoutine,
};
use crate::errors::AliError;

pub(super) fn map_err_mountpoints(
    err: AliError,
    action_failed: ActionMountpoints,
    actions_performed: Vec<ActionMountpoints>,
) -> AliError {
    AliError::ApplyError {
        error: Box::new(err),
        action_failed: Box::new(Action::Mountpoints(action_failed)),
        actions_performed: actions_performed
            .into_iter()
            .map(Action::Mountpoints)
            .collect(),
    }
}

pub(super) fn map_err_routine(
    err: AliError,
    action_failed: ActionRoutine,
    actions_performed: Vec<ActionRoutine>,
) -> AliError {
    AliError::ApplyError {
        error: Box::new(err),
        action_failed: Box::new(Action::Routines(action_failed)),
        actions_performed: actions_performed
            .into_iter()
            .map(Action::Routines)
            .collect(),
    }
}

pub(super) fn map_err_chroot_ali(
    err: AliError,
    action_failed: ActionChrootAli,
    actions_performed: Vec<ActionChrootAli>,
) -> AliError {
    AliError::ApplyError {
        error: Box::new(err),
        action_failed: Box::new(Action::ChrootAli(action_failed)),
        actions_performed: actions_performed
            .into_iter()
            .map(Action::ChrootAli)
            .collect(),
    }
}

pub(super) fn map_err_chroot_user(
    err: AliError,
    action_failed: ActionChrootUser,
    actions_performed: Vec<ActionChrootUser>,
) -> AliError {
    AliError::ApplyError {
        error: Box::new(err),
        action_failed: Box::new(Action::ChrootUser(action_failed)),
        actions_performed: actions_performed
            .into_iter()
            .map(Action::ChrootUser)
            .collect(),
    }
}
