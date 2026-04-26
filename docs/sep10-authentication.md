# SEP-10 Authentication

[SEP-10](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0010.md)
defines a standard for authenticating Stellar accounts with anchors using
challenge transactions and JWTs. AnchorKit's `sep10_jwt` module verifies the
JWT that an anchor issues after a successful challenge-response exchange.

## How SEP-10 Works

1. Client requests a challenge transaction from the anchor's `/auth` endpoint.
2. Anchor returns a Stellar transaction that the client must sign.
3. Client signs the transaction and submits it back to `/auth`.
4. Anchor verifies the signature and returns a JWT.
5. Client includes the JWT in subsequent API calls.
6. AnchorKit verifies the JWT on-chain using the anchor's Ed25519 public key.

## Storing the Anchor's Verifying Key

Before verifying any tokens, the admin must store the anchor's 32-byte Ed25519
public key on-chain:

```rust,no_run
// In your contract deployment / setup script:
contract.set_sep10_jwt_verifying_key(env, anchor_address, public_key_bytes);
```

## Verifying a Token On-Chain

```rust,no_run
// The contract panics with ErrorCode::InvalidSep10Token on failure.
contract.verify_sep10_token(env, jwt_token, anchor_address);
```

To also verify the `sub` claim matches a specific subject:

```rust,no_run
contract.verify_sep10_token_for_subject(env, jwt_token, anchor_address, subject_address);
```

## Verifying a Token Off-Chain (in tests / service layer)

```rust,no_run
use anchorkit::sep10_jwt::verify_sep10_jwt;
use soroban_sdk::{Bytes, Env, String};

let env = Env::default();
let anchor_public_key = Bytes::from_slice(&env, &[/* 32 bytes */]);
let token = String::from_str(&env, "header.payload.signature");

match verify_sep10_jwt(&env, &token, &anchor_public_key, None) {
    Ok(()) => println!("Token is valid"),
    Err(()) => println!("Token is invalid or expired"),
}
```

## JWT Validation Rules

AnchorKit enforces the following checks:

| Check | Description |
|-------|-------------|
| Length | Token must be ≤ `MAX_JWT_LEN` (default 2048, configurable up to 16384) |
| Format | Exactly two `.` separators (three parts) |
| Algorithm | Header must contain `"EdDSA"` |
| Signature | 64-byte Ed25519 signature must verify against the stored public key |
| `exp` | Must be present and in the future |
| `nbf` | If present, must not be in the future |
| `jti` | If present, must not have been seen before (replay protection) |
| `sub` | Must be present; compared to `expected_sub` when provided |

## Configuring the Maximum JWT Length

The default maximum is 2048 characters. Admins can increase it up to 16384:

```rust,no_run
// Allow tokens up to 8192 characters.
contract.set_jwt_max_len(env, 8192);

// Read the current setting.
let max_len = contract.get_jwt_max_len(env);
```

## Registering an Attestor with SEP-10

Attestor registration requires a valid SEP-10 JWT to prove identity:

```rust,no_run
contract.register_attestor(
    env,
    attestor_address,
    sep10_jwt_token,
    sep10_issuer_address,
);
```

The contract verifies that the JWT's `sub` claim matches `attestor_address`
before registering.

## Error Codes

| Code | Meaning |
|------|---------|
| `ErrorCode::InvalidSep10Token` | Token is missing, expired, has an invalid signature, or `sub` mismatch |
| `ErrorCode::AttestorNotRegistered` | Attestor not found when looking up the verifying key |
