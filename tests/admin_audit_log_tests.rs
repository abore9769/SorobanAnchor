//! Tests for admin audit log configuration change tracking.
//!
//! These tests verify that:
//! 1. Configuration changes are recorded in the audit log
//! 2. Audit entries include sufficient detail (admin, change type, old/new values)
//! 3. Audit log entries are retrievable and queryable
//! 4. Audit logging can be enabled/disabled
//! 5. Audit entries include timestamps and status

#![cfg(test)]

mod admin_audit_log_tests {
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
    use soroban_sdk::{Address, Env};

    use anchorkit::admin_audit_log::{AdminAuditLog, AdminAuditLogConfig, AdminConfigChangeEvent};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn set_ledger(env: &Env, ts: u64) {
        env.ledger().set(LedgerInfo {
            timestamp: ts,
            protocol_version: 21,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });
    }

    // -----------------------------------------------------------------------
    // Basic Logging Tests
    // -----------------------------------------------------------------------

    /// Test that a configuration change is logged
    #[test]
    fn configuration_change_is_logged() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(
            &env,
            &admin,
            "endpoint_update",
            "attestor_001",
            "https://old.example.com",
            "https://new.example.com",
        );

        // Verify entry was created
        let entry = AdminAuditLog::get_entry(&env, 0);
        assert!(entry.is_some());

        let entry = entry.unwrap();
        assert_eq!(entry.entry_id, 0);
        assert_eq!(entry.admin, admin);
        assert_eq!(entry.change_type, soroban_sdk::String::from_small_str(&env, "endpoint_update"));
        assert_eq!(entry.target, soroban_sdk::String::from_small_str(&env, "attestor_001"));
        assert_eq!(entry.old_value, soroban_sdk::String::from_small_str(&env, "https://old.example.com"));
        assert_eq!(entry.new_value, soroban_sdk::String::from_small_str(&env, "https://new.example.com"));
        assert_eq!(entry.status, soroban_sdk::String::from_small_str(&env, "success"));
    }

    /// Test that multiple configuration changes are logged sequentially
    #[test]
    fn multiple_changes_logged_sequentially() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);

        // Log first change
        AdminAuditLog::log_change(
            &env,
            &admin1,
            "endpoint_update",
            "attestor_001",
            "old_url",
            "new_url",
        );

        // Log second change
        AdminAuditLog::log_change(
            &env,
            &admin2,
            "service_config",
            "attestor_002",
            "deposits",
            "deposits,withdrawals",
        );

        // Verify both entries exist
        let entry1 = AdminAuditLog::get_entry(&env, 0).unwrap();
        let entry2 = AdminAuditLog::get_entry(&env, 1).unwrap();

        assert_eq!(entry1.entry_id, 0);
        assert_eq!(entry2.entry_id, 1);
        assert_eq!(entry1.admin, admin1);
        assert_eq!(entry2.admin, admin2);
    }

    /// Test that entry count is tracked correctly
    #[test]
    fn entry_count_tracked_correctly() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        assert_eq!(AdminAuditLog::get_entry_count(&env), 0);

        AdminAuditLog::log_change(&env, &admin, "change1", "target1", "old", "new");
        assert_eq!(AdminAuditLog::get_entry_count(&env), 1);

        AdminAuditLog::log_change(&env, &admin, "change2", "target2", "old", "new");
        assert_eq!(AdminAuditLog::get_entry_count(&env), 2);

        AdminAuditLog::log_change(&env, &admin, "change3", "target3", "old", "new");
        assert_eq!(AdminAuditLog::get_entry_count(&env), 3);
    }

    // -----------------------------------------------------------------------
    // Audit Entry Detail Tests
    // -----------------------------------------------------------------------

    /// Test that audit entries include admin address
    #[test]
    fn audit_entry_includes_admin_address() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(&env, &admin, "test_change", "target", "old", "new");

        let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
        assert_eq!(entry.admin, admin);
    }

    /// Test that audit entries include change type
    #[test]
    fn audit_entry_includes_change_type() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(&env, &admin, "endpoint_update", "target", "old", "new");

        let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
        assert_eq!(entry.change_type, soroban_sdk::String::from_small_str(&env, "endpoint_update"));
    }

    /// Test that audit entries include target
    #[test]
    fn audit_entry_includes_target() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(&env, &admin, "change", "attestor_123", "old", "new");

        let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
        assert_eq!(entry.target, soroban_sdk::String::from_small_str(&env, "attestor_123"));
    }

    /// Test that audit entries include old and new values
    #[test]
    fn audit_entry_includes_old_and_new_values() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(
            &env,
            &admin,
            "change",
            "target",
            "old_value_123",
            "new_value_456",
        );

        let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
        assert_eq!(entry.old_value, soroban_sdk::String::from_small_str(&env, "old_value_123"));
        assert_eq!(entry.new_value, soroban_sdk::String::from_small_str(&env, "new_value_456"));
    }

    /// Test that audit entries include timestamp
    #[test]
    fn audit_entry_includes_timestamp() {
        let env = make_env();
        set_ledger(&env, 5000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(&env, &admin, "change", "target", "old", "new");

        let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
        assert_eq!(entry.timestamp, 5000);
    }

    /// Test that audit entries include status
    #[test]
    fn audit_entry_includes_status() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(&env, &admin, "change", "target", "old", "new");

        let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
        assert_eq!(entry.status, soroban_sdk::String::from_small_str(&env, "success"));
    }

    // -----------------------------------------------------------------------
    // Status and Error Message Tests
    // -----------------------------------------------------------------------

    /// Test that failed changes can be logged with error message
    #[test]
    fn failed_change_logged_with_error_message() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change_with_status(
            &env,
            &admin,
            "endpoint_update",
            "attestor_001",
            "old_url",
            "new_url",
            "failed",
            "Invalid URL format",
        );

        let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
        assert_eq!(entry.status, soroban_sdk::String::from_small_str(&env, "failed"));
        assert_eq!(entry.error_message, soroban_sdk::String::from_small_str(&env, "Invalid URL format"));
    }

    /// Test that successful changes have empty error message
    #[test]
    fn successful_change_has_empty_error_message() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(&env, &admin, "change", "target", "old", "new");

        let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
        assert_eq!(entry.error_message, soroban_sdk::String::from_small_str(&env, ""));
    }

    // -----------------------------------------------------------------------
    // Configuration Tests
    // -----------------------------------------------------------------------

    /// Test that default configuration is created
    #[test]
    fn default_configuration_created() {
        let env = make_env();
        set_ledger(&env, 1000);

        let config = AdminAuditLog::get_config(&env);
        assert!(config.enabled);
        assert_eq!(config.max_entries, 10000);
        assert_eq!(config.ttl_seconds, 31_536_000);
    }

    /// Test that configuration can be updated
    #[test]
    fn configuration_can_be_updated() {
        let env = make_env();
        set_ledger(&env, 1000);

        let new_config = AdminAuditLogConfig {
            enabled: false,
            max_entries: 5000,
            ttl_seconds: 86400,
        };

        AdminAuditLog::set_config(&env, &new_config);

        let config = AdminAuditLog::get_config(&env);
        assert!(!config.enabled);
        assert_eq!(config.max_entries, 5000);
        assert_eq!(config.ttl_seconds, 86400);
    }

    /// Test that logging can be disabled
    #[test]
    fn logging_can_be_disabled() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        // Disable logging
        let config = AdminAuditLogConfig {
            enabled: false,
            max_entries: 10000,
            ttl_seconds: 31_536_000,
        };
        AdminAuditLog::set_config(&env, &config);

        // Try to log a change
        AdminAuditLog::log_change(&env, &admin, "change", "target", "old", "new");

        // Entry should not be created
        let entry = AdminAuditLog::get_entry(&env, 0);
        assert!(entry.is_none());
    }

    /// Test that logging can be re-enabled
    #[test]
    fn logging_can_be_reenabled() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        // Disable logging
        let config = AdminAuditLogConfig {
            enabled: false,
            max_entries: 10000,
            ttl_seconds: 31_536_000,
        };
        AdminAuditLog::set_config(&env, &config);

        // Re-enable logging
        let config = AdminAuditLogConfig {
            enabled: true,
            max_entries: 10000,
            ttl_seconds: 31_536_000,
        };
        AdminAuditLog::set_config(&env, &config);

        // Log a change
        AdminAuditLog::log_change(&env, &admin, "change", "target", "old", "new");

        // Entry should be created
        let entry = AdminAuditLog::get_entry(&env, 0);
        assert!(entry.is_some());
    }

    // -----------------------------------------------------------------------
    // Change Type Tests
    // -----------------------------------------------------------------------

    /// Test endpoint update change type
    #[test]
    fn endpoint_update_change_type() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(
            &env,
            &admin,
            "endpoint_update",
            "attestor_001",
            "https://old.example.com",
            "https://new.example.com",
        );

        let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
        assert_eq!(entry.change_type, soroban_sdk::String::from_small_str(&env, "endpoint_update"));
    }

    /// Test service configuration change type
    #[test]
    fn service_config_change_type() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(
            &env,
            &admin,
            "service_config",
            "attestor_001",
            "deposits",
            "deposits,withdrawals",
        );

        let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
        assert_eq!(entry.change_type, soroban_sdk::String::from_small_str(&env, "service_config"));
    }

    /// Test rate limit update change type
    #[test]
    fn rate_limit_update_change_type() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(
            &env,
            &admin,
            "rate_limit_update",
            "attestor_001",
            "100",
            "200",
        );

        let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
        assert_eq!(entry.change_type, soroban_sdk::String::from_small_str(&env, "rate_limit_update"));
    }

    // -----------------------------------------------------------------------
    // Retrieval Tests
    // -----------------------------------------------------------------------

    /// Test that non-existent entries return None
    #[test]
    fn non_existent_entry_returns_none() {
        let env = make_env();
        set_ledger(&env, 1000);

        let entry = AdminAuditLog::get_entry(&env, 999);
        assert!(entry.is_none());
    }

    /// Test that entries can be retrieved by ID
    #[test]
    fn entries_can_be_retrieved_by_id() {
        let env = make_env();
        set_ledger(&env, 1000);

        let admin = Address::generate(&env);

        AdminAuditLog::log_change(&env, &admin, "change1", "target1", "old1", "new1");
        AdminAuditLog::log_change(&env, &admin, "change2", "target2", "old2", "new2");
        AdminAuditLog::log_change(&env, &admin, "change3", "target3", "old3", "new3");

        let entry0 = AdminAuditLog::get_entry(&env, 0).unwrap();
        let entry1 = AdminAuditLog::get_entry(&env, 1).unwrap();
        let entry2 = AdminAuditLog::get_entry(&env, 2).unwrap();

        assert_eq!(entry0.entry_id, 0);
        assert_eq!(entry1.entry_id, 1);
        assert_eq!(entry2.entry_id, 2);
    }
}
