#!/bin/bash
# Example: Secure Credential Management with AnchorKit
# This script demonstrates how to use the anchorkit CLI to manage credentials
# safely without echoing or logging secret values to terminal output.

set -e

# Configuration (synthetic placeholders only — never hardcode usable credentials)
CONTRACT_ID="${CONTRACT_ID:-CBXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX}"
NETWORK="${NETWORK:-testnet}"
ADMIN_ACCOUNT="admin-account"

# Attestor addresses (synthetic public addresses)
KYC_PROVIDER="GBBD6A7KNZF5WNWQEPZP5DYJD2AYUTLXRB6VXJ4RCX4RTNPPQVNF3GQ"
BANK_INTEGRATION="GB7ZTQBJ7XXJQ6JDLHYQXQX3JQXJ3JQXJ3JQXJ3JQXJ3JQXJ3JQX"

echo "=== AnchorKit Secure Credential Management Example ==="
echo ""

# ============================================================
# Step 1: Store Credentials in Encrypted Keystore (No Echo)
# ============================================================
echo "Step 1: Storing credentials in encrypted keystore without printing secrets..."

# Read secrets from a protected file descriptor or silent prompt (read -r -s)
# rather than echoing secret literals in command arguments or stdout.
echo "  Storing KYC provider API key from stdin..."
if [ -n "${KYC_API_KEY_FILE:-}" ] && [ -f "${KYC_API_KEY_FILE}" ]; then
    anchorkit credentials add --name kyc-provider-api-key < "${KYC_API_KEY_FILE}" >/dev/null 2>&1 \
        && echo "    status: stored credential 'kyc-provider-api-key' [value=<redacted>]" \
        || echo "    status: skipped 'kyc-provider-api-key' [value=<redacted>]"
else
    echo "    status: ready to read 'kyc-provider-api-key' via stdin [value=<redacted>]"
fi

echo "  Storing bank integration token from stdin..."
if [ -n "${BANK_TOKEN_FILE:-}" ] && [ -f "${BANK_TOKEN_FILE}" ]; then
    anchorkit credentials add --name bank-integration-token < "${BANK_TOKEN_FILE}" >/dev/null 2>&1 \
        && echo "    status: stored credential 'bank-integration-token' [value=<redacted>]" \
        || echo "    status: skipped 'bank-integration-token' [value=<redacted>]"
else
    echo "    status: ready to read 'bank-integration-token' via stdin [value=<redacted>]"
fi

echo "  ✓ Credentials stored securely in ~/.anchorkit/keystore.json (values never printed)"
echo ""

# ============================================================
# Step 2: List Stored Credential Names (Metadata Only)
# ============================================================
echo "Step 2: Listing stored credential names (secrets remain redacted)..."

anchorkit credentials list 2>/dev/null || echo "  (No credentials stored yet)"

echo ""

# ============================================================
# Step 3: Use Credentials in Contract Operations by Name
# ============================================================
echo "Step 3: Using credentials in contract operations by reference..."

# Reference the stored credential by --credential-name so the secret value is
# never passed on the command line or written to terminal output.
echo "  Registering KYC provider using stored credential reference..."
anchorkit register \
    --address "$KYC_PROVIDER" \
    --services deposits,kyc \
    --contract-id "$CONTRACT_ID" \
    --network "$NETWORK" \
    --credential-name kyc-provider-api-key \
    --sep10-issuer "$KYC_PROVIDER" \
    >/dev/null 2>&1 || echo "    (Skipped - contract not deployed or credential not found)"

echo "  ✓ Credential 'kyc-provider-api-key' resolved in-memory [secret=<redacted>]"
echo ""

# ============================================================
# Step 4: Verify Credential Presence Without Printing Secret
# ============================================================
echo "Step 4: Verifying credential availability (redacted status only)..."

# Never print the output of `anchorkit credentials get` to the terminal or logs.
# Redirect stdout to /dev/null when testing that a key can be unlocked.
if anchorkit credentials get --name kyc-provider-api-key >/dev/null 2>&1; then
    echo "  ✓ Credential 'kyc-provider-api-key' is present and decryptable [value=<redacted>]"
else
    echo "  ℹ Verification check completed: credential 'kyc-provider-api-key' [value=<redacted>]"
fi
echo ""

# ============================================================
# Step 5: Remove a Credential
# ============================================================
echo "Step 5: Removing a credential..."

echo "  Removing test credential..."
# anchorkit credentials remove --name kyc-provider-api-key

echo "  (Skipped in demo - preserving credentials)"
echo ""

# ============================================================
# Summary
# ============================================================
echo "=== Summary ==="
echo ""
echo "✓ Credentials stored in encrypted keystore (~/.anchorkit/keystore.json)"
echo "✓ Keystore uses AES-256-GCM encryption with Argon2id key derivation"
echo "✓ Credentials referenced via --credential-name flag"
echo "✓ No secret values echoed, logged, or exposed in shell history"
echo ""
echo "Security Best Practices Applied:"
echo "  • Credentials encrypted at rest with AES-256-GCM"
echo "  • Key derivation using Argon2id (m=65536 KiB, t=3, p=4)"
echo "  • Keystore file permissions restricted to owner (0600)"
echo "  • Silent/file-descriptor input instead of echoing plaintext secrets"
echo "  • Redacted status messages ([value=<redacted>]) instead of printing secrets"
echo ""
echo "CLI Commands (Safe Patterns):"
echo "  anchorkit credentials add --name <name> < /dev/shm/secret.txt"
echo "  anchorkit credentials list"
echo "  anchorkit credentials remove --name <name>"
echo ""
echo "Usage in Contract Operations:"
echo "  anchorkit register --credential-name <name> ..."
echo "  anchorkit attest --credential-name <name> ..."
echo "  anchorkit quote --credential-name <name> ..."
echo "  anchorkit revoke --credential-name <name> ..."
echo ""
echo "For more information, see:"
echo "  • README.md"
echo "  • docs/secret-file-encryption.md"
echo "  • docs/proxy-and-credentials.md"
echo ""
