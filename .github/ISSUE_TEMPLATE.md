Please do not include credentials, secrets, or sensitive personal data.

## Redact before posting

Share a minimal, sanitized reproduction using dummy data. Never attach raw
production logs, complete environment dumps, keypair files, JWTs, or signed
transaction payloads. Check issue text, screenshots, attachments, URLs, decoded
payloads, and error messages before submitting. Automatic Debug redaction does
not cover every URL or personal field.

Remove or replace:

- Stellar secret keys (`S...`), private keys, seed phrases, keypair contents,
  passwords, passphrases, `ANCHOR_ADMIN_SECRET`, and `--secret-key` values.
- SEP-10 JWTs/`--sep10-token`, access/refresh tokens, API keys, cookies,
  `Authorization`, `Proxy-Authorization`, custom credential headers such as
  `X-Api-Key`, webhook signing/HMAC keys, and signatures.
- `proxy.credentials.username` and `password`; userinfo in `proxy_url`,
  `http_proxy_url`, `https_proxy_url`, `HTTP_PROXY`, or `HTTPS_PROXY`;
  internal hosts and `no_proxy`/`NO_PROXY` entries. Check URL paths, query
  parameters, and fragments for tokens or customer identifiers too.
- Names, emails, phone numbers, IP addresses, KYC documents, customer/account
  identifiers, transaction memos, and sensitive transaction/request payloads.
  Replace addresses, hashes, and IDs that link a person or private activity
  consistently with labels such as `DUMMY_ACCOUNT_A` and `DUMMY_TX_1`.

For example, a sanitized diagnostic excerpt can look like this (all values
are dummy substitutes, not usable credentials or a runnable configuration):

```text
Authorization: Bearer DUMMY_TOKEN_REMOVED
Proxy-Authorization: DUMMY_CREDENTIAL_REMOVED
X-Api-Key: DUMMY_API_KEY_REMOVED
ANCHOR_ADMIN_SECRET=DUMMY_SECRET_REMOVED
proxy.credentials.username=DUMMY_USER
proxy.credentials.password=DUMMY_PASSWORD_REMOVED
proxy_url=http://proxy.example.invalid:3128
no_proxy=DUMMY_INTERNAL_HOST_REMOVED
customer_email=user@example.invalid
transaction.memo=DUMMY_MEMO_REMOVED
transaction.payload=DUMMY_PAYLOAD_REMOVED
```

Keep safe structural details such as field names, error codes, software version,
network type, and the steps needed to reproduce with synthetic data. If malformed
input matters, describe its shape or build an equivalent dummy input rather than
posting the original. See [credential handling](../docs/proxy-and-credentials.md)
and [secret-file handling](../docs/secret-file-encryption.md).

**Suspected vulnerability?** Do not report it in this public issue, and do not
post exploit steps, proof-of-concept code, or affected deployment details here.
Follow the
[private responsible-disclosure process](../docs/governance-and-security.md#responsible-disclosure)
and use GitHub's private vulnerability reporting for the affected repository
(**Security** tab → **Report a vulnerability**). Never include live secrets in a
report. If a credential was exposed, revoke or rotate it; editing the issue alone
does not invalidate it.

## Context
<!-- What are you trying to do, and what relevant context should maintainers know? -->

## Steps to reproduce or proposed work
<!-- For a bug, list the steps to reproduce it. For an enhancement, describe the proposed work. -->

## Expected behavior
<!-- What should happen? -->

## Acceptance criteria
<!-- What observable results would show this issue is complete? -->