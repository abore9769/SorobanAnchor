# Credential Lifecycle Quick Reference

This page indexes the `anchorkit credentials` subcommands that manage the local
encrypted keystore. For how the keystore is used and how to avoid exposing
secrets, see [handling CLI secrets safely](ONBOARDING.md#handling-cli-secrets-safely).
For protecting secret files at rest, see
[secret file encryption](secret-file-encryption.md).

The keystore is a single file, `~/.anchorkit/credentials.json`. The home
directory is taken from `USERPROFILE`, then `HOME`. Each entry is the
credential's secret encrypted with AES-256-GCM under a key derived from the
keystore password with Argon2id. The password is entered at a hidden prompt.
No subcommand accepts it as an argument or environment variable.

## Operations

| Stage | Command | Prompts for | With `--no-interactive` | Effect |
|-------|---------|-------------|-------------------------|--------|
| Store | `anchorkit credentials add --name <NAME>` | Secret value, keystore password, confirmation | Refused (exit 1) | Writes the entry. **Replaces an existing entry with the same name without asking.** |
| List | `anchorkit credentials list` | Nothing | Runs | Prints stored names only, never values. Read-only. |
| Use | `--credential-name <NAME>` on `register`, `attest`, `quote`, `revoke` | Keystore password | Refused (exit 1) | Decrypts the entry to sign the operation. Read-only. Ignored when a higher-priority key source is set ([precedence](ONBOARDING.md#cli-flag-and-environment-variable-precedence)). |
| Read | `anchorkit credentials get --name <NAME>` | Keystore password | Refused (exit 1) | **Prints the decrypted secret to standard output.** Read-only. |
| Rotate | `anchorkit credentials rotate` | Current password, new password, confirmation | Refused (exit 1) | **Rewrites every entry** under the new keystore password. |
| Remove | `anchorkit credentials remove --name <NAME>` | Nothing | Runs | **Permanently deletes the entry.** There is no confirmation prompt and no undo. |

`--no-interactive` can also be enabled with the `ANCHORKIT_NO_INTERACTIVE`
environment variable.

## Destructive operations

- **`remove`** deletes the encrypted entry from the keystore file immediately.
  It does not prompt, and it runs in non-interactive scripts. Check the name
  with `credentials list` before running it.
- **`add`** with a name that already exists replaces the stored entry. The
  previous secret is not kept.
- **`rotate`** changes the keystore password only. It decrypts every entry with
  the current password first; if any entry fails to decrypt, nothing is
  changed. Entries stored under different passwords cannot be rotated
  together. After rotation the old password no longer decrypts any entry. The
  new password must differ from the current one.

Rotating the keystore password does not rotate the Stellar key stored in it.
To replace a key that may be exposed, see
[key rotation scope](key-rotation-scope.md).

The CLI does not report a failed write to the keystore file. After `add`,
`remove`, or `rotate`, run `anchorkit credentials list` to confirm the result.
If the keystore file cannot be read or parsed, the CLI treats it as empty, and
a following `add` replaces the file with only the new entry.

## Keep key material out of arguments

`credentials add` accepts an optional `--value`. Do not use it: the secret
would be saved in shell history and visible in process listings. Omit it and
enter the secret at the hidden prompt:

```bash
anchorkit credentials add --name anchor-admin
```

Run `credentials get` only where its output is not logged, recorded, or piped
to a file.

## Backup and recovery

The CLI has no backup, export, or password-recovery command. A forgotten
keystore password cannot be recovered by AnchorKit, and entries encrypted
under it cannot be decrypted. A removed or overwritten entry can only be
restored from a copy of the keystore file you made beforehand, together with
the password that was current when the copy was made.

The CLI creates the keystore file with your account's default permissions.
Restrict access to it, and protect any copy as you would the secret itself.
For encrypting files and backups at rest, see
[secret file encryption](secret-file-encryption.md). Keep the original source
of each secret under your own control; do not treat the keystore as the only
copy.

## Related references

- [Handling CLI secrets safely](ONBOARDING.md#handling-cli-secrets-safely)
- [Secret file encryption](secret-file-encryption.md)
- [Key rotation scope](key-rotation-scope.md)
- [Credential management example](../examples/credential_management.sh)
