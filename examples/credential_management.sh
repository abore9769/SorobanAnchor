#!/bin/bash
# Example: Secure Credential Management with AnchorKit
# This script demonstrates how to use the anchorkit CLI to manage credentials securely
#
# Mode: Live (local keystore). Steps 1-2 write to and read from your local
#   keystore file. Step 3 attempts a contract call and is skipped if it fails.
#
# Prerequisites:
#   Binary/build: anchorkit binary in PATH
#                 (cargo build --release && export PATH="$PWD/target/release:$PATH")
#                 stellar CLI in PATH (only for Step 3, which invokes the contract)
#   Environment:  CONTRACT_ID (optional; defaults to a CBXXX... placeholder)
#                 NETWORK     (optional; default: testnet)
#                 An interactive terminal: `credentials add` prompts for the
#                 secret value and a keystore password, and it refuses to run
#                 under --no-interactive / ANCHORKIT_NO_INTERACTIVE.
#   Network:      None for Steps 1-2. Step 3 needs RPC access and a deployed
#                 contract; without them it prints "Skipped" and continues.
#
# Run:
#   bash examples/credential_management.sh
#
# ── Safety: every value in this script is an example ─────────────────────────
#
#   - Nothing in this file is a usable credential. CONTRACT_ID is a CBXXX...
#     placeholder, the G... addresses are public account IDs (never secrets),
#     and "dummy-token" is not a valid SEP-10 JWT.
#   - The script never contains a secret value. `credentials add` prompts for
#     it without echoing. Enter a testnet-only Stellar secret key (S...). The
#     stored value is used as a signing key by --credential-name, so an API
#     key or other token will not work here.
#   - Credential names are prefixed "example-" so you can find and remove
#     them afterwards (see Cleanup below).
#
# ── Supported signing-key sources ────────────────────────────────────────────
#
#   When a command needs a signing key, the CLI checks these sources in order
#   and uses the first one present:
#     1. --ephemeral-token / ANCHORKIT_EPHEMERAL_TOKEN  (one-time automation token)
#     2. --secret-key <S...>     (visible in shell history and `ps`; avoid)
#     3. ANCHOR_ADMIN_SECRET     (environment variable)
#     4. --keypair-file <path>   (JSON {"secret_key":"S..."} or plain text;
#                                 see docs/secret-file-encryption.md)
#     5. --credential-name <n>   (encrypted local keystore, demonstrated here;
#                                 needs an interactive password prompt)
#
#   Keystore details, as implemented in src/main.rs:
#     - File: ~/.anchorkit/credentials.json (JSON map of name -> ciphertext)
#     - Each value is encrypted with AES-256-GCM using a key derived from your
#       password with Argon2id (argon2 crate default parameters).
#     - The CLI does not set file permissions. Restrict them yourself:
#         chmod 600 ~/.anchorkit/credentials.json
#
# ── Do not commit keystore data ──────────────────────────────────────────────
#
#   The keystore lives in your home directory, outside the repository. Never
#   copy it, keypair files, or `credentials get` output into the repo, CI
#   logs, or issue reports. .gitignore matches credentials.json, .env*, *.key
#   and *.pem, but that is a backstop, not a guarantee.
#
# ── Cleanup ──────────────────────────────────────────────────────────────────
#
#   Remove the entries this script creates:
#     anchorkit credentials remove --name example-kyc-attestor-key
#     anchorkit credentials remove --name example-bank-attestor-key
#
#   Limits of deletion: `credentials remove` rewrites credentials.json without
#   the entry, and `rm` unlinks the file. Neither guarantees the old bytes are
#   erased from disk. SSD wear levelling, journaling or copy-on-write
#   filesystems, snapshots, and backups (e.g. Time Machine) may keep copies.
#   If a real secret key was ever stored or exposed, treat it as compromised:
#   rotate or revoke it on-chain (see docs/key-rotation-scope.md).
#   `anchorkit credentials rotate` only re-encrypts the keystore under a new
#   password. It does not change the underlying keys.

set -e

# Configuration
CONTRACT_ID="${CONTRACT_ID:-CBXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX}"
NETWORK="${NETWORK:-testnet}"
ADMIN_ACCOUNT="admin-account"

# Attestor addresses (public, safe to store)
KYC_PROVIDER="GBBD6A7KNZF5WNWQEPZP5DYJD2AYUTLXRB6VXJ4RCX4RTNPPQVNF3GQ"
BANK_INTEGRATION="GB7ZTQBJ7XXJQ6JDLHYQXQX3JQXJ3JQXJ3JQXJ3JQXJ3JQXJ3JQX"

echo "=== AnchorKit Secure Credential Management Example ==="
echo ""

# ============================================================
# Step 1: Store Credentials in Encrypted Keystore
# ============================================================
echo "Step 1: Storing credentials in encrypted keystore..."

# Store the KYC attestor signing key. The CLI prompts for the value without
# echoing it. Use a testnet-only Stellar secret key (S...).
echo "  Storing KYC attestor signing key..."
if anchorkit credentials add --name example-kyc-attestor-key; then
    echo "    Credential stored (value redacted)."
else
    echo "    (Skipped - credential may already exist)"
fi

# Store the bank integration attestor signing key
echo "  Storing bank integration attestor signing key..."
if anchorkit credentials add --name example-bank-attestor-key; then
    echo "    Credential stored (value redacted)."
else
    echo "    (Skipped - credential may already exist)"
fi

echo "  ✓ Credentials stored encrypted in ~/.anchorkit/credentials.json"
echo ""

# ============================================================
# Step 2: List Stored Credentials
# ============================================================
echo "Step 2: Listing stored credentials..."

anchorkit credentials list 2>/dev/null || echo "  (No credentials stored yet)"

echo ""

# ============================================================
# Step 3: Use Credentials in Contract Operations
# ============================================================
echo "Step 3: Using credentials in contract operations..."

# Example: Register attestor using stored credential
echo "  Registering KYC provider using stored credential..."
if anchorkit register \
    --address "$KYC_PROVIDER" \
    --services deposits,kyc \
    --contract-id "$CONTRACT_ID" \
    --network "$NETWORK" \
    --credential-name example-kyc-attestor-key \
    --sep10-token "dummy-token" \
    --sep10-issuer "$KYC_PROVIDER" \
    2>/dev/null; then
    echo "  ✓ Credential retrieved from keystore and used for signing"
else
    echo "    (Skipped - contract not deployed or credential not found)"
fi
echo ""

# ============================================================
# Step 4: Retrieve a Credential (for verification)
# ============================================================
echo "Step 4: Retrieving a credential (for verification)..."

echo "  Retrieving KYC attestor signing key..."
# Note: This prompts for the keystore password and prints the decrypted
# secret to stdout. Do not run it where output is logged or shared.
# anchorkit credentials get --name example-kyc-attestor-key

echo "  (Skipped in demo - would prompt for password and print the secret)"
echo ""

# ============================================================
# Step 5: Remove a Credential
# ============================================================
echo "Step 5: Removing a credential..."

echo "  Removing example credentials..."
# anchorkit credentials remove --name example-kyc-attestor-key
# anchorkit credentials remove --name example-bank-attestor-key

echo "  (Skipped in demo - run the commands above to clean up; see header)"
echo ""

# ============================================================
# Summary
# ============================================================
echo "=== Summary ==="
echo ""
echo "✓ Credentials stored in encrypted keystore (~/.anchorkit/credentials.json)"
echo "✓ Each entry uses AES-256-GCM with an Argon2id-derived key"
echo "✓ Credentials can be used via --credential-name flag"
echo "✓ Secret values are prompted for, not written in config files or this script"
echo ""
echo "Security Best Practices Applied:"
echo "  • Credentials encrypted at rest with AES-256-GCM"
echo "  • Key derivation using Argon2id (argon2 crate default parameters)"
echo "  • Password-protected keystore access"
echo "  • Secret values entered at a prompt, not passed as command-line arguments"
echo ""
echo "Your responsibility:"
echo "  • Restrict keystore permissions: chmod 600 ~/.anchorkit/credentials.json"
echo "  • Never commit keystore data, keypair files, or 'credentials get' output"
echo "  • Removing a credential does not guarantee the old bytes are erased from disk"
echo ""
echo "CLI Commands:"
echo "  anchorkit credentials add --name <name>     (prompts for the value)"
echo "  anchorkit credentials get --name <name>     (prints the decrypted secret)"
echo "  anchorkit credentials list"
echo "  anchorkit credentials remove --name <name>"
echo "  anchorkit credentials rotate                (re-encrypts under a new password)"
echo ""
echo "  'credentials add' also accepts --value <value>, but that exposes the"
echo "  secret in shell history and process listings. Prefer the prompt."
echo ""
echo "Usage in Contract Operations:"
echo "  anchorkit register --credential-name <name> ..."
echo "  anchorkit attest --credential-name <name> ..."
echo "  anchorkit quote --credential-name <name> ..."
echo "  anchorkit revoke --credential-name <name> ..."
echo ""
echo "Next Steps:"
echo "  1. Remove the example-* credentials created by this script"
echo "  2. Store your own keys using 'anchorkit credentials add' (interactive)"
echo "  3. Use --credential-name instead of --secret-key in interactive operations"
echo "  4. For CI, use ANCHOR_ADMIN_SECRET or --ephemeral-token from your secret store"
echo "  5. Plan key rotation and incident response (docs/key-rotation-scope.md)"
echo ""
echo "For more information, see:"
echo "  • README.md"
echo "  • docs/secret-file-encryption.md"
echo "  • docs/key-rotation-scope.md"
echo ""
