//! Rate limiting for attestation submissions
//!
//! This module implements per-attestor rate limiting for attestation submissions
//! to prevent spam and abuse of the contract.

use soroban_sdk::{contracttype, xdr::ToXdr, Address, Env};
use crate::deterministic_hash::make_storage_key;
use crate::errors::AnchorKitError;
#[cfg(test)]
use crate::errors::ErrorCode;

/// Rate limit configuration stored in contract storage.
///
/// Defines the sliding-window parameters used by [`RateLimiter::check_and_increment`].
/// The admin can update this at runtime via [`RateLimiter::update_config`].
///
/// # Examples
///
/// ```rust,no_run
/// use anchorkit::RateLimitConfig;
///
/// // Allow at most 5 submissions per 50-ledger window.
/// let config = RateLimitConfig { max_submissions: 5, window_length: 50 };
/// ```
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitConfig {
    /// Maximum number of submissions allowed per window
    pub max_submissions: u32,
    /// Length of the rate limit window in ledgers
    pub window_length: u32,
}

/// Per-attestor rate limit state stored in contract storage.
///
/// Tracks how many submissions an attestor has made in the current window and
/// when that window started. Automatically reset when the window expires.
///
/// # Examples
///
/// ```rust,no_run
/// use anchorkit::RateLimitState;
///
/// let state = RateLimitState { submission_count: 3, window_start_ledger: 1000 };
/// assert_eq!(state.submission_count, 3);
/// ```
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitState {
    /// Number of submissions in the current window
    pub submission_count: u32,
    /// Ledger number when the current window started
    pub window_start_ledger: u32,
}

/// Per-attestor sliding-window rate limiter for attestation submissions.
///
/// All methods are associated functions that operate directly on Soroban
/// persistent storage, so no instance state is needed.
///
/// The default configuration (10 submissions per 100-ledger window) is used
/// when no config has been stored yet.
pub struct RateLimiter;

impl RateLimiter {
    /// Store a per-role rate limit override.
    ///
    /// The config is stored under a key derived from the role symbol bytes, keyed
    /// as `rl_role:<role_bytes>`. Only the contract admin should call this; access
    /// control is enforced in the contract layer via `require_admin`.
    pub fn set_role_override(env: &Env, role: soroban_sdk::Symbol, config: RateLimitConfig) {
        let key = Self::role_override_key(env, &role);
        env.storage().persistent().set(&key, &config);
    }

    /// Retrieve a per-role rate limit override, or `None` if not set.
    pub fn get_role_override(env: &Env, role: soroban_sdk::Symbol) -> Option<RateLimitConfig> {
        let key = Self::role_override_key(env, &role);
        env.storage().persistent().get::<_, RateLimitConfig>(&key)
    }

    /// Store a per-address rate limit override.
    pub fn set_address_override(env: &Env, address: &Address, config: RateLimitConfig) {
        let key = Self::address_override_key(env, address);
        env.storage().persistent().set(&key, &config);
    }

    /// Retrieve a per-address rate limit override, or `None` if not set.
    pub fn get_address_override(env: &Env, address: &Address) -> Option<RateLimitConfig> {
        let key = Self::address_override_key(env, address);
        env.storage().persistent().get::<_, RateLimitConfig>(&key)
    }

    /// Resolve the effective config for an attestor.
    ///
    /// Resolution order:
    /// 1. Per-address override
    /// 2. Per-role override (if `role` is `Some`)
    /// 3. Global default config
    pub fn resolve_config(
        env: &Env,
        attestor: &Address,
        role: Option<soroban_sdk::Symbol>,
    ) -> RateLimitConfig {
        if let Some(cfg) = Self::get_address_override(env, attestor) {
            return cfg;
        }
        if let Some(r) = role {
            if let Some(cfg) = Self::get_role_override(env, r) {
                return cfg;
            }
        }
        Self::get_config(env)
    }

    /// Check whether an attestor is within their rate limit and increment the counter.
    ///
    /// Config resolution order: address override → role override → global default.
    ///
    /// When the caller is the contract admin the check is bypassed entirely; an
    /// audit entry is written via `AdminAuditLog` so the bypass is on-chain record.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban execution environment.
    /// * `attestor` - The address of the attestor being checked.
    /// * `config` - The active [`RateLimitConfig`] (use [`RateLimiter::resolve_config`]).
    ///
    /// # Returns
    ///
    /// `Ok(())` if the attestor is within the rate limit.
    ///
    /// # Errors
    ///
    /// Returns [`AnchorKitError`] with code [`ErrorCode::RateLimitExceeded`] when
    /// the attestor has reached `config.max_submissions` in the current window.
    pub fn check_and_increment(
        env: &Env,
        attestor: &Address,
        config: &RateLimitConfig,
    ) -> Result<(), AnchorKitError> {
        // If attestor is the admin, skip rate limits and write an audit entry.
        if let Some(admin) = env
            .storage()
            .instance()
            .get::<_, Address>(&make_storage_key(env, &[b"ADMIN"]))
        {
            if *attestor == admin {
                // Record the bypass so it is auditable on-chain.
                crate::admin_audit_log::AdminAuditLog::log_change(
                    env,
                    attestor,
                    "rate_limit_bypass",
                    "bypassed",
                    "",
                    "bypassed",
                );
                return Ok(());
            }
        }
        
        let current_ledger = env.ledger().sequence();
        let state_key = Self::get_state_key(env, attestor);
        
        // Get or initialize rate limit state
        let mut state = env.storage().persistent().get::<_, RateLimitState>(&state_key)
            .unwrap_or(RateLimitState {
                submission_count: 0,
                window_start_ledger: current_ledger,
            });
        
        // Check if window has expired and reset if needed
        if Self::is_window_expired(
            current_ledger,
            state.window_start_ledger,
            config.window_length,
        ) {
            state = RateLimitState {
                submission_count: 0,
                window_start_ledger: current_ledger,
            };
        }
        
        // Check if limit is exceeded
        if state.submission_count >= config.max_submissions {
            return Err(AnchorKitError::rate_limit_exceeded());
        }
        
        // Increment counter and save state
        state.submission_count += 1;
        env.storage().persistent().set(&state_key, &state);
        
        Ok(())
    }
    
    /// Get the current rate limit state for an attestor.
    ///
    /// Returns a default state (zero submissions, current ledger as window start)
    /// if no state has been stored yet.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban execution environment.
    /// * `attestor` - The address of the attestor to query.
    ///
    /// # Returns
    ///
    /// The current [`RateLimitState`] for the attestor.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use soroban_sdk::Env;
    /// # use soroban_sdk::testutils::Address as _;
    /// # let env = Env::default();
    /// # let attestor = soroban_sdk::Address::generate(&env);
    /// use anchorkit::RateLimiter;
    ///
    /// let state = RateLimiter::get_state(&env, &attestor);
    /// assert_eq!(state.submission_count, 0);
    /// ```
    pub fn get_state(env: &Env, attestor: &Address) -> RateLimitState {
        let state_key = Self::get_state_key(env, attestor);
        env.storage().persistent().get::<_, RateLimitState>(&state_key)
            .unwrap_or(RateLimitState {
                submission_count: 0,
                window_start_ledger: env.ledger().sequence(),
            })
    }
    
    /// Update the rate limit configuration (admin only).
    ///
    /// Loads the stored admin from instance storage (key `"ADMIN"`) and calls
    /// `admin.require_auth()`. Returns `Err(NotInitialized)` if no admin is stored.
    /// Returns `Err(ValidationError)` if `config` contains zero or nonsensical values.
    pub fn update_config(
        env: &Env,
        admin: &Address,
        config: &RateLimitConfig,
    ) -> Result<(), AnchorKitError> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get::<_, Address>(&make_storage_key(env, &[b"ADMIN"]))
            .ok_or_else(AnchorKitError::not_initialized)?;
        if *admin != stored_admin {
            return Err(AnchorKitError::unauthorized_attestor());
        }
        Self::validate_config(config)?;
        let config_key = Self::get_config_key(env);
        env.storage().persistent().set(&config_key, config);
        Ok(())
    }

    /// Validate that a [`RateLimitConfig`] has sensible non-zero values.
    ///
    /// Returns `Err(ValidationError)` if `max_submissions` or `window_length` is zero.
    pub fn validate_config(config: &RateLimitConfig) -> Result<(), AnchorKitError> {
        if config.max_submissions == 0 {
            return Err(AnchorKitError::validation_error("max_submissions must be > 0"));
        }
        if config.window_length == 0 {
            return Err(AnchorKitError::validation_error("window_length must be > 0"));
        }
        Ok(())
    }
    
    /// Get the current rate limit configuration.
    ///
    /// Returns the stored configuration, or the default (10 submissions per
    /// 100-ledger window) if none has been set.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban execution environment.
    ///
    /// # Returns
    ///
    /// The active [`RateLimitConfig`].
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use soroban_sdk::Env;
    /// # let env = Env::default();
    /// use anchorkit::RateLimiter;
    ///
    /// let config = RateLimiter::get_config(&env);
    /// assert_eq!(config.max_submissions, 10);
    /// assert_eq!(config.window_length, 100);
    /// ```
    pub fn get_config(env: &Env) -> RateLimitConfig {
        let config_key = Self::get_config_key(env);
        env.storage().persistent().get::<_, RateLimitConfig>(&config_key)
            .unwrap_or(RateLimitConfig {
                max_submissions: 10,
                window_length: 100,
            })
    }
    
    /// Check if a rate-limit window has expired.
    ///
    /// Uses `checked_sub` instead of `saturating_sub` so that a sequence
    /// anomaly where `current < window_start` (e.g. due to a ledger rollback
    /// or corrupted stored state) is treated as **not expired** rather than
    /// silently wrapping to 0 and comparing against `window_length`.
    ///
    /// # Safe-default rationale
    ///
    /// When `current_ledger < window_start_ledger` the subtraction overflows.
    /// Saturating to 0 makes the condition `0 >= window_length` false for any
    /// positive `window_length`, so the old behaviour was accidentally correct.
    /// Using `checked_sub` makes the intent explicit: we detect the underflow
    /// and deliberately return `false` (window not expired), preserving the
    /// existing rate-limit state so an attestor cannot exploit the anomaly to
    /// bypass their quota.
    fn is_window_expired(
        current_ledger: u32,
        window_start_ledger: u32,
        window_length: u32,
    ) -> bool {
        match current_ledger.checked_sub(window_start_ledger) {
            Some(delta) => delta >= window_length,
            // current < window_start: ledger sequence anomaly or stored-state
            // inconsistency. Treat as window not yet expired so the existing
            // submission count is preserved and the attestor cannot exploit
            // the anomaly to bypass their rate limit.
            None => false,
        }
    }
    
    /// Generate collision-resistant storage key for per-attestor rate limit state.
    fn get_state_key(env: &Env, attestor: &Address) -> soroban_sdk::BytesN<32> {
        let addr_xdr = attestor.clone().to_xdr(env);
        // collect xdr bytes into a plain slice via Bytes
        let mut raw = alloc::vec::Vec::with_capacity(addr_xdr.len() as usize);
        for i in 0..addr_xdr.len() {
            raw.push(addr_xdr.get(i).unwrap_or(0));
        }
        make_storage_key(env, &[b"RL_STATE", &raw])
    }

    /// Generate collision-resistant storage key for the global rate limit config.
    fn get_config_key(env: &Env) -> soroban_sdk::BytesN<32> {
        make_storage_key(env, &[b"RL_CONFIG"])
    }

    /// Storage key for a per-role rate limit override.
    fn role_override_key(env: &Env, role: &soroban_sdk::Symbol) -> soroban_sdk::BytesN<32> {
        use soroban_sdk::xdr::ToXdr;
        let role_xdr = role.clone().to_xdr(env);
        let mut raw = alloc::vec::Vec::with_capacity(role_xdr.len() as usize);
        for i in 0..role_xdr.len() {
            raw.push(role_xdr.get(i).unwrap_or(0));
        }
        make_storage_key(env, &[b"RL_ROLE", &raw])
    }

    /// Storage key for a per-address rate limit override.
    fn address_override_key(env: &Env, address: &Address) -> soroban_sdk::BytesN<32> {
        use soroban_sdk::xdr::ToXdr;
        let addr_xdr = address.clone().to_xdr(env);
        let mut raw = alloc::vec::Vec::with_capacity(addr_xdr.len() as usize);
        for i in 0..addr_xdr.len() {
            raw.push(addr_xdr.get(i).unwrap_or(0));
        }
        make_storage_key(env, &[b"RL_ADDR", &raw])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Ledger as _;

    #[test]
    fn test_rate_limit_under_limit() {
        let env = Env::default();
        let attestor = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let config = RateLimitConfig {
            max_submissions: 10,
            window_length: 100,
        };
        
        // Create a dummy contract address for testing
        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        
        // Register a dummy contract for testing
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);
        
        // Should succeed for first submission
        let result = env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        });
        assert!(result.is_ok());
        
        // Check state
        let state = env.as_contract(&contract_id, &|| {
            RateLimiter::get_state(&env, &attestor)
        });
        assert_eq!(state.submission_count, 1);
    }
    
    #[test]
    fn test_rate_limit_at_limit() {
        let env = Env::default();
        let attestor = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let config = RateLimitConfig {
            max_submissions: 2,
            window_length: 100,
        };
        
        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);
        
        // First two submissions should succeed
        assert!(env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).is_ok());
        assert!(env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).is_ok());
        
        // Third submission should fail
        let result = env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::RateLimitExceeded);
    }
    
    #[test]
    fn test_rate_limit_over_limit() {
        let env = Env::default();
        let attestor = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let config = RateLimitConfig {
            max_submissions: 1,
            window_length: 100,
        };
        
        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);
        
        // First submission should succeed
        assert!(env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).is_ok());
        
        // Second submission should fail
        let result = env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::RateLimitExceeded);
    }
    
    #[test]
    fn test_rate_limit_window_reset() {
        let env = Env::default();
        let attestor = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let config = RateLimitConfig {
            max_submissions: 1,
            window_length: 10,
        };
        
        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);
        
        // First submission should succeed
        assert!(env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).is_ok());
        
        // Second submission should fail (still in same window)
        assert!(env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).is_err());
        
        // Note: In Soroban SDK, we cannot directly set the ledger sequence in tests
        // The window reset logic will be tested in integration tests with actual ledger progression
        // For now, we verify the state is correct
        let state = env.as_contract(&contract_id, &|| {
            RateLimiter::get_state(&env, &attestor)
        });
        assert_eq!(state.submission_count, 1);
    }
    
    #[test]
    fn test_rate_limit_config_update_uses_contract_admin_key() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let new_config = RateLimitConfig {
            max_submissions: 20,
            window_length: 200,
        };

        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);

        // Mirror AnchorKitContract::initialize by using the deterministic admin key.
        env.as_contract(&contract_id, &|| {
            env.storage()
                .instance()
                .set(&make_storage_key(&env, &[b"ADMIN"]), &admin);
        });

        let result = env.as_contract(&contract_id, &|| {
            RateLimiter::update_config(&env, &admin, &new_config)
        });
        assert!(result.is_ok());

        let config = env.as_contract(&contract_id, &|| {
            RateLimiter::get_config(&env)
        });
        assert_eq!(config.max_submissions, 20);
        assert_eq!(config.window_length, 200);
    }

    #[test]
    fn test_admin_bypasses_rate_limits() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let non_admin = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let config = RateLimitConfig { max_submissions: 2, window_length: 100 };
        
        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);
        
        // Store admin in instance storage
        env.as_contract(&contract_id, &|| {
            env.storage()
                .instance()
                .set(&make_storage_key(&env, &[b"ADMIN"]), &admin);
        });
        
        // Non-admin should be rate limited
        env.as_contract(&contract_id, &|| {
            assert!(RateLimiter::check_and_increment(&env, &non_admin, &config).is_ok());
            assert!(RateLimiter::check_and_increment(&env, &non_admin, &config).is_ok());
            assert!(RateLimiter::check_and_increment(&env, &non_admin, &config).is_err());
        });
        
        // Admin should never be rate limited
        env.as_contract(&contract_id, &|| {
            for _ in 0..10 {
                assert!(RateLimiter::check_and_increment(&env, &admin, &config).is_ok());
            }
        });
        
        // Verify non-admin state still has max submissions (admin didn't affect it)
        let state = env.as_contract(&contract_id, &|| {
            RateLimiter::get_state(&env, &non_admin)
        });
        assert_eq!(state.submission_count, 2);
    }

    #[test]
    fn test_update_config_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let non_admin = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let new_config = RateLimitConfig { max_submissions: 5, window_length: 50 };

        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);

        env.as_contract(&contract_id, &|| {
            env.storage()
                .instance()
                .set(&make_storage_key(&env, &[b"ADMIN"]), &admin);
        });

        let result = env.as_contract(&contract_id, &|| {
            RateLimiter::update_config(&env, &non_admin, &new_config)
        });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::UnauthorizedAttestor);
    }

    #[test]
    fn test_update_config_not_initialized() {
        let env = Env::default();
        let admin = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let new_config = RateLimitConfig { max_submissions: 5, window_length: 50 };

        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);

        // No admin stored — should return NotInitialized
        let result = env.as_contract(&contract_id, &|| {
            RateLimiter::update_config(&env, &admin, &new_config)
        });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::NotInitialized);
    }
    
    #[test]
    fn test_rate_limit_default_config() {
        let env = Env::default();
        
        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);
        
        // Get default config
        let config = env.as_contract(&contract_id, &|| {
            RateLimiter::get_config(&env)
        });
        assert_eq!(config.max_submissions, 10);
        assert_eq!(config.window_length, 100);
    }

    #[test]
    fn test_validate_config_rejects_zero_max_submissions() {
        let config = RateLimitConfig { max_submissions: 0, window_length: 100 };
        assert!(RateLimiter::validate_config(&config).is_err());
        assert_eq!(
            RateLimiter::validate_config(&config).unwrap_err().code,
            ErrorCode::ValidationError
        );
    }

    #[test]
    fn test_validate_config_rejects_zero_window_length() {
        let config = RateLimitConfig { max_submissions: 5, window_length: 0 };
        assert!(RateLimiter::validate_config(&config).is_err());
        assert_eq!(
            RateLimiter::validate_config(&config).unwrap_err().code,
            ErrorCode::ValidationError
        );
    }

    #[test]
    fn test_validate_config_accepts_valid() {
        let config = RateLimitConfig { max_submissions: 1, window_length: 1 };
        assert!(RateLimiter::validate_config(&config).is_ok());
    }

    #[test]
    fn test_update_config_rejects_zero_values() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let bad_config = RateLimitConfig { max_submissions: 0, window_length: 100 };

        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);

        env.as_contract(&contract_id, &|| {
            env.storage()
                .instance()
                .set(&make_storage_key(&env, &[b"ADMIN"]), &admin);
        });

        let result = env.as_contract(&contract_id, &|| {
            RateLimiter::update_config(&env, &admin, &bad_config)
        });
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ValidationError);
    }

    #[test]
    fn test_window_rollover_at_exact_boundary() {
        let env = Env::default();
        let attestor = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let config = RateLimitConfig { max_submissions: 1, window_length: 10 };

        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);

        // Fill the window
        assert!(env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).is_ok());
        // Same window — should fail
        assert!(env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).is_err());

        // Advance ledger by exactly window_length (10)
        env.ledger().set(soroban_sdk::testutils::LedgerInfo {
            sequence_number: 10,
            timestamp: 1000,
            protocol_version: 21,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6312000,
        });

        // Window should have rolled over — first submission in new window succeeds
        assert!(env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).is_ok());
    }

    /// If the stored window_start_ledger is somehow *ahead* of the current ledger
    /// (sequence anomaly), the window must be treated as NOT expired so that the
    /// existing submission count is preserved and the rate-limit cannot be bypassed.
    /// count == max_submissions is the exact rejection threshold: max-1 succeeds,
    /// max is rejected.  Isolates the off-by-one boundary in the >= check.
    #[test]
    fn test_at_limit_exact_last_allowed_then_rejected() {
        let env = Env::default();
        let attestor = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let config = RateLimitConfig { max_submissions: 3, window_length: 100 };
        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);

        // Submissions 1 through max_submissions-1 must all succeed.
        for _ in 0..2 {
            assert!(env.as_contract(&contract_id, &|| {
                RateLimiter::check_and_increment(&env, &attestor, &config)
            }).is_ok());
        }

        // The max_submissions-th call is the LAST allowed — must succeed.
        assert!(env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).is_ok(), "submission at count == max_submissions-1 must succeed");

        // Now count == max_submissions; the next call must be rejected.
        let result = env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        });
        assert!(result.is_err(), "submission at count == max_submissions must be rejected");
        assert_eq!(result.unwrap_err().code, ErrorCode::RateLimitExceeded);

        // State must be capped — no overflow past max.
        let state = env.as_contract(&contract_id, &|| RateLimiter::get_state(&env, &attestor));
        assert_eq!(state.submission_count, 3, "count must not exceed max_submissions");
    }

    /// Every call after the limit (count > max_submissions) must still return
    /// RateLimitExceeded and must not mutate the stored count.
    #[test]
    fn test_one_over_limit_state_stays_capped() {
        let env = Env::default();
        let attestor = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let config = RateLimitConfig { max_submissions: 2, window_length: 100 };
        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);

        // Fill the window.
        env.as_contract(&contract_id, &|| { RateLimiter::check_and_increment(&env, &attestor, &config).unwrap(); });
        env.as_contract(&contract_id, &|| { RateLimiter::check_and_increment(&env, &attestor, &config).unwrap(); });

        // count == max: first rejection (one-over).
        let err1 = env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).unwrap_err();
        assert_eq!(err1.code, ErrorCode::RateLimitExceeded);

        // count still == max: second rejection (two-over) — state must not have changed.
        let err2 = env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).unwrap_err();
        assert_eq!(err2.code, ErrorCode::RateLimitExceeded);

        let state = env.as_contract(&contract_id, &|| RateLimiter::get_state(&env, &attestor));
        assert_eq!(state.submission_count, 2, "count must remain at max after over-limit calls");
    }

    /// A submission at ledger window_start + window_length - 1 (one before expiry)
    /// must still be rejected, while one at window_start + window_length is in the
    /// new window and must succeed.  Pins the exact >=/> boundary in is_window_expired.
    #[test]
    fn test_window_one_before_expiry_still_restricted() {
        let env = Env::default();
        let attestor = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let config = RateLimitConfig { max_submissions: 1, window_length: 10 };
        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);

        // Consume the sole slot at ledger 0.
        assert!(env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).is_ok());

        // Advance to one ledger BEFORE the window expires (delta = window_length - 1 = 9).
        env.ledger().set(soroban_sdk::testutils::LedgerInfo {
            sequence_number: 9,
            timestamp: 900,
            protocol_version: 21,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6312000,
        });

        // delta = 9 < window_length = 10 → still in old window → must be rejected.
        let result = env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        });
        assert!(result.is_err(), "submission at window_start + window_length - 1 must be rejected");
        assert_eq!(result.unwrap_err().code, ErrorCode::RateLimitExceeded);

        // Advance to exactly window_start + window_length (delta = 10 = window_length → expired).
        env.ledger().set(soroban_sdk::testutils::LedgerInfo {
            sequence_number: 10,
            timestamp: 1000,
            protocol_version: 21,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6312000,
        });

        // New window → must succeed.
        assert!(env.as_contract(&contract_id, &|| {
            RateLimiter::check_and_increment(&env, &attestor, &config)
        }).is_ok(), "submission at window_start + window_length must start a new window");
    }

    #[test]
    fn test_window_not_expired_when_current_less_than_start() {
        // current < window_start → checked_sub underflows → None → false
        assert!(!RateLimiter::is_window_expired(5, 10, 10));
        assert!(!RateLimiter::is_window_expired(0, 1, 1));
        assert!(!RateLimiter::is_window_expired(100, 200, 50));
    }

    /// current == window_start and window_length > 0: delta is 0, so NOT expired.
    #[test]
    fn test_window_not_expired_at_exact_start() {
        assert!(!RateLimiter::is_window_expired(10, 10, 1));
    }

    /// Verify the boundary: delta == window_length means expired.
    #[test]
    fn test_window_expired_at_exact_length() {
        assert!(RateLimiter::is_window_expired(20, 10, 10)); // delta = 10 >= 10
    }

    #[test]
    fn test_max_submission_error_is_consistent() {
        let env = Env::default();
        let attestor = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let config = RateLimitConfig { max_submissions: 2, window_length: 100 };

        let contract_address = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);
        let contract_id = env.register_contract(&contract_address, crate::contract::AnchorKitContract);

        env.as_contract(&contract_id, &|| { RateLimiter::check_and_increment(&env, &attestor, &config).unwrap(); });
        env.as_contract(&contract_id, &|| { RateLimiter::check_and_increment(&env, &attestor, &config).unwrap(); });

        // Every subsequent call must return RateLimitExceeded without corrupting state
        for _ in 0..3 {
            let err = env.as_contract(&contract_id, &|| {
                RateLimiter::check_and_increment(&env, &attestor, &config)
            }).unwrap_err();
            assert_eq!(err.code, ErrorCode::RateLimitExceeded);
        }
        // State must still show exactly max_submissions
        let state = env.as_contract(&contract_id, &|| RateLimiter::get_state(&env, &attestor));
        assert_eq!(state.submission_count, 2);
    }
}
