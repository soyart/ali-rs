# ali-rs hooks

Hooks are special commands starting with `@<HOOK_NAME>`.

Users register hooks inside manifest keys [`chroot`](https://github.com/soyart/ali/blob/master/ALI.md#key-chroot)
and [`postinstall`](https://github.com/soyart/ali/blob/master/ALI.md#key-chroot).

Hooks can also be invoked separately via ali-rs `hooks` subcommand:

```shell
# Run 1 hook
ali-rs hooks --hooks "@hook-1 foo bar"

# Run 2 hooks
ali-rs hooks --hooks "@hook-1 foo bar" "@hook-2 baz 'hello, world!'"

# Run 1 hook, in chroot to mountpoint /mnt
ali-rs hooks --hooks "@hook-1 foo bar" --mountpoint "/mnt"
```

## Print hooks

All hooks, by default, modifies some files on the system.
To avoid corrupting system files, ali-rs provide _print_ hooks,
which is a stateless print-only version of each hook.

For example, `@uncomment-all` hook has print-only version
`@uncomment-all-print` which instead of writing to output files,
simply prints `@uncomment-all` output to screen.

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
