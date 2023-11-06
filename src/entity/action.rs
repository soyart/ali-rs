use std::collections::HashSet;

use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    ali,
    hooks,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Action {
    Mountpoints(ActionMountpoints),
    Bootstrap(ActionBootstrap),
    Routines(ActionRoutine),
    ChrootAli(ActionChrootAli),
    ChrootUser(ActionChrootUser),
    UserPostInstall(ActionPostInstallUser),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionMountpoints {
    #[serde(rename = "applyDisk")]
    ApplyDisk { device: String },

    #[serde(rename = "applyDisks")]
    ApplyDisks,

    #[serde(rename = "appliedDms")]
    ApplyDms,

    #[serde(rename = "applyDm")]
    ApplyDm,

    #[serde(rename = "applyRootFs")]
    ApplyRootfs,

    #[serde(rename = "applyFilesystems")]
    ApplyFilesystems,

    #[serde(rename = "mkdirRootFs")]
    MkdirRootFs,

    #[serde(rename = "mountRootFs")]
    MountRootFs,

    #[serde(rename = "mkdirFs")]
    MkdirFs(String),

    #[serde(rename = "mountFilesystems")]
    MountFilesystems,

    #[serde(rename = "createPartitionTable")]
    CreatePartitionTable {
        device: String,
        table: ali::PartitionTable,
    },

    #[serde(rename = "createPartition")]
    CreatePartition {
        device: String,
        number: usize,
        size: String,
    },

    #[serde(rename = "setParitionType")]
    SetPartitionType {
        device: String,
        number: usize,
        partition_type: String,
    },

    #[serde(rename = "createDmLuks")]
    CreateDmLuks { device: String },

    #[serde(rename = "createLvmPv")]
    CreateDmLvmPv(String),

    #[serde(rename = "createLvmVg")]
    CreateDmLvmVg { pvs: Vec<String>, vg: String },

    #[serde(rename = "createLvmLv")]
    CreateDmLvmLv { vg: String, lv: String },

    #[serde(rename = "createFilesystem")]
    CreateFs {
        device: String,
        fs_type: String,
        fs_opts: Option<String>,
    },

    #[serde(rename = "mountFilesystem")]
    MountFs {
        src: String,
        dst: String,
        opts: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]

pub enum ActionBootstrap {
    #[serde(rename = "installBase")]
    InstallBase,

    #[serde(rename = "installPackages")]
    InstallPackages { packages: HashSet<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]

pub enum ActionRoutine {
    #[serde(rename = "setHostname")]
    SetHostname,

    #[serde(rename = "genfstab")]
    GenFstab,

    #[serde(rename = "localeConf")]
    LocaleConf,

    #[serde(rename = "rootPasswd")]
    RootPasswd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]

pub enum ActionChrootAli {
    #[serde(rename = "linkTimezone")]
    LinkTimezone(String),

    #[serde(rename = "localeGen")]
    LocaleGen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]

pub enum ActionChrootUser {
    #[serde(rename = "userArchChrootCmd")]
    UserArchChrootCmd(String),

    #[serde(rename = "aliRsHookChrootUser")]
    Hook(hooks::ActionHook),
}

#[derive(Debug, Clone, Serialize, Deserialize)]

pub enum ActionPostInstallUser {
    #[serde(rename = "userPostInstallCmd")]
    UserPostInstallCmd(String),

    #[serde(rename = "aliRsHookPostInstall")]
    Hook(hooks::ActionHook),
}

#[ignore = "Ignored because just dummy print JSON"]
#[test]
// Dummy function to see JSON result
fn test_json_stages() {
    use super::report::Report;
    use super::stage::StageActions;
    use ali::PartitionTable;

    let actions_mountpoints = vec![
        ActionMountpoints::CreatePartitionTable {
            device: "/dev/sda".into(),
            table: PartitionTable::Gpt,
        },
        ActionMountpoints::CreatePartition {
            device: "/dev/sda1".into(),
            number: 1,
            size: "8G".into(),
        },
        ActionMountpoints::CreateFs {
            device: "/dev/sda1".into(),
            fs_type: "btrfs".into(),
            fs_opts: None,
        },
    ];

    let actions_bootstrap = vec![
        ActionBootstrap::InstallBase,
        ActionBootstrap::InstallPackages {
            packages: HashSet::from([
                "git".to_string(),
                "rustup".to_string(),
                "curl".to_string(),
            ]),
        },
    ];

    let actions_routines =
        vec![ActionRoutine::GenFstab, ActionRoutine::LocaleConf];

    let actions_chroot_ali = vec![
        ActionChrootAli::LinkTimezone("Asia/Bangkok".to_string()),
        ActionChrootAli::LocaleGen,
    ];

    let actions_chroot_user = vec![ActionChrootUser::UserArchChrootCmd(
        "curl https://foo.bar/loader_conf.conf > /boot/loader/entries/default.conf "
            .to_string(),
    )];

    let actions_postinstall_user =
        vec![ActionPostInstallUser::UserPostInstallCmd(
            "grep vultr /alitarget/boot/loader/entries/default.conf"
                .to_string(),
        )];

    let stages = StageActions {
        mountpoints: actions_mountpoints.clone(),
        bootstrap: actions_bootstrap.clone(),
        routines: actions_routines.clone(),
        chroot_ali: actions_chroot_ali.clone(),
        chroot_user: actions_chroot_user.clone(),
        postinstall_user: actions_postinstall_user.clone(),
    };

    let report = Report {
        summary: Box::new(stages),
        duration: std::time::Duration::from_secs(20),
        location: "dummy".to_string(),
    };

    println!("{}", report.to_json_string());
}
