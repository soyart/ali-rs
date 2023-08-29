use crate::errors::AliError;
use crate::manifest;
use crate::utils::shell;

/// Executes:
/// ```shell
/// pvcreate ${{ pv }}
/// ```
pub fn create_pv(pv: &str) -> Result<(), AliError> {
    shell::exec("pvcreate", &[pv])
}

/// Executes:
/// ```shell
/// vgcreate ${{ vg.name }} ${{ vg.pvs }}
/// ```
pub fn create_vg(vg: &manifest::ManifestLvmVg) -> Result<(), AliError> {
    let mut arg = vec![vg.name.as_str()];
    let pvs = vg.pvs.iter().map(|pv| pv.as_str());
    arg.extend(pvs);

    shell::exec("vgcreate", &arg)
}

/// Executes:
/// ```shell
/// lvcreate -L ${{ lv.size }} ${{ lv.vg }} -n ${{ lv.name }}
///
/// # or, if lv.size is None:
///
/// lvcreate -l 100%FREE ${{ lv.vg }} -n ${{ lv.name }}
/// ```
pub fn create_lv(lv: &manifest::ManifestLvmLv) -> Result<(), AliError> {
    let (size_flag, size) = match &lv.size {
        Some(size) => ("-L", size.as_str()),
        None => ("-l", "100FREE"),
    };

    shell::exec("lvcreate", &[size_flag, size, "-n", &lv.name])
}
