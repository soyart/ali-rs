# {{crate}}

{{badges}}

```text
┏━┓╻  ╻   ┏━┓┏━┓
┣━┫┃  ┃╺━╸┣┳┛┗━┓
╹ ╹┗━╸╹   ╹┗╸┗━┛
```

Rust implementation of [ALI](https://github.com/soyart/ali),
the Aux Linarch Installer.

## ALI manifest validation

ali-rs is intended to be safe to use, so it validates the manifest
before attempting to change anything on-disk.

If a manifest instruction suggests it might wipe an existing
block device, ali-rs exits and throw an error.

It is possible to skip validation with `--no-validate` flags,
and it is possible to overwrite existing system devices with
`-o` or `--overwrite` flags.

ali-rs also provides [ali-rs hooks](./HOOKS.md) as an extension of ALI.

## Usage

Run `ali-rs -h` to get the list of all available subcommands,
or `ali-rs <subcommand> -h` to get available subcommand options.

Currently, if no subcommand is given, ali-rs defaults to manifest
validation which is safe to run.

## ALI manifest application

Once the validation step is done (or skipped), ali-rs applies
the manifest in stages.

Each stage groups closely related _actions_ together,
and they are applied in a particular order. If any of the stages
failed, ali-rs exits.

## Root password in ali-rs

User `root` password (hashed) is defined in manifest key
[`rootpasswd`](https://github.com/soyart/ali/blob/master/ALI.md#key-rootpasswd).

If not given, ali-rs will use the default password as defined in
[`constants.rs`](./src/constants.rs), currently `archalirs`.

Note that users can always do a manual `chroot` to change root password
any time after the installer exits.

> ALI spec does not specify what an installer should do in case it is not given.

### ALI manifest application stages in ali-rs

ali-rs follows ALI steps in this strict order:

1. `stage-mountpoints`

   This stage contains actions relating to preparing block devices
   and filesystems.

2. `stage-bootstrap`

   This stage contains actions relating to using `pacstrap(8)` to
   pre-install packages before ali-rs does `arch-chroot(1)`.

3. `stage-routines`

   This stage contains actions that ali-rs will apply on the behalf
   of the users **outside of a `chroot(1)`**, e.g. writing `/etc/locale.gen`,
   `/etc/locale.conf`, `/etc/hostname`, and populating `/etc/fstab`
   with `genfstab(8)`.

4. `stage-chroot_ali`

   This stage contains actions that ali-rs will apply on the behalf
   of the users **inside of `chroot(1)`**, e.g. linking timezones, and
   generating locale with `locale-gen`.

5. `stage-chroot_user`

   This stage executes user-defined shell commands in manifest key `chroot`
   **inside of `chroot(1)`**. Users could use this stage to configure their
   bootloader or set root password.

6. `stage-postinstall_user`

   This stage executes user-defined shell commands in manifest key `postinstall`
   **outside of `chroot(1)`**. This is currently the last stage of ALI.

## [Validation details](./src/ali/validation/)

Although ALI does specifies how YAML manifests should be parsed,
and define steps an implementation should follow, it does not
specify manifest validation.

This opens opportunities for unsafe implementations, i.e. the
ones that would blindly wipe existing data etc, which might
be desirable based on each user's use case.

Users can turn off validation, which will make ali-rs performs
whatever steps are in the manifest without validation.

### Block device validation

#### Disks

Disks (via key `disks`) defined in the manifest will be wiped.

ali-rs will create a new partition table, partition the disks,
and created DM devices or filesystems on top of it.

If you wish to use existing partitions, don't define them in
`disks`, instead, point to it in `dm` `rootfs` `fs`, `swap`
instead.

#### DMs (LUKS and LVM)

DMs (via key `dm`) defined in the manifest will also be created,
if and only if the matching device does not exist in the first place.

If during validation, ali-rs found that a specified LUKS, PV, VG,
or LV already exists on the system, it will throw an error.

Apart from non-existent devices, ali-rs also validates that the
specified DM devices have correct underlying devices, e.g.
an LV can only live on a VG, and LUKS can only live on a disk,
partition, or a LV.

Like with `disks`, if you already have a PV and VG and want to
create a new LV on top of it, then omit `pv` and `vg` YAML keys,
and only add a `lv` pointing to the desired VG via `lv.vg` key.

If ali-rs detects that LVM2 or Btrfs were used in the block device
manifest, it helps adds `lvm2` and `btrgs-progs` packages to
`manifest.pacstrap`

### Command validation

Any commands specified in `chroot` and `postinstall` keys will
be validated just before they are run.

If your block device manifest is correct, but your command manifest
is bad, then ali-rs will only have known about bad commands only after
it started to execute these commands - leaving you with half-installed
systems.

ali-rs will soon have an option to only run these commands without
messing with block devices.

{{readme}}

Version: {{version}}
License: {{license}}