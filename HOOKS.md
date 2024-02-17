# ali-rs hooks

Hooks are special commands starting with `@<HOOK_NAME>`.

Users register hooks inside manifest keys [`chroot`](https://github.com/soyart/ali/blob/master/ALI.md#key-chroot)
and [`postinstall`](https://github.com/soyart/ali/blob/master/ALI.md#key-chroot).

## Subcommand `hooks`

Hooks can also be invoked separately via ali-rs `hooks` subcommand:

```shell
# Run 1 hook
ali-rs hooks "@hook-1 foo bar"

# Run 2 hooks
ali-rs hooks "@hook-1 foo bar" "@hook-2 baz 'hello, world!'"

# Run 1 hook, in chroot to mountpoint /mnt
ali-rs hooks "@hook-1 foo bar" --mountpoint "/mnt"
```

The `hooks` subcommand can also read hooks from manifest files
with `--manifest` flags:

```shell
ali-rs hooks --manifest -f path/to/manifest.yaml
```

If `--manifest` flag is used, only hooks in manifest file is
executed, otherwise, only the hook CLI strings are used.

Normally, hooks are validated before being executed, and the
`hooks` subcommand also comes with `--dry-run` flag which will
only validates the hook but does not execute it.

```shell
# Validates "@hook-1 foo bar" with mountpoint /mnt
ali-rs hooks --dry-run "@hook-1 foo bar" --mountpoint "/mnt"

# Validates hooks in manifest file
ali-rs hooks --dry-run --manifest -f path/to/manifest.yaml
```

## Debug hooks

All hooks, by default, modifies some files on the system.
To avoid corrupting system files, ali-rs provide _debug_ hooks,
which is a stateless, idempotent, print-only version of each hook.

For example, `@uncomment-all` hook has debug mode `@uncomment-all-debug`
which instead of writing to output files,
simply prints `@uncomment-all` output to screen.

## Hooks in ALI manifest, execution stage, and output file locations

> See also: [ALI stages](https://github.com/soyart/ali/blob/master/ALI.md#ali-stages)

ALI provides 2 place where users can put arbitary command strings:
(1) under [manifest key `chroot`](https://github.com/soyart/ali/blob/master/ALI.md#key-chroot)
and (2) under [key `postinstall`](https://github.com/soyart/ali/blob/master/ALI.md#key-postinstall).

ali-rs leverages this existing infrastructure, and allows users
to put their ali-rs hooks in ALI manifests under these 2 keys.

Hooks defined under manifest key `chroot` will be executed with the
absolute root path changed from the root of the live system to the
mountpoint of the new system being installed. Hooks defined here will
be executed in `stage-chroot_user`.

Hooks defined under manifest key `postinstall` will instead be executed
in the running live as-is. Hooks defined here will be executed in
`stage-chroot_ali`.

> These running environments can also be specified when executing hooks
> with ali-rs subcommands `ali-rs hooks "@foo-hook bar baz"` via the
> flag `--mountpoint`.

Some hooks, e.g. `@quicknet` and `@mkinitcpio`, are to be only run
inside `chroot` due to their nature.

These `chroot`-only hooks can still be defined under key `postinstall`,
and the hook will be executed in `stage-postinstall` after `stage-chroot`,
although ali-rs will automatically passes to them the mountpoints so that
files are written to the correct path under the mountpoint.

## Hook manuals

### `@quicknet`

  Quick network setup (DHCP and DNS), based on [`systemd-networkd`
  configuration template](./src/hooks/constants.rs)

  Synopsis:

  ```
  @quicknet [dns <DNS_UPSTREAM>] <INTERFACE>

  @quicknet <INTERFACE> [dns <DNS_UPSTREAM>]
  ```

  Examples:

  -  Simple DHCP for ens3

      ```
      @quicknet ens3
      ```

  - Simple DHCP and DNS upstream 1.1.1.1 for ens3

      ```
      @quicknet dns 1.1.1.1 ens3
      ```

### `@uncomment` and `@uncomment-all`

  Uncomments certain pattern

  Synopsis:

  ```
  @uncomment <PATTERN> [marker <COMMENT_MARKER="#">] FILE
  ```

  Examples:

  - Uncomments a commented line starting with key `PORT` with default
    coment marker `#` in file `/etc/ssh/sshd_config`:

      ```
      @uncomment Port /etc/ssh/sshd_config
      ```

  - Uncomments a commented line starting with key `FOO` with custom
    comment market `--` in file `/etc/bar`

      ```
      @uncomment FOO marker '--' /etc/bar
      ```
  

### `@replace-token`

  Replaces tokens in text files

  Synopsis:

  ```
  @replace-token <TOKEN> <VALUE> <TEMPLATE> [OUTPUT]
  ```

  Note: `<TOKEN>` expands to `{{ <TOKEN> }}`

  Examples:

  - Replaces token `{{ PORT }}` with `3322` _in-place_ on file `/etc/ssh/sshd`

      ```
      "@replace-token PORT 3322 /etc/ssh/sshd",
      ```

  - Reads template from `https://example.com/template`, and
  replaces token `{{ foo }}` in the template with `bar`, writing output to `/some/file`

      ```
      @replace-token foo bar https://example.com/template /some/file
      ```

  - Reads template from `/some/template`, and replaces token `{{ linux_boot }}`
  with `loglevel=3 quiet root=/dev/archvg/archlv ro`, then write output to `/etc/default/grub`

      ```
      @replace-token "linux_boot" "loglevel=3 quiet root=/dev/archvg/archlv ro" /some/template /etc/default/grub
      ```

### `@mkinitcpio`

  Formats [`/etc/mkinitcpio.conf`](https://man.archlinux.org/man/mkinitcpio.8)
  entries `BINARIES` and `HOOKS`.

  Some presets are available for `HOOKS`, e.g. `lvm-on-luks` via key `boot_hook`,
  which will produces `HOOKS` string suitable for booting a root on LVM-on-LUKS.

  > Note: `hooks` and `boot_hook` are mutually exclusive.

  Synopsis:

  ```
  @mkinitcpio [boot_hook=<BOOT_HOOK>] [binaries='bin2 bin2'] [hooks='hook1 hook2']
  ```

  Examples:

  - Uses preset `lvm` for `HOOKS`, and add `btrfs` to `BINARIES`

    ```
    @mkinitcpio 'boot_hook=lvm' 'binaries=btrfs'
    ```
      
    Output:

    ```
    HOOKS=(base udev autodetect modconf kms keyboard keymap consolefont block lvm2 filesystems fsck)
    BINARIES=(btrfs)
    ```

  - Uses preset preset `lvm` for `HOOKS`, and add `btrfs`,
    and `foo` to `BINARIES`, only printing output

    ```
    @mkinitcpio-debug 'boot_hook=lvm' 'binaries=btrfs foo'
    ```
      
    Output:

    ```
    HOOKS=(base udev autodetect modconf kms keyboard keymap consolefont block lvm2 filesystems fsck)
    BINARIES=(btrfs foo)
    ```

    Available `boot_hook` presets:

    - `lvm` for booting to rootfs on LVM

    - `luks` for booting to rootfs on LUKS

    - `lvm-on-luks` for booting to rootfs on LVM-on-LUKS

    - `luks-on-lvm` for booting to rootfs on LUKS-on-LVM

### `@download`

  Download a file from remote resource

  Synopsis:

  ```
  @download <URL> <OUTFILE>
  ```

  Examples:

  - Download using HTTPS to `/tmp/foo`

    ```
    @download https://example.com/foo /tmp/foo
    ```
    
  - Download using SCP from host `bar` to `/tmp/foo`, where `bar` is a configured
    host in `ssh.conf`.

    ```
    @download scp://bar:~/some/path /tmp/foo
    ```
