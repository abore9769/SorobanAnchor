# Cache Policy Reference

This document describes the cache governance guardrails enforced by `src/cache_governance.rs`: the allowed TTL bounds, who may approve TTL changes, and the outcomes of policy updates and forced invalidation. It is a reference for maintainers and operators and does not change any cache defaults.

For log event formats, see [Structured Logging](structured-logging.md). For approval roles and authorization, see [Governance and Security](governance-and-security.md).

## TTL Bounds

Cache TTL policy is validated against the following bounds. Any proposed policy that violates these bounds is rejected.

| Field | Rule | Notes |
| --- | --- | --- |
| `min_ttl_seconds` | Must be greater than `0` | A zero or negative minimum TTL is invalid. |
| `max_ttl_seconds` | Must be greater than or equal to `min_ttl_seconds` | Defines the upper bound for any effective TTL. |
| `refresh_threshold_pct` | Must be a valid percentage within the allowed range | Controls when a cached entry is refreshed relative to its TTL. |

Effective TTLs are clamped into the `[min_ttl_seconds, max_ttl_seconds]` range. When a requested TTL falls outside these bounds, it is clamped and a `cache.ttl_clamped` event is emitted. The clamp never widens the configured bounds; it only brings an out-of-range value back within them.

These bounds and their validation match the cache governance tests (`tests/cache_governance_tests.rs`, `tests/cache_policy_tests.rs`). No cache defaults are altered by this document.

## Approval and Authorization

TTL policy changes require governance approval. Only the roles authorized by the governance model may approve a TTL change; see [Governance and Security](governance-and-security.md) for the authoritative role list and approval workflow.

- Proposals that satisfy the TTL bounds and are approved by an authorized role are applied.
- Proposals that violate the TTL bounds, or that lack approval from an authorized role, are rejected and the existing policy remains in effect.

## Policy Update Outcomes

Approved and rejected policy updates are distinguishable in both behavior and logging:

- **Approved update:** The new TTL policy is applied. The change is recorded through the governance logging path described in [Structured Logging](structured-logging.md).
- **Rejected update:** The new TTL policy is not applied; the prior policy stays in effect. The rejection is recorded through the same logging path so operators can distinguish it from an approval.

## Forced Invalidation

Forced invalidation clears cached entries on demand. It is subject to authorization:

- **Authorized invalidation:** The invalidation proceeds and its outcome is logged.
- **Unauthorized invalidation:** The request is denied, no entries are cleared, and a `cache.invalidation_denied` event is emitted.

Operators should watch for `cache.invalidation_denied` events, which indicate an invalidation attempt that failed the authorization check. The denial behavior matches `tests/cache_invalidation_hook_tests.rs`.

## Related Documentation

- [Structured Logging](structured-logging.md) — event formats, including `cache.ttl_clamped` and `cache.invalidation_denied`.
- [Governance and Security](governance-and-security.md) — approval roles, authorization, and the governance workflow.