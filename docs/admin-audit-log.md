# Admin Audit Log

The admin audit log tracks all configuration changes made by administrators, providing a complete audit trail for compliance and security purposes.

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
println!("TTL: {} seconds", config.ttl_seconds);

// Update configuration
let new_config = AdminAuditLogConfig {
    enabled: true,
    max_entries: 5000,
    ttl_seconds: 86400, // 1 day
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
    pub ttl_seconds: u64,           // Time-to-live for entries
}
```

**Default Configuration:**
- Enabled: `true`
- Max Entries: `10,000`
- TTL: `31,536,000` seconds (1 year)

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

Configure retention based on compliance requirements:

```rust
// 1 year retention (default)
let config = AdminAuditLogConfig {
    enabled: true,
    max_entries: 10000,
    ttl_seconds: 31_536_000,
};

// 7 years retention (compliance)
let config = AdminAuditLogConfig {
    enabled: true,
    max_entries: 100000,
    ttl_seconds: 220_752_000, // 7 years
};
```

### Access Control

Ensure only authorized admins can:
- Make configuration changes
- View audit logs
- Modify audit log configuration

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
2. Reduce `ttl_seconds` to expire old entries faster
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
