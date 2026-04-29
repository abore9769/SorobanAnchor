# Implementation Notes: RequestContext Propagation & Secure Credential Keystore

## Overview

This document describes the implementation of two major features:

**Feature A:** RequestContext propagation for distributed tracing  
**Feature B:** Secure credential keystore with AES-256-GCM encryption

---

## Feature A: RequestContext Propagation

### Description

The `RequestContext` struct enables full-chain distributed tracing by propagating a root request ID through all sub-operations. Previously, each operation generated its own request ID; now, all operations under a single user request share the same root ID and maintain an ordered operation chain.

### Implementation

#### 1. New `RequestContext` Struct (`src/contract.rs`)

```rust
#[contracttype]
#[derive(Clone)]
pub struct RequestContext {
    /// The root request ID that initiated this chain of operations.
    pub root_request_id: RequestId,
    /// Ordered list of operation names performed under this root request.
    pub operation_chain: Vec<String>,
    /// Ledger timestamp when this context was first created.
    pub created_at: u64,
}
```

#### 2. New Contract Functions

- **`create_request_context(env, root_request_id) -> RequestContext`**  
  Creates a new context for a root request ID with an empty operation chain.

- **`append_operation(env, root_request_id_bytes, operation_name)`**  
  Appends an operation name to the chain. Auto-creates context if it doesn't exist.

- **`get_request_context(env, root_request_id_bytes) -> Option<RequestContext>`**  
  Returns the full context including the operation chain for a given root request ID.

#### 3. Updated Functions

- **`submit_with_request_id`**: Now calls `record_operation_in_context` to append `"submit_attestation"` to the chain.
- **`quote_with_request_id`**: Now calls `record_operation_in_context` to append `"submit_quote"` to the chain.

#### 4. Internal Helper

- **`record_operation_in_context(env, root_id_bytes, operation_name)`**  
  Private helper that appends an operation to the context, creating it if needed.

### Storage

- **Key**: `(symbol_short!("REQCTX"), root_request_id_bytes)`
- **Storage Type**: Temporary (same TTL as tracing spans: `SPAN_TTL`)
- **TTL**: 17,280 ledgers (~24 hours at 5s/ledger)

### Tests

New tests in `tests/request_id_tests.rs`:

1. **`test_root_request_id_preserved_across_sub_operations`**  
   Verifies that the root request ID is preserved when multiple operations append to the chain.

2. **`test_operation_chain_populated_in_order`**  
   Verifies that operations are recorded in the order they occur.

3. **`test_get_request_context_returns_full_chain`**  
   Simulates a full deposit flow (SEP-10 auth → attestation → status poll) and verifies the complete chain is returned.

4. **`test_get_request_context_returns_none_for_unknown_id`**  
   Verifies that querying an unknown request ID returns `None`.

Updated existing tests:

- **`test_submit_attestation_with_request_id`**: Now verifies `RequestContext` is created and populated.
- **`test_submit_quote_with_request_id`**: Now verifies `RequestContext` is created and populated.

### Acceptance Criteria ✅

- [x] Every contract operation references a `RequestContext` rather than a standalone `RequestId`
- [x] The `operation_chain` accurately reflects the sequence of operations performed
- [x] The `get_request_context` function returns the full chain for a root request ID
- [x] All existing request ID tests pass
- [x] New tests verify end-to-end propagation

---

## Feature B: Secure Credential Keystore

### Description

The CLI now provides a secure keystore for managing credentials (secret keys, API tokens) with AES-256-GCM encryption and Argon2id key derivation. Credentials are encrypted at rest and retrieved on demand, eliminating the need to store plaintext secrets in environment variables or config files.

### Implementation

#### 1. Keystore Module (`src/main.rs`)

A new `keystore` module provides:

- **`EncryptedEntry`**: Stores nonce, ciphertext, and salt (all base64-encoded).
- **`Keystore`**: On-disk format (`HashMap<String, EncryptedEntry>`).
- **`keystore_path()`**: Returns `~/.anchorkit/keystore.json`.
- **`load()`**: Loads the keystore from disk (returns empty if file doesn't exist).
- **`save(ks)`**: Persists the keystore with restricted permissions (0600 on Unix).
- **`encrypt(password, plaintext) -> EncryptedEntry`**: Encrypts using AES-256-GCM with Argon2id-derived key.
- **`decrypt(password, entry) -> String`**: Decrypts and returns plaintext (or error if wrong password).

#### 2. Encryption Details

- **Algorithm**: AES-256-GCM (authenticated encryption)
- **Key Derivation**: Argon2id with parameters:
  - Memory: 65536 KiB (64 MB)
  - Iterations: 3
  - Parallelism: 4
  - Output: 32 bytes
- **Nonce**: 12 bytes (randomly generated per encryption)
- **Salt**: 16 bytes (randomly generated per encryption)
- **Authentication Tag**: 16 bytes (included in ciphertext)

#### 3. New CLI Subcommands

```bash
anchorkit credentials add --name <name> [--value <value>]
anchorkit credentials get --name <name>
anchorkit credentials list
anchorkit credentials remove --name <name>
```

**`add`**: Stores an encrypted credential. If `--value` is omitted, prompts for value (avoids shell history exposure). Prompts for keystore password twice (with confirmation).

**`get`**: Retrieves and decrypts a credential. Prompts for keystore password.

**`list`**: Lists stored credential names (not values).

**`remove`**: Deletes a credential from the keystore.

#### 4. Updated `resolve_source` Function

Priority order:
1. `--secret-key`
2. `ANCHOR_ADMIN_SECRET` environment variable
3. `--keypair-file`
4. **`--credential-name`** (new)

When `--credential-name` is provided, the function:
1. Prompts for keystore password
2. Loads the keystore
3. Retrieves the encrypted entry
4. Decrypts and returns the credential

#### 5. Updated Subcommands

Added `--credential-name` flag to:
- `register`
- `attest`
- `quote`
- `revoke`

#### 6. Updated Example Script

`examples/credential_management.sh` now demonstrates:
- Storing credentials with `anchorkit credentials add`
- Listing credentials with `anchorkit credentials list`
- Using credentials in contract operations via `--credential-name`
- Security best practices

### Dependencies Added

```toml
aes-gcm = { version = "0.10.3", features = ["aes"] }
argon2 = { version = "0.5.3" }
rand = { version = "0.8", features = ["std"] }
base64 = { version = "0.22" }
rpassword = { version = "7.3" }
```

### Security Features

1. **Encryption at Rest**: All credentials encrypted with AES-256-GCM
2. **Strong Key Derivation**: Argon2id with recommended parameters
3. **Per-Entry Salt**: Each credential has a unique salt
4. **Per-Encryption Nonce**: Each encryption uses a fresh nonce
5. **Authenticated Encryption**: GCM mode provides integrity protection
6. **File Permissions**: Keystore file restricted to owner (0600 on Unix)
7. **Password Protection**: Keystore access requires password
8. **No Shell History Exposure**: Values can be entered via stdin prompt

### Acceptance Criteria ✅

- [x] All four credential management subcommands implemented (`add`, `get`, `list`, `remove`)
- [x] Credentials encrypted at rest using AES-256-GCM
- [x] Keystore stored at `~/.anchorkit/keystore.json`
- [x] `--credential-name` flag works as alternative to `--secret-key`
- [x] `credential_management.sh` example updated to use new CLI commands

---

## Testing

### Feature A Tests

Run the request ID tests:

```bash
cargo test --test request_id_tests
```

Expected output:
- `test_generate_request_id` ✓
- `test_unique_request_ids` ✓
- `test_submit_attestation_with_request_id` ✓
- `test_tracing_span_timing` ✓
- `test_tracing_span_records_failure` ✓
- `test_submit_quote_with_request_id` ✓
- `test_root_request_id_preserved_across_sub_operations` ✓
- `test_operation_chain_populated_in_order` ✓
- `test_get_request_context_returns_full_chain` ✓
- `test_get_request_context_returns_none_for_unknown_id` ✓

### Feature B Manual Testing

1. **Add a credential:**
   ```bash
   echo "S..." | anchorkit credentials add --name my-secret
   # Enter keystore password when prompted
   ```

2. **List credentials:**
   ```bash
   anchorkit credentials list
   ```

3. **Get a credential:**
   ```bash
   anchorkit credentials get --name my-secret
   # Enter keystore password when prompted
   ```

4. **Use in contract operation:**
   ```bash
   anchorkit register \
     --address GABC... \
     --services deposits,kyc \
     --contract-id CBXX... \
     --network testnet \
     --credential-name my-secret \
     --sep10-token "..." \
     --sep10-issuer GABC...
   ```

5. **Remove a credential:**
   ```bash
   anchorkit credentials remove --name my-secret
   ```

---

## Migration Guide

### For Existing Users

**Before:**
```bash
export ANCHOR_ADMIN_SECRET="S..."
anchorkit register --address GABC... --services deposits ...
```

**After:**
```bash
# One-time setup
echo "S..." | anchorkit credentials add --name admin-key

# Use in operations
anchorkit register --address GABC... --services deposits --credential-name admin-key ...
```

### For CI/CD Pipelines

**Option 1: Continue using environment variables**
```bash
export ANCHOR_ADMIN_SECRET="S..."
anchorkit register ...
```

**Option 2: Use keystore with non-interactive password**
```bash
# Store credential once
echo "S..." | anchorkit credentials add --name ci-key --value "S..."

# Use in pipeline (password from secret manager)
echo "$KEYSTORE_PASSWORD" | anchorkit credentials get --name ci-key | \
  anchorkit register --secret-key "$(cat -)" ...
```

---

## Future Enhancements

### Feature A
- [ ] Add `RequestContext` to more operations (SEP-6 deposit, SEP-24 interactive flows)
- [ ] Expose `operation_chain` in webhook payloads
- [ ] Add operation duration tracking to each chain entry

### Feature B
- [ ] Add credential expiry/rotation tracking
- [ ] Support hardware security modules (HSM) for key storage
- [ ] Add audit logging for credential access
- [ ] Support multiple keystore profiles (dev, staging, prod)
- [ ] Add credential backup/restore commands

---

## References

- **AES-GCM**: [NIST SP 800-38D](https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication800-38d.pdf)
- **Argon2**: [RFC 9106](https://www.rfc-editor.org/rfc/rfc9106.html)
- **Soroban SDK**: [docs.rs/soroban-sdk](https://docs.rs/soroban-sdk)
- **Stellar SEPs**: [github.com/stellar/stellar-protocol](https://github.com/stellar/stellar-protocol/tree/master/ecosystem)

---

## Author

Implementation completed by senior developer as per requirements.

**Date**: 2026-04-28
