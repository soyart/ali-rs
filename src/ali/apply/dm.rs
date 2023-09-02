use crate::ali::Dm;
use crate::errors::AliError;
use crate::linux;
use crate::run::apply::{map_err_mountpoints, ActionMountpoints};

pub fn apply_dms(dms: &[Dm]) -> Result<Vec<ActionMountpoints>, AliError> {
    let mut actions = Vec::new();

    let action_dms = ActionMountpoints::ApplyDms;
    for dm in dms {
        match apply_dm(dm) {
            Err(err) => {
                return Err(map_err_mountpoints(err, action_dms, actions));
            }
            Ok(actions_dm) => {
                actions.extend(actions_dm);
            }
        };
    }

    actions.push(action_dms);

    Ok(actions)
}

pub fn apply_dm(dm: &Dm) -> Result<Vec<ActionMountpoints>, AliError> {
    match dm {
        Dm::Luks(_) => Err(AliError::NotImplemented("Apply LUKS".to_string())),
        Dm::Lvm(lvm) => {
            let mut actions = Vec::new();

            if let Some(pvs) = &lvm.pvs {
                for pv in pvs {
                    let action_create_pv = ActionMountpoints::CreateDmLvmPv(pv.clone());

                    linux::lvm::create_pv(pv)?;
                    actions.push(action_create_pv);
                }
            }

            if let Some(vgs) = &lvm.vgs {
                for vg in vgs {
                    let vg_name = format!("/dev/{}", vg.name);
                    let action_create_vg = ActionMountpoints::CreateDmLvmVg {
                        pvs: vg.pvs.clone(),
                        vg: vg_name.clone(),
                    };

                    linux::lvm::create_vg(vg)?;
                    actions.push(action_create_vg);
                }
            }

            if let Some(lvs) = &lvm.lvs {
                for lv in lvs {
                    let vg_name = format!("/dev/{}", lv.vg);
                    let lv_name = format!("{vg_name}/{}", lv.name);
                    let action_create_lv = ActionMountpoints::CreateDmLvmLv {
                        vg: vg_name.clone(),
                        lv: lv_name.clone(),
                    };

                    linux::lvm::create_lv(lv)?;
                    actions.push(action_create_lv);
                }
            }

            Ok(actions)
        }
    }
}
