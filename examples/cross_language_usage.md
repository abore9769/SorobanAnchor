# AnchorKit Cross-Language Usage Guide

## Scope and support status

Read this first. This guide does **not** describe a multi-language SDK. AnchorKit ships two interfaces, both in this repository and both pre-1.0 (`version = "0.1.0"` in `Cargo.toml`):

- a **Rust library** (`anchorkit` crate: on-chain WASM contract + off-chain native helpers), and
- a **CLI binary** (`anchorkit`, built from `src/main.rs`).

Everything else here shows how other languages can reach those two interfaces. The table gives the status of each section:

| Section | What it is | Status |
|---------|------------|--------|
| [1. Rust](#1-rust-native--off-chain) | Calls into the in-repo crate | **Supported, unstable API.** Type and function names match `src/`. The public API may change before v1.0 (see `docs/ROADMAP.md`). Snippets are fragments that belong inside a function returning a compatible `Result`. |
| [2. Shell / CLI](#2-shell--cli) | The in-repo `anchorkit` binary | **Supported.** Flags match `src/main.rs`. Commands that touch a contract need the `stellar` CLI in `PATH` (the binary shells out to it), a deployed contract, network access, and a signing key. |
| [3. Python](#3-python-via-subprocess) | A subprocess wrapper around the CLI | **Pattern only.** No Python package exists. The code runs if the CLI prerequisites in section 2 are met. |
| [4. JavaScript / TypeScript](#4-javascript--typescript-illustrative-only) | Direct contract call via the third-party Stellar JS SDK | **Illustrative only.** There is no maintained JS/TS SDK and no generated contract bindings in this repository. The snippet does not run as written. |

Also note:

- `ui/` (`@anchorkit/ui-components`) holds React presentation components. It is not a contract client or SDK binding.
- If you need typed TypeScript bindings, generate them yourself from the built contract WASM with `stellar contract bindings typescript`. This project does not maintain, version, or test such bindings.
- Placeholders such as `GATTESTOR_ADDRESS`, `GUSER_ADDRESS` and `$ANCHOR_CONTRACT_ID` must be replaced with your own values.

---

## 1. Rust (native / off-chain)

Add to `Cargo.toml`, pointing at your local checkout of this repository. This repository does not document a crates.io release.

```toml
[dependencies]
anchorkit = { path = "../SorobanAnchor" }   # path to your checkout, or a git dependency
```

### SEP-6 deposit normalization

`RawDepositResponse` does not implement `Default`, so set every field:

```rust
use anchorkit::sep6::{initiate_deposit, RawDepositResponse};

let raw = RawDepositResponse {
    transaction_id: "txn-001".into(),
    how: "Send to bank account 1234".into(),
    extra_info: None,
    min_amount: Some(10),
    max_amount: Some(10_000),
    fee_fixed: Some(1),
    status: Some("pending_external".into()),
    clawback_enabled: None,
    stellar_memo: None,
    stellar_memo_type: None,
    asset_code: Some("USDC".into()),
};
let deposit = initiate_deposit(raw)?;
println!("tx: {}, status: {:?}", deposit.transaction_id, deposit.status);
```

### SEP-24 interactive deposit

```rust
use anchorkit::sep24::{initiate_interactive_deposit, RawInteractiveDepositResponse};

let raw = RawInteractiveDepositResponse {
    url: "https://anchor.example.com/interactive/deposit".into(),
    id: "txn-002".into(),
};
let resp = initiate_interactive_deposit(raw)?;
// Redirect the user's browser to resp.url
println!("Redirect to: {}", resp.url);
```

### Retry with exponential backoff

`fetch_transaction_status` and `TransientError` below stand in for your own code.

```rust
use anchorkit::retry::{retry_with_backoff, MockJitterSource, RetryConfig};

let config = RetryConfig::default(); // 3 attempts, 100 ms base delay, 5 s cap
// Any `JitterSource` works. MockJitterSource is deterministic; implement the
// trait yourself (or use LedgerJitterSource) for real jitter.
let mut jitter = MockJitterSource::new(vec![0]);
let result = retry_with_backoff(
    &config,
    |attempt| fetch_transaction_status("txn-001", attempt),
    |err| matches!(err, TransientError),
    |delay_ms| std::thread::sleep(std::time::Duration::from_millis(delay_ms)),
    &mut jitter,
)?;
```

### Domain validation

```rust
use anchorkit::validate_anchor_domain;

assert!(validate_anchor_domain("https://anchor.example.com").is_ok());
assert!(validate_anchor_domain("http://anchor.example.com").is_err()); // HTTP not allowed
```

---

## 2. Shell / CLI

The `anchorkit` binary exposes the major workflows. Build it once:

```bash
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

Signing: commands that send a transaction take their key from `--ephemeral-token`, `--secret-key`, `ANCHOR_ADMIN_SECRET`, `--keypair-file`, or `--credential-name` (checked in that order). See `examples/credential_management.sh`. `--contract-id` falls back to `ANCHOR_CONTRACT_ID` when omitted.

### Registration

```bash
anchorkit register \
  --address GATTESTOR_ADDRESS \
  --services deposits,withdrawals,kyc \
  --contract-id "$ANCHOR_CONTRACT_ID" \
  --network testnet \
  --credential-name anchor-admin-key \
  --sep10-token "$SEP10_JWT" \
  --sep10-issuer "$SEP10_ISSUER"
```

### Attestation submission

```bash
# Linux: sha256sum. macOS: replace with `shasum -a 256`.
PAYLOAD_HASH=$(echo -n "deposit:usdc:500:$(date +%s)" | sha256sum | awk '{print $1}')

anchorkit attest \
  --issuer GATTESTOR_ADDRESS \
  --subject GUSER_ADDRESS \
  --payload-hash "$PAYLOAD_HASH" \
  --contract-id "$ANCHOR_CONTRACT_ID" \
  --network testnet \
  --credential-name kyc-attestor-key
```

### Quote retrieval

```bash
anchorkit quote \
  --from USD \
  --to USDC \
  --amount 1000 \
  --contract-id "$ANCHOR_CONTRACT_ID" \
  --network testnet \
  --credential-name anchor-admin-key
```

### Contract deployment

```bash
# Testnet (--source is a stellar CLI identity alias)
anchorkit deploy \
  --network testnet \
  --source anchor-admin \
  --admin GADMIN_ADDRESS

# Mainnet upgrade. Signs with --secret-key, --keypair-file, or ANCHOR_ADMIN_SECRET
# (--credential-name is not used for upgrades).
anchorkit deploy \
  --upgrade \
  --contract-id "$ANCHOR_CONTRACT_ID" \
  --network mainnet \
  --keypair-file ./admin-keypair.json
```

### Environment check

```bash
anchorkit doctor
# Use --fix to auto-resolve common issues
anchorkit doctor --fix
```

---

## 3. Python (via subprocess)

Any language that can run a subprocess can drive the CLI. This needs no Python package, only the CLI prerequisites from section 2:

```python
import subprocess, os

def attest(issuer: str, subject: str, payload_hash: str) -> str:
    result = subprocess.run(
        [
            "anchorkit", "attest",
            "--issuer", issuer,
            "--subject", subject,
            "--payload-hash", payload_hash,
            "--contract-id", os.environ["ANCHOR_CONTRACT_ID"],
            "--network", os.environ.get("STELLAR_NETWORK", "testnet"),
            "--credential-name", "kyc-attestor-key",
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    # The CLI prints "Attestation ID: <id>" on stdout
    return result.stdout.strip().removeprefix("Attestation ID: ")

attestation_id = attest("GATTESTOR_ADDRESS", "GUSER_ADDRESS", "abc123deadbeef...")
print(f"Attestation ID: {attestation_id}")
```

`--credential-name` prompts for the keystore password on the terminal. For unattended runs, set `ANCHOR_ADMIN_SECRET` in the child environment from your secret store and drop `--credential-name`.

---

## 4. JavaScript / TypeScript (illustrative only)

> **Not runnable as written.** This shows the general shape of calling the deployed contract directly over Soroban RPC with the third-party [`@stellar/stellar-sdk`](https://stellar.github.io/js-stellar-sdk/). That package is not a dependency of this repository. Its export names have changed between major versions, and the `submit_attestation` arguments must be encoded as Soroban `ScVal`s, which is omitted here. Check every call against the SDK version you install.

```typescript
import { Contract, SorobanRpc, TransactionBuilder, Networks, Keypair } from "@stellar/stellar-sdk";

const server = new SorobanRpc.Server("https://soroban-testnet.stellar.org");
const contract = new Contract(process.env.ANCHOR_CONTRACT_ID!);
const keypair = Keypair.fromSecret(process.env.ANCHOR_ADMIN_SECRET!);

// Build a submit_attestation transaction
const account = await server.getAccount(keypair.publicKey());
const tx = new TransactionBuilder(account, {
  fee: "100",
  networkPassphrase: Networks.TESTNET,
})
  .addOperation(
    contract.call(
      "submit_attestation",
      // issuer, subject, timestamp, payload_hash, signature
      // (encode as Soroban XDR ScVal types)
    )
  )
  .setTimeout(30)
  .build();

const prepared = await server.prepareTransaction(tx);
prepared.sign(keypair);
const result = await server.sendTransaction(prepared);
console.log("Attestation submitted:", result.hash);
```

The contract's entrypoints and argument types are defined in `src/contract.rs`.

---

## 5. Scenario quick-reference

The last column lists contract entrypoints in `src/contract.rs`, called from Rust through the generated `AnchorKitContractClient` (see `examples/cli_example.rs`).

| Scenario | Shell example | Contract entrypoint |
|----------|--------------|---------------------|
| Register attestor | `anchorkit register ...` | `register_attestor()` |
| Submit attestation | `anchorkit attest ...` | `submit_attestation()` |
| KYC submission | see `examples/kyc_workflow.sh` | `submit_kyc()` |
| KYC approval | — | `approve_kyc()` |
| Get quote | `anchorkit quote ...` | `route_transaction()` |
| SEP-6 deposit | — | — (off-chain: `anchorkit::sep6::initiate_deposit`) |
| SEP-24 interactive | — | — (off-chain: `anchorkit::sep24::initiate_interactive_deposit`) |
| Deploy contract | `anchorkit deploy ...` | — |
| Revoke attestor | `anchorkit revoke ...` | `revoke_attestor()` |

---

## Further reading

- `docs/error-codes.md` — full error code reference
- `docs/gas-and-storage-costs.md` — on-chain cost guide
- `docs/secret-file-encryption.md` — secret management
- `docs/multi-asset-routing-guide.md` — multi-anchor routing strategies
- `examples/kyc_workflow.sh` — end-to-end KYC lifecycle
- `examples/attestation_workflow.sh` — attestation submission and verification
- `examples/credential_management.sh` — signing-key sources and the encrypted keystore
