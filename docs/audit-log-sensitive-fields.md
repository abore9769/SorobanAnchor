# Audit Log and Provenance Field Sensitivity

Audit and provenance records help with incident response. They must not become
a store of raw user data or credentials. This guide lists the fields each record
type serializes, what this codebase writes into them, and what must stay out.

All examples here are synthetic. Do not paste production records into this
guide, into issues, or into pull requests.

## Record types covered

| Record | Source | Where it ends up |
|--------|--------|------------------|
| `AdminConfigChangeEvent` | [`src/admin_audit_log.rs`](../src/admin_audit_log.rs) | Contract storage, and the full record is published as an `("admin", "audit", entry_id)` contract event |
| `AuditLog` / `OperationContext` | [`src/contract.rs`](../src/contract.rs) | Contract storage; returned by `get_audit_log*` and by `export_audit_log_batch` as hex-encoded XDR |
| `AuditLogEvent` | [`src/contract.rs`](../src/contract.rs) | `("audit", "logged", log_id)` and `("audit", "pruned")` contract events |
| `ProvenanceRecord` | [`src/request_provenance.rs`](../src/request_provenance.rs) | Off-chain, and propagated to other services as `X-Request-*` HTTP headers by `to_headers` |
| `ArtifactProvenance` | [`src/artifact_provenance.rs`](../src/artifact_provenance.rs) | Off-chain build provenance; see [artifact provenance](artifact-provenance.md) |

Contract storage and contract events can be read by anyone with access to the
network. The read and export helpers do not check the caller (see
[admin audit log](admin-audit-log.md#retention-and-export-limits)). Pruning or
clearing removes entries from current contract storage only. It does not remove
events that were already emitted, or ledger history that network participants
and archives may keep. Treat every on-chain audit field as public and permanent.

## Admin configuration records

`AdminConfigChangeEvent` fields and what the contract writes into them:

| Field | Written by this codebase | Category |
|-------|--------------------------|----------|
| `entry_id` | Sequential counter | Operational |
| `admin` | The authorizing account: the primary admin, a role holder (`caller`/`operator`), or the attestor for `set_endpoint` and `rate_limit_bypass` | Pseudonymous account address |
| `change_type` | A fixed token, such as `grant_role`, `register_attestor`, `set_endpoint`, `approve_kyc`, `blacklist_anchor`, `set_rate_limit_config`, `upgrade` | Operational |
| `target` | An account address (grantee, attestor, anchor, or KYC subject) or a fixed label such as `contract`, `quotes`, `cache`, `rate_limiter`, `service_snapshot`, `bypassed` | Pseudonymous address or label |
| `old_value`, `new_value` | State tokens only: role and capability names, `registered`/`revoked`, `enabled`/`disabled`, `Pending`/`Approved`/`Rejected`/`Reopened`, `updated`, `v1`/`v2 (<count>)`. For `set_endpoint` the contract records `updated`, not the endpoint URL. | Operational |
| `timestamp` | Ledger timestamp | Operational |
| `status` | `success` from contract writers; callers of `log_change_with_status` may pass `failed` | Operational |
| `error_message` | Empty from contract writers; supplied by callers of `log_change_with_status` | Free text (review required) |

Points to note:

- For `approve_kyc`, `reject_kyc` and `reopen_kyc`, `target` is the KYC
  subject's account address. On its own it is pseudonymous. Combined with
  off-chain KYC data it identifies a person, so never add names, document
  numbers, or other KYC field values next to it.
- `AdminAuditLog::log_change` and `log_change_with_status` accept arbitrary
  strings for `target`, `old_value`, `new_value`, and `error_message`. Code
  that calls them directly must apply the rules in
  [what must stay out](#what-must-stay-out). The URL examples in
  [admin audit log](admin-audit-log.md) are illustrative; prefer a state token
  such as `updated` over a raw URL. If a URL is genuinely needed, remove
  userinfo, query strings, and fragments first.

## Session operation records

`AuditLog` holds `log_id`, `session_id`, `actor`, and an `OperationContext`:

| Field | Written by this codebase | Category |
|-------|--------------------------|----------|
| `log_id`, `session_id`, `operation_index` | Counters | Operational |
| `actor` | The session's issuer or operator account. Stored in the record; not included in `AuditLogEvent`. | Pseudonymous account address |
| `operation_type` | `attest`, `register`, or `revoke` in stored records; `auto_prune` appears only in the `("audit", "pruned")` event | Operational |
| `timestamp` | Ledger timestamp | Operational |
| `status` | `success` | Operational |
| `result_data` | The attestation ID for stored `attest` records, otherwise `0`; the number of pruned records in the `auto_prune` event | Operational |

Session records carry no request payload, payload hash, KYC data, or
credential. Keep it that way when adding operation types: record an ID or a
count in `result_data`, not the content of the operation.

## Request provenance records

`ProvenanceRecord` fields: `request_id`, `parent_id`, `ancestors`,
`origin_service`, `operation`, `metadata`, and `created_at`.

- `to_headers` sends `request_id`, `parent_id`, depth, `origin_service`,
  `operation`, and `metadata` to the next service as `X-Request-*` headers.
  Anything placed in these fields leaves the current process.
- `metadata` is free text of up to `MAX_METADATA_LEN` (1024) bytes. The length
  check is its only validation. Put opaque operational context there, such as
  a transaction ID; never a token, secret, or personal data.
- `root` and `child` derive `request_id` from a SHA-256 digest of the service,
  operation, and timestamp. `root_with_id` and `child_with_id` accept a
  caller-supplied ID (at most `MAX_ID_LEN`, 128 bytes, no control characters).
  Never use an email address, account number, or other user identifier as that
  ID.

## Artifact provenance records

`ArtifactProvenance` fields: `provenance_id`, `artifact_name`, `content_hash`,
`source_revision`, `source_repository`, `build_command`, `build_env`,
`recorded_at`, and `signature`.

- `source_repository`: record the public repository URL. Remove any embedded
  access token or userinfo (for example `https://<token>@host/...`).
- `build_command`: record the command, not secrets passed to it. Never include
  `--secret-key`, tokens, or passwords.
- `build_env`: record an allowlist of keys needed to reproduce the build, such
  as the `rustc` version, target architecture, `SOURCE_DATE_EPOCH`, and `CI`.
  Never copy the whole process environment; it may contain
  `ANCHOR_ADMIN_SECRET`, CI tokens, or proxy credentials.
- `signature`: a signature over `content_hash`. It is meant to be shared. The
  signing key itself is never part of the record.

## What must stay out

These must never appear in any audit or provenance field, whether stored,
emitted as an event, exported, or propagated as a header:

- **Credentials:** Stellar secret keys (`S...`), seed phrases, keystore
  passwords, SEP-10 JWTs, bearer and API tokens, `Authorization` or
  `Proxy-Authorization` values, proxy passwords, and webhook signing or HMAC
  keys.
- **Unnecessary personal data:** names, emails, phone numbers, postal
  addresses, ID or document numbers, other KYC/SEP-12 field values, and memos
  that identify a person.
- **Raw payloads and dumps:** full request or response bodies, decoded
  transaction payloads, full environment dumps, and error messages that quote
  raw input.
- **Credential-bearing URLs:** URLs with userinfo, or with tokens or
  customer identifiers in the path, query string, or fragment.

When an event must show that a sensitive value was involved, record a
non-sensitive fact about it instead: whether it was present, its kind, a state
token, or an existing operational ID. The same rule applies to logs; see
[reviewing new log fields](structured-logging.md#reviewing-new-log-fields).

## What the tests cover

[`tests/safe_logging_tests.rs`](../tests/safe_logging_tests.rs) checks that the
CLI's `SecretKey` wrapper and the credential types in `src/http_client.rs`
redact values in `Debug` and `Display` output. It does not inspect audit or
provenance record contents. Keeping credentials and personal data out of those
records depends on code review of every call site that writes them.

This guide does not set or promise a retention period. For the storage limits
that apply, see
[retention and export limits](admin-audit-log.md#retention-and-export-limits).

## Related references

- [Admin audit log](admin-audit-log.md)
- [Artifact provenance inspection](artifact-provenance.md)
- [Structured logging](structured-logging.md)
- [Governance and security](governance-and-security.md)
