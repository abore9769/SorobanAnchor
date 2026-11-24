# Admin Audit Log

The admin audit log tracks all configuration changes made by administrators, providing a bounded history for operator investigations.

## Overview

The admin audit log records:
- **Who** made the change (admin address)
- **What** was changed (change type and target)
- **When** it was changed (timestamp)
- **Old and new values** for the configuration
- **Status** of the change (success or failed)
- **Error details** if the change failed

## Configuration Changes Tracked

### Endpoint Updates
When an attestor's endpoint is updated:
```
Change Type: endpoint_update
Target: attestor_address
Old Value: https://old.example.com
New Value: https://new.example.com
```

### Service Configuration
When an attestor's services are configured:
```
Change Type: service_config
Target: attestor_address
Old Value: deposits
New Value: deposits,withdrawals
```

### Rate Limit Updates
When rate limits are modified:
```
Change Type: rate_limit_update
Target: attestor_address
Old Value: 100
New Value: 200
```

### Other Configuration Changes
Additional change types can be added as needed:
- `webhook_update` - Webhook URL changes
- `ttl_update` - TTL configuration changes
- `cache_config_update` - Cache configuration changes
- `admin_change` - Admin address changes

## Read-only operator example

Admin configuration entries use `AdminAuditLog::get_entry`, a Rust module
interface called in the contract storage context. It is not an
`AnchorKitContractClient` query endpoint. For an existing `env` in that context:

```rust
use anchorkit::admin_audit_log::AdminAuditLog;

let entry_id = 0u64;
let record = AdminAuditLog::get_entry(&env, entry_id);
let config = AdminAuditLog::get_config(&env);
match record {
    Some(entry) => {
        // Inspect entry.change_type, target, old_value, new_value,
        // timestamp, status, and error_message in a trusted operator tool.
        // Redact before sharing; do not print the raw record into issue logs.
        assert_eq!(entry.entry_id, entry_id);
    }
    None => {
        // No stored entry at this ID; this is not an authorization error.
    }
}
```

Illustrative redacted rendering (not the wire/export format):

```text
entry_id: 0
admin: [REDACTED_ADMIN]
change_type: endpoint_update
target: [REDACTED_ATTESTOR]
old_value: https://old.example.invalid
new_value: https://new.example.invalid
timestamp: 1000
status: success
error_message: ""
```

This records an endpoint change at ledger timestamp 1000; `success` describes
the recorded change, not the endpoint's current health. A `failed` record's
`error_message` supplies diagnostic context and must also be checked for secrets.
`None` means unavailable at that ID: it may never have been written, may have
been evicted or cleared, or storage may no longer be available. It does not prove
that no privileged action occurred. These read helpers do not authenticate callers;
an operator service must apply its own access policy. A service authorization
failure must be reported separately from an absent record.

Regression references:
[`entries_can_be_retrieved_by_id`, `non_existent_entry_returns_none`, and
`failed_change_logged_with_error_message`](../tests/admin_audit_log_tests.rs).
See [retention and export limits](#retention-and-export-limits) before relying on
this log for historical investigations.

## Retention and export limits

There are two distinct stores; their settings and record formats are not
interchangeable.

- **Admin configuration records**
  ([implementation](../src/admin_audit_log.rs)): defaults are logging enabled,
  `max_entries = 10_000`, and `ttl_seconds = 31_536_000`. At capacity, each write
  removes ID `entry_id - max_entries`; `max_entries = 0` disables that eviction.
  Lowering capacity does not sweep all older records immediately.
  `clear_entries` requires the primary admin, removes records, and resets IDs
  to zero. The counter counts writes since reset, not currently retained entries.
  The TTL value must fit `u32`; it is passed directly to Soroban instance
  `extend_ttl`, whose units are ledger counts. Despite the field name, this
  implementation does not enforce a per-record wall-clock expiry. A configured
  “one year” value is therefore not a guaranteed year of history.
- **Session operation records**
  ([contract implementation](../src/contract.rs)): `get_audit_log_retention`
  defaults to `0`; `set_audit_log_retention` accepts `0..=3650` days.
  Zero represents no configured time limit, not unlimited archival availability.
  `prune_audit_logs(before_timestamp)` is admin-only and removes timestamps
  strictly less than the cutoff, keeping equality. Each call scans only IDs
  `0..min(total, 100)`; repeated calls do not advance beyond those first 100 IDs.
  Setting retention alone does not schedule pruning. If a positive auto-prune
  threshold is exceeded, `get_audit_log_count` invokes that same bounded scan
  with cutoff `now.saturating_sub(retention_days * 86400)`. This path also
  computes a cutoff when retention is zero; do not enable it expecting zero to
  preserve all records. Counts are IDs ever allocated, not live record counts.
  Storage TTL remains a separate constraint.

For session records, supported reads include `get_audit_log(log_id)`,
`get_audit_logs_paginated(offset, limit)`, and
`get_session_logs_paginated(session_id, offset, limit)`. A missing single
record raises `AuditLogNotFound`; pages skip absent records. Page limits are
capped at 50 and offsets select ID/index windows, not a count of surviving
records.

`export_audit_log_batch(start_id, limit)` exports **session operation records**,
not `AdminConfigChangeEvent` records. It returns UTF-8 JSON bytes containing an
array of quoted hex-encoded XDR records, not JSON objects. The limit is capped
at 50 IDs examined; missing IDs are skipped. An empty window returns `[]`,
including a pruned window with surviving records elsewhere. Advance by the
requested ID window (at most 50), not the number of returned records; an empty
batch does not establish that all later IDs are empty. Export does not delete
records. These query/export functions have no admin authorization check.

Operators are responsible for exporting available history before loss, decoding
hex then XDR with the matching schema, tracking contract/network and ID windows,
and protecting and retaining any external archive. Admin entries require a
separate collection using their module interface or published
`("admin", "audit", entry_id)` events; the session export is not their archive.
On-chain storage and emitted events are not an unlimited archival service.

Direct regression references:

- [Admin tests](../tests/admin_audit_log_tests.rs):
  `default_configuration_created`,
  `clear_removes_stored_records_and_resets_counter`,
  `oversized_ttl_is_rejected_before_storage_write`.
- [Retention tests](../tests/audit_log_retention_tests.rs):
  `test_max_retention_boundary_is_accepted_exactly`,
  `test_over_limit_retention_is_rejected`,
  `test_auto_prune_triggers_when_threshold_exceeded`.
- [Export/boundary tests](../tests/audit_log_retention_export_tests.rs):
  `prune_boundary_entry_at_threshold_is_kept`,
  `prune_removes_entry_strictly_before_threshold`,
  `export_each_entry_is_a_hex_string`,
  `export_range_with_no_events_returns_empty_array`,
  `export_batch_size_is_capped_at_50`.

## API Usage

### Logging a Configuration Change

```rust
use anchorkit::admin_audit_log::AdminAuditLog;

// Log a successful change
AdminAuditLog::log_change(
    &env,
    &admin_address,
    "endpoint_update",
    "attestor_001",
    "https://old.example.com",
    "https://new.example.com",
);
```

### Logging a Failed Change

```rust
// Log a failed change with error message
AdminAuditLog::log_change_with_status(
    &env,
    &admin_address,
    "endpoint_update",
    "attestor_001",
    "https://old.example.com",
    "https://invalid.url",
    "failed",
    "Invalid URL format",
);
```

### Retrieving Audit Entries

```rust
// Get a specific audit entry
if let Some(entry) = AdminAuditLog::get_entry(&env, entry_id) {
    println!("Admin: {}", entry.admin);
    println!("Change: {}", entry.change_type);
    println!("Target: {}", entry.target);
    println!("Old Value: {}", entry.old_value);
    println!("New Value: {}", entry.new_value);
    println!("Timestamp: {}", entry.timestamp);
    println!("Status: {}", entry.status);
}

// Get total number of audit entries
let count = AdminAuditLog::get_entry_count(&env);
println!("Total audit entries: {}", count);
```

### Managing Audit Log Configuration

```rust
use anchorkit::admin_audit_log::{AdminAuditLog, AdminAuditLogConfig};

// Get current configuration
let config = AdminAuditLog::get_config(&env);
println!("Logging enabled: {}", config.enabled);
println!("Max entries: {}", config.max_entries);
println!("Instance TTL argument (ledgers): {}", config.ttl_seconds);

// Update configuration
let new_config = AdminAuditLogConfig {
    enabled: true,
    max_entries: 5000,
    ttl_seconds: 86400, // Passed to instance extend_ttl as ledger counts
};
AdminAuditLog::set_config(&env, &new_config);

// Disable logging
let disabled_config = AdminAuditLogConfig {
    enabled: false,
    max_entries: 10000,
    ttl_seconds: 31_536_000,
};
AdminAuditLog::set_config(&env, &disabled_config);
```

## Audit Entry Structure

Each audit entry contains:

```rust
pub struct AdminConfigChangeEvent {
    pub entry_id: u64,              // Unique identifier
    pub admin: Address,             // Admin who made the change
    pub change_type: String,        // Type of change
    pub target: String,             // What was changed
    pub old_value: String,          // Previous value
    pub new_value: String,          // New value
    pub timestamp: u64,             // When it happened
    pub status: String,             // "success" or "failed"
    pub error_message: String,      // Error details if failed
}
```

## Configuration Options

### AdminAuditLogConfig

```rust
pub struct AdminAuditLogConfig {
    pub enabled: bool,              // Enable/disable logging
    pub max_entries: u32,           // Maximum entries to retain (0 = unlimited)
    pub ttl_seconds: u64,           // Passed to instance extend_ttl (ledger counts)
}
```

**Default Configuration:**
- Enabled: `true`
- Max Entries: `10,000`
- TTL field: `31,536,000` (passed as ledger counts; see retention limits)

## Best Practices

### 1. Enable Audit Logging in Production

Always keep audit logging enabled in production environments:

```rust
let config = AdminAuditLogConfig {
    enabled: true,
    max_entries: 10000,
    ttl_seconds: 31_536_000,
};
AdminAuditLog::set_config(&env, &config);
```

### 2. Log All Configuration Changes

Ensure every admin configuration change is logged:

```rust
// Before updating endpoint
let old_endpoint = get_current_endpoint(&env, &attestor);

// Update endpoint
update_endpoint(&env, &attestor, &new_endpoint);

// Log the change
AdminAuditLog::log_change(
    &env,
    &admin,
    "endpoint_update",
    &attestor.to_string(),
    &old_endpoint,
    &new_endpoint,
);
```

### 3. Include Descriptive Change Types

Use clear, descriptive change types:

```
✓ "endpoint_update"
✓ "service_config"
✓ "rate_limit_update"
✗ "change"
✗ "update"
```

### 4. Log Failed Changes

Always log failed configuration attempts:

```rust
match update_endpoint(&env, &attestor, &new_endpoint) {
    Ok(_) => {
        AdminAuditLog::log_change(
            &env, &admin, "endpoint_update", &target, &old, &new,
        );
    }
    Err(e) => {
        AdminAuditLog::log_change_with_status(
            &env, &admin, "endpoint_update", &target, &old, &new,
            "failed", &e.to_string(),
        );
    }
}
```

### 5. Regularly Review Audit Logs

Implement monitoring to review audit logs:

```rust
// Check for suspicious activity
for i in 0..AdminAuditLog::get_entry_count(&env) {
    if let Some(entry) = AdminAuditLog::get_entry(&env, i) {
        if entry.status == "failed" {
            // Alert on failed changes
            alert_admin(&entry);
        }
    }
}
```

## Compliance and Security

### Audit Trail Requirements

The admin audit log satisfies common compliance requirements:

- **SOC 2**: Tracks all administrative changes
- **ISO 27001**: Provides access control audit trail
- **GDPR**: Documents data processing changes
- **PCI DSS**: Records configuration modifications

### Data Retention

Use the [retention and export limits](#retention-and-export-limits) above when
planning retention. Configuration values do not guarantee wall-clock availability
or establish compliance; preserve required history in a protected external archive.

### Access Control

Apply access controls in operator tools that display audit records. The read
helpers do not enforce an admin check; see the store-specific authorization
and retention details above. Keep privileged configuration and clearing actions
restricted to authorized operators.

### Sensitive Fields

Every field is stored on-chain and published in the `("admin", "audit",
entry_id)` event. Never write credentials or unnecessary personal data into
`target`, `old_value`, `new_value`, or `error_message`. See
[audit log and provenance field sensitivity](audit-log-sensitive-fields.md) for
what each field holds and what must stay out.

## Troubleshooting

### Audit Entries Not Being Created

**Problem**: Configuration changes are not appearing in the audit log.

**Solutions**:
1. Check if logging is enabled: `AdminAuditLog::get_config(&env).enabled`
2. Verify `log_change()` is being called after each change
3. Check entry count: `AdminAuditLog::get_entry_count(&env)`

### Audit Log Growing Too Large

**Problem**: Audit log is consuming too much storage.

**Solutions**:
1. Reduce `max_entries` in configuration
2. Review instance TTL behavior in [retention limits](#retention-and-export-limits); it is not per-entry wall-clock expiry
3. Implement archival of old entries to external storage

### Missing Audit Entries

**Problem**: Some configuration changes are not logged.

**Solutions**:
1. Audit all code paths that modify configuration
2. Add logging to any missed paths
3. Review test coverage for audit logging

## Testing

The audit log includes comprehensive tests:

```bash
# Run audit log tests
cargo test admin_audit_log_tests

# Run specific test
cargo test admin_audit_log_tests::configuration_change_is_logged

# Run with output
cargo test admin_audit_log_tests -- --nocapture
```

## References

- [Admin Audit Log API](../src/admin_audit_log.rs)
- [Admin Audit Log Tests](../tests/admin_audit_log_tests.rs)
- [Audit Logging Best Practices](https://owasp.org/www-community/attacks/Audit_Log_Injection)
