# nayi-rs

Rust implementation of [nayi](https://github.com/soyart/nayi).

It is intended to be safe to use, so it validates the manifest
before attempting to change anything on-disk.

If a manifest instruction suggests it might wipe an existing
block device, nayi-rs stops and throw an error.

## [Validation](./src/manifest/validation/)

Although nayi does specifies how YAML manifests should be parsed,
and define steps an implementation should follow, it does not
specify manifest validation.

This opens opportunities for unsafe implementations, i.e. the
ones that would blindly wipe existing data etc, which might
be desirable based on each user's use case.

Users can turn off validation, which will make nayi-rs performs
whatever steps are in the manifest without validation.

### Block device validation

#### Disks

Disks (via key `disks`) defined in the manifest will be wiped.

nayi-rs will create a new partition table, partition the disks,
and created DM devices or filesystems on top of it.

If you wish to use existing partitions, don't define them in
`disks`, instead, point to it in `dm` `rootfs` `fs`, `swap`
instead.

#### DMs (LUKS and LVM)

DMs (via key `dm`) defined in the manifest will also be created,
if and only if the matching device does not exist in the first place.

If during validation, nayi-rs found that a specified LUKS, PV, VG,
or LV already exists on the system, it will throw an error.

Apart from non-existent devices, nayi-rs also validates that the
specified DM devices have correct underlying devices, e.g.
an LV can only live on a VG, and LUKS can only live on a disk,
partition, or a LV.

Like with `disks`, if you already have a PV and VG and want to
create a new LV on top of it, then omit `pv` and `vg` YAML keys,
and only add a `lv` pointing to the desired VG via `lv.vg` key.

If nayi-rs detects that LVM2 or Btrfs were used in the block device
manifest, it helps adds `lvm2` and `btrgs-progs` packages to
`manifest.pacstrap`

### Command validation

Any commands specified in `chroot` and `postinstall` keys will
be validated just before they are run.

If your block device manifest is correct, but your command manifest
is bad, then nayi-rs will only have known about bad commands only after
it started to execute these commands - leaving you with half-installed
systems.

nayi-rs will soon have an option to only run these commands without
messing with block devices.
