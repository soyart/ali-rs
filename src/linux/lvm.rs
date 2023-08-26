use crate::errors::AliError;
use crate::manifest;
use crate::utils::shell::exec;

/// Executes:
/// pvcreate ${{ pv }}
pub fn create_pv(pv: &str) -> Result<(), AliError> {
    exec("pvcreate", &[pv])
}

/// Executes:
/// vgcreate ${{ vg.name }} ${{ vg.pvs }}
pub fn create_vg(vg: &manifest::ManifestLvmVg) -> Result<(), AliError> {
    let mut arg = vec![vg.name.as_str()];
    let pvs = vg.pvs.iter().map(|pv| pv.as_str());
    arg.extend(pvs);

    exec("vgcreate", &arg)
}

/// Executes:
// lvcreate -L ${{ lv.size }} ${{ lv.vg }} -n ${{ lv.name }}
// lvcreate -l 100%FREE ${{ lv.vg }} -n ${{ lv.name }}
pub fn create_lv(lv: &manifest::ManifestLvmLv) -> Result<(), AliError> {
    let (size_flag, size) = match &lv.size {
        Some(size) => ("-L", size.as_str()),
        None => ("-l", "100FREE"),
    };

    exec("lvcreate", &[size_flag, size, "-n", &lv.name])
}
