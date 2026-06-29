//! Tests for admin audit log configuration change tracking.

#![cfg(test)]

mod admin_audit_log_tests {
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
    use soroban_sdk::{Address, Env};

    use anchorkit::admin_audit_log::{AdminAuditLog, AdminAuditLogConfig};
    use anchorkit::contract::AnchorKitContract;

    fn make_env() -> (Env, soroban_sdk::Address) {
        let env = Env::default();
        env.mock_all_auths();
        let cid = env.register_contract(None, AnchorKitContract);
        (env, cid)
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

    #[test]
    fn configuration_change_is_logged() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "endpoint_update", "attestor_001",
                "https://old.example.com", "https://new.example.com");
            let entry = AdminAuditLog::get_entry(&env, 0);
            assert!(entry.is_some());
            let entry = entry.unwrap();
            assert_eq!(entry.entry_id, 0);
            assert_eq!(entry.admin, admin);
            assert_eq!(entry.change_type, soroban_sdk::String::from_str(&env, "endpoint_update"));
            assert_eq!(entry.target, soroban_sdk::String::from_str(&env, "attestor_001"));
            assert_eq!(entry.old_value, soroban_sdk::String::from_str(&env, "https://old.example.com"));
            assert_eq!(entry.new_value, soroban_sdk::String::from_str(&env, "https://new.example.com"));
            assert_eq!(entry.status, soroban_sdk::String::from_str(&env, "success"));
        });
    }

    #[test]
    fn multiple_changes_logged_sequentially() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin1, "endpoint_update", "attestor_001", "old_url", "new_url");
            AdminAuditLog::log_change(&env, &admin2, "service_config", "attestor_002", "deposits", "deposits,withdrawals");
            let entry1 = AdminAuditLog::get_entry(&env, 0).unwrap();
            let entry2 = AdminAuditLog::get_entry(&env, 1).unwrap();
            assert_eq!(entry1.entry_id, 0);
            assert_eq!(entry2.entry_id, 1);
            assert_eq!(entry1.admin, admin1);
            assert_eq!(entry2.admin, admin2);
        });
    }

    #[test]
    fn entry_count_tracked_correctly() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            assert_eq!(AdminAuditLog::get_entry_count(&env), 0);
            AdminAuditLog::log_change(&env, &admin, "change1", "target1", "old", "new");
            assert_eq!(AdminAuditLog::get_entry_count(&env), 1);
            AdminAuditLog::log_change(&env, &admin, "change2", "target2", "old", "new");
            assert_eq!(AdminAuditLog::get_entry_count(&env), 2);
            AdminAuditLog::log_change(&env, &admin, "change3", "target3", "old", "new");
            assert_eq!(AdminAuditLog::get_entry_count(&env), 3);
        });
    }

    #[test]
    fn audit_entry_includes_admin_address() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "test_change", "target", "old", "new");
            let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
            assert_eq!(entry.admin, admin);
        });
    }

    #[test]
    fn audit_entry_includes_change_type() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "endpoint_update", "target", "old", "new");
            let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
            assert_eq!(entry.change_type, soroban_sdk::String::from_str(&env, "endpoint_update"));
        });
    }

    #[test]
    fn audit_entry_includes_target() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "change", "attestor_123", "old", "new");
            let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
            assert_eq!(entry.target, soroban_sdk::String::from_str(&env, "attestor_123"));
        });
    }

    #[test]
    fn audit_entry_includes_old_and_new_values() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "change", "target", "old_value_123", "new_value_456");
            let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
            assert_eq!(entry.old_value, soroban_sdk::String::from_str(&env, "old_value_123"));
            assert_eq!(entry.new_value, soroban_sdk::String::from_str(&env, "new_value_456"));
        });
    }

    #[test]
    fn audit_entry_includes_timestamp() {
        let (env, cid) = make_env();
        set_ledger(&env, 5000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "change", "target", "old", "new");
            let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
            assert_eq!(entry.timestamp, 5000);
        });
    }

    #[test]
    fn audit_entry_includes_status() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "change", "target", "old", "new");
            let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
            assert_eq!(entry.status, soroban_sdk::String::from_str(&env, "success"));
        });
    }

    #[test]
    fn failed_change_logged_with_error_message() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change_with_status(&env, &admin, "endpoint_update", "attestor_001",
                "old_url", "new_url", "failed", "Invalid URL format");
            let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
            assert_eq!(entry.status, soroban_sdk::String::from_str(&env, "failed"));
            assert_eq!(entry.error_message, soroban_sdk::String::from_str(&env, "Invalid URL format"));
        });
    }

    #[test]
    fn successful_change_has_empty_error_message() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "change", "target", "old", "new");
            let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
            assert_eq!(entry.error_message, soroban_sdk::String::from_str(&env, ""));
        });
    }

    #[test]
    fn default_configuration_created() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        env.as_contract(&cid, || {
            let config = AdminAuditLog::get_config(&env);
            assert!(config.enabled);
            assert_eq!(config.max_entries, 10000);
            assert_eq!(config.ttl_seconds, 31_536_000);
        });
    }

    #[test]
    fn configuration_can_be_updated() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        env.as_contract(&cid, || {
            let new_config = AdminAuditLogConfig { enabled: false, max_entries: 5000, ttl_seconds: 86400 };
            AdminAuditLog::set_config(&env, &new_config);
            let config = AdminAuditLog::get_config(&env);
            assert!(!config.enabled);
            assert_eq!(config.max_entries, 5000);
            assert_eq!(config.ttl_seconds, 86400);
        });
    }

    #[test]
    fn logging_can_be_disabled() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::set_config(&env, &AdminAuditLogConfig { enabled: false, max_entries: 10000, ttl_seconds: 31_536_000 });
            AdminAuditLog::log_change(&env, &admin, "change", "target", "old", "new");
            let entry = AdminAuditLog::get_entry(&env, 0);
            assert!(entry.is_none());
        });
    }

    #[test]
    fn logging_can_be_reenabled() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::set_config(&env, &AdminAuditLogConfig { enabled: false, max_entries: 10000, ttl_seconds: 31_536_000 });
            AdminAuditLog::set_config(&env, &AdminAuditLogConfig { enabled: true, max_entries: 10000, ttl_seconds: 31_536_000 });
            AdminAuditLog::log_change(&env, &admin, "change", "target", "old", "new");
            let entry = AdminAuditLog::get_entry(&env, 0);
            assert!(entry.is_some());
        });
    }

    #[test]
    fn endpoint_update_change_type() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "endpoint_update", "attestor_001",
                "https://old.example.com", "https://new.example.com");
            let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
            assert_eq!(entry.change_type, soroban_sdk::String::from_str(&env, "endpoint_update"));
        });
    }

    #[test]
    fn service_config_change_type() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "service_config", "attestor_001", "deposits", "deposits,withdrawals");
            let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
            assert_eq!(entry.change_type, soroban_sdk::String::from_str(&env, "service_config"));
        });
    }

    #[test]
    fn rate_limit_update_change_type() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "rate_limit_update", "attestor_001", "100", "200");
            let entry = AdminAuditLog::get_entry(&env, 0).unwrap();
            assert_eq!(entry.change_type, soroban_sdk::String::from_str(&env, "rate_limit_update"));
        });
    }

    #[test]
    fn non_existent_entry_returns_none() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        env.as_contract(&cid, || {
            let entry = AdminAuditLog::get_entry(&env, 999);
            assert!(entry.is_none());
        });
    }

    #[test]
    fn entries_can_be_retrieved_by_id() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            AdminAuditLog::log_change(&env, &admin, "change1", "target1", "old1", "new1");
            AdminAuditLog::log_change(&env, &admin, "change2", "target2", "old2", "new2");
            AdminAuditLog::log_change(&env, &admin, "change3", "target3", "old3", "new3");
            let entry0 = AdminAuditLog::get_entry(&env, 0).unwrap();
            let entry1 = AdminAuditLog::get_entry(&env, 1).unwrap();
            let entry2 = AdminAuditLog::get_entry(&env, 2).unwrap();
            assert_eq!(entry0.entry_id, 0);
            assert_eq!(entry1.entry_id, 1);
            assert_eq!(entry2.entry_id, 2);
        });
    }
}

#[cfg(test)]
mod admin_bypass_audit_tests {
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
    use soroban_sdk::{Address, Env};
    use anchorkit::admin_audit_log::AdminAuditLog;
    use anchorkit::contract::AnchorKitContract;
    use anchorkit::rate_limiter::{RateLimiter, RateLimitConfig};
    use anchorkit::deterministic_hash::make_storage_key;

    fn make_env() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let cid = env.register_contract(None, AnchorKitContract);
        (env, cid)
    }

    fn set_ledger(env: &Env, ts: u64) {
        env.ledger().set(LedgerInfo {
            timestamp: ts,
            protocol_version: 21,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });
    }

    /// Admin bypass writes an audit log entry with change_type = "rate_limit_bypass".
    #[test]
    fn test_admin_bypass_is_logged() {
        let (env, cid) = make_env();
        set_ledger(&env, 1000);
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            // Store admin address so bypass is detected.
            env.storage()
                .instance()
                .set(&make_storage_key(&env, &[b"ADMIN"]), &admin);
            let config = RateLimitConfig { max_submissions: 5, window_length: 100 };
            // Admin bypass: check_and_increment should succeed AND log.
            let before = AdminAuditLog::get_entry_count(&env);
            let result = RateLimiter::check_and_increment(&env, &admin, &config);
            assert!(result.is_ok());
            let after = AdminAuditLog::get_entry_count(&env);
            assert_eq!(after, before + 1, "bypass should write one audit entry");
            let entry = AdminAuditLog::get_entry(&env, before).unwrap();
            assert_eq!(
                entry.change_type,
                soroban_sdk::String::from_str(&env, "rate_limit_bypass")
            );
        });
    }
}
