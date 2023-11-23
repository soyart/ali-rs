use crate::ali::{
    Dm,
    ManifestLuks,
    ManifestLvm,
};
use crate::entity::action::ActionMountpoints;
use crate::errors::AliError;
use crate::linux;

use super::map_err::map_err_mountpoints;

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
    let mut actions = Vec::new();
    match dm {
        Dm::Luks(ManifestLuks {
            device,
            passphrase,
            name,
        }) => {
            let passphrase = passphrase.as_ref().map(|p| p.as_str());
            let action_create = ActionMountpoints::CreateDmLuks {
                device: device.clone(),
            };

            linux::luks::format(device, passphrase)?;
            actions.push(action_create);

            let action_open = ActionMountpoints::OpenDmLuks {
                device: device.clone(),
                name: name.clone(),
            };

            linux::luks::open(device, passphrase, name)?;
            actions.push(action_open);
        }

        // For each LVM entry, do PV, then VG, then LV
        Dm::Lvm(ManifestLvm { pvs, vgs, lvs }) => {
            if let Some(pvs) = &pvs {
                for pv in pvs {
                    let action_create_pv =
                        ActionMountpoints::CreateDmLvmPv(pv.clone());

                    linux::lvm::create_pv(pv)?;
                    actions.push(action_create_pv);
                }
            }

            if let Some(vgs) = &vgs {
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

            if let Some(lvs) = &lvs {
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
        }
    }

    Ok(actions)
}
