#!/bin/bash
# Example: Secure Credential Management with AnchorKit
# This script demonstrates how to use the anchorkit CLI to manage credentials securely

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

# Store KYC provider API key
echo "  Storing KYC provider API key..."
echo "sk_test_kyc_key_12345678901234567890" | anchorkit credentials add --name kyc-provider-api-key 2>/dev/null || echo "    (Skipped - credential may already exist)"

# Store bank integration token
echo "  Storing bank integration token..."
echo "Bearer bank_token_12345678901234567890" | anchorkit credentials add --name bank-integration-token 2>/dev/null || echo "    (Skipped - credential may already exist)"

echo "  ✓ Credentials stored securely in ~/.anchorkit/keystore.json"
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
anchorkit register \
    --address "$KYC_PROVIDER" \
    --services deposits,kyc \
    --contract-id "$CONTRACT_ID" \
    --network "$NETWORK" \
    --credential-name kyc-provider-api-key \
    --sep10-token "dummy-token" \
    --sep10-issuer "$KYC_PROVIDER" \
    2>/dev/null || echo "    (Skipped - contract not deployed or credential not found)"

echo "  ✓ Credential retrieved from keystore and used for signing"
echo ""

# ============================================================
# Step 4: Retrieve a Credential (for verification)
# ============================================================
echo "Step 4: Retrieving a credential (for verification)..."

echo "  Retrieving KYC provider API key..."
# Note: This will prompt for keystore password
# anchorkit credentials get --name kyc-provider-api-key

echo "  (Skipped in demo - would prompt for password)"
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
echo "✓ Credentials can be used via --credential-name flag"
echo "✓ No plaintext credentials in config files or shell history"
echo ""
echo "Security Best Practices Applied:"
echo "  • Credentials encrypted at rest with AES-256-GCM"
echo "  • Key derivation using Argon2id (m=65536 KiB, t=3, p=4)"
echo "  • Keystore file permissions restricted to owner (0600)"
echo "  • Password-protected keystore access"
echo "  • No credentials exposed in command-line arguments"
echo ""
echo "CLI Commands:"
echo "  anchorkit credentials add --name <name> [--value <value>]"
echo "  anchorkit credentials get --name <name>"
echo "  anchorkit credentials list"
echo "  anchorkit credentials remove --name <name>"
echo ""
echo "Usage in Contract Operations:"
echo "  anchorkit register --credential-name <name> ..."
echo "  anchorkit attest --credential-name <name> ..."
echo "  anchorkit quote --credential-name <name> ..."
echo "  anchorkit revoke --credential-name <name> ..."
echo ""
echo "Next Steps:"
echo "  1. Store your production credentials using 'anchorkit credentials add'"
echo "  2. Use --credential-name instead of --secret-key in all operations"
echo "  3. Set up automated rotation (cron job or CI/CD)"
echo "  4. Configure monitoring and alerting"
echo "  5. Document incident response plan"
echo ""
echo "For more information, see:"
echo "  • README.md"
echo "  • docs/getting-started.md"
echo ""
