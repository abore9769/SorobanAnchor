use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec};

use crate::errors::AnchorKitError;

const TXSTATE_TTL: u32 = 1_555_200;

/// Transaction states for the state tracker
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TransactionState {
    Pending = 1,
    InProgress = 2,
    Completed = 3,
    Failed = 4,
}

impl TransactionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransactionState::Pending => "pending",
            TransactionState::InProgress => "in_progress",
            TransactionState::Completed => "completed",
            TransactionState::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(TransactionState::Pending),
            "in_progress" => Some(TransactionState::InProgress),
            "completed" => Some(TransactionState::Completed),
            "failed" => Some(TransactionState::Failed),
            _ => None,
        }
    }

    /// Returns true only for legal forward transitions:
    /// Pending → InProgress, InProgress → Completed, InProgress → Failed
    pub fn is_valid_transition(&self, to: TransactionState) -> bool {
        matches!(
            (self, to),
            (TransactionState::Pending, TransactionState::InProgress)
                | (TransactionState::InProgress, TransactionState::Completed)
                | (TransactionState::InProgress, TransactionState::Failed)
        )
    }
}

/// Transaction state record
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionStateRecord {
    pub transaction_id: u64,
    pub state: TransactionState,
    pub initiator: Address,
    pub timestamp: u64,
    pub last_updated: u64,
    pub error_message: Option<String>,
    /// Full state progression: (state, timestamp) pairs in chronological order.
    pub state_history: Vec<(TransactionState, u64)>,
}

/// Audit entry for a single transition attempt (success or failure).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionAuditEntry {
    pub transaction_id: u64,
    pub from_state: TransactionState,
    pub to_state: TransactionState,
    pub timestamp: u64,
    pub success: bool,
}

/// Transaction state tracker
#[derive(Clone)]
pub struct TransactionStateTracker {
    cache: alloc::vec::Vec<TransactionStateRecord>,
    pub audit_log: alloc::vec::Vec<TransitionAuditEntry>,
    is_dev_mode: bool,
}

impl TransactionStateTracker {
    /// Create a new transaction state tracker
    pub fn new(is_dev_mode: bool) -> Self {
        TransactionStateTracker {
            cache: alloc::vec::Vec::new(),
            audit_log: alloc::vec::Vec::new(),
            is_dev_mode,
        }
    }

    /// Create a transaction with pending state
    pub fn create_transaction(
        &mut self,
        transaction_id: u64,
        initiator: Address,
        env: &Env,
    ) -> Result<TransactionStateRecord, String> {
        let current_time = env.ledger().timestamp();
        let mut history = Vec::new(env);
        history.push_back((TransactionState::Pending, current_time));

        let record = TransactionStateRecord {
            transaction_id,
            state: TransactionState::Pending,
            initiator,
            timestamp: current_time,
            last_updated: current_time,
            error_message: None,
            state_history: history,
        };

        if self.is_dev_mode {
            self.cache.push(record.clone());
        } else {
            let key = (symbol_short!("TXSTATE"), transaction_id);
            env.storage().persistent().set(&key, &record);
            env.storage().persistent().extend_ttl(&key, TXSTATE_TTL, TXSTATE_TTL);
        }

        Ok(record)
    }

    /// Update transaction state to in-progress
    pub fn start_transaction(
        &mut self,
        transaction_id: u64,
        env: &Env,
    ) -> Result<TransactionStateRecord, String> {
        self.update_state(transaction_id, TransactionState::InProgress, None, env)
    }

    /// Mark transaction as completed
    pub fn complete_transaction(
        &mut self,
        transaction_id: u64,
        env: &Env,
    ) -> Result<TransactionStateRecord, String> {
        self.update_state(transaction_id, TransactionState::Completed, None, env)
    }

    /// Mark transaction as failed
    pub fn fail_transaction(
        &mut self,
        transaction_id: u64,
        error_message: String,
        env: &Env,
    ) -> Result<TransactionStateRecord, String> {
        self.update_state(
            transaction_id,
            TransactionState::Failed,
            Some(error_message),
            env,
        )
    }

    /// Update transaction state
    fn update_state(
        &mut self,
        transaction_id: u64,
        new_state: TransactionState,
        error_message: Option<String>,
        env: &Env,
    ) -> Result<TransactionStateRecord, String> {
        let current_time = env.ledger().timestamp();

        if self.is_dev_mode {
            for record in self.cache.iter_mut() {
                if record.transaction_id == transaction_id {
                    let from_state = record.state;
                    let valid = from_state.is_valid_transition(new_state);
                    self.audit_log.push(TransitionAuditEntry {
                        transaction_id,
                        from_state,
                        to_state: new_state,
                        timestamp: current_time,
                        success: valid,
                    });
                    if !valid {
                        return Err(String::from_str(
                            env,
                            AnchorKitError::illegal_transition(
                                from_state.as_str(),
                                new_state.as_str(),
                            )
                            .message
                            .as_str(),
                        ));
                    }
                    record.state = new_state;
                    record.last_updated = current_time;
                    record.error_message = error_message;
                    record.state_history.push_back((new_state, current_time));
                    return Ok(record.clone());
                }
            }
            return Err(String::from_str(env, "Transaction not found in cache"));
        } else {
            let key = (symbol_short!("TXSTATE"), transaction_id);
            let mut record: TransactionStateRecord = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or_else(|| String::from_str(env, "Transaction not found"))?;

            let from_state = record.state;
            let valid = from_state.is_valid_transition(new_state);

            // Write audit entry to persistent storage
            let audit_cnt_key = (symbol_short!("TXAUDIT"), transaction_id);
            let audit_idx: u64 = env
                .storage()
                .persistent()
                .get(&audit_cnt_key)
                .unwrap_or(0u64);
            let audit_entry_key = (symbol_short!("TXAUDITK"), transaction_id, audit_idx);
            env.storage().persistent().set(
                &audit_entry_key,
                &(from_state as u32, new_state as u32, current_time, valid),
            );
            env.storage()
                .persistent()
                .extend_ttl(&audit_entry_key, TXSTATE_TTL, TXSTATE_TTL);
            env.storage()
                .persistent()
                .set(&audit_cnt_key, &(audit_idx + 1));
            env.storage()
                .persistent()
                .extend_ttl(&audit_cnt_key, TXSTATE_TTL, TXSTATE_TTL);

            if !valid {
                return Err(String::from_str(
                    env,
                    AnchorKitError::illegal_transition(
                        from_state.as_str(),
                        new_state.as_str(),
                    )
                    .message
                    .as_str(),
                ));
            }

            record.state = new_state;
            record.last_updated = current_time;
            record.error_message = error_message;
            record.state_history.push_back((new_state, current_time));

            env.storage().persistent().set(&key, &record);
            env.storage()
                .persistent()
                .extend_ttl(&key, TXSTATE_TTL, TXSTATE_TTL);

            Ok(record)
        }
    }

    /// Advance a transaction to `new_state`, enforcing legal transition rules.
    /// Returns an error if the transition is illegal or the transaction is not found.
    pub fn advance_transaction_state(
        &mut self,
        transaction_id: u64,
        new_state: TransactionState,
        env: &Env,
    ) -> Result<TransactionStateRecord, String> {
        self.update_state(transaction_id, new_state, None, env)
    }

    /// Get transaction state by ID
    pub fn get_transaction_state(
        &self,
        transaction_id: u64,
        env: &Env,
    ) -> Result<Option<TransactionStateRecord>, String> {
        if self.is_dev_mode {
            for record in self.cache.iter() {
                if record.transaction_id == transaction_id {
                    return Ok(Some(record.clone()));
                }
            }
            Ok(None)
        } else {
            Ok(env
                .storage()
                .persistent()
                .get(&(symbol_short!("TXSTATE"), transaction_id)))
        }
    }

    /// Get all transactions in a specific state
    pub fn get_transactions_by_state(
        &self,
        state: TransactionState,
    ) -> Result<alloc::vec::Vec<TransactionStateRecord>, String> {
        if self.is_dev_mode {
            let mut result = alloc::vec::Vec::new();
            for record in self.cache.iter() {
                if record.state == state {
                    result.push(record.clone());
                }
            }
            Ok(result)
        } else {
            Ok(alloc::vec::Vec::new())
        }
    }

    /// Get all transactions
    pub fn get_all_transactions(&self) -> Result<alloc::vec::Vec<TransactionStateRecord>, String> {
        if self.is_dev_mode {
            Ok(self.cache.clone())
        } else {
            Ok(alloc::vec::Vec::new())
        }
    }

    /// Clear all cached transactions (dev mode only)
    pub fn clear_cache(&mut self) -> Result<(), String> {
        if self.is_dev_mode {
            self.cache = alloc::vec::Vec::new();
            Ok(())
        } else {
            Err("Cannot clear cache in production mode".into())
        }
    }

    /// Get cache size
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;
    use soroban_sdk::testutils::Address;

    #[test]
    fn test_create_transaction() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        let result = tracker.create_transaction(1, initiator.clone(), &env);
        assert!(result.is_ok());

        let record = result.unwrap();
        assert_eq!(record.transaction_id, 1);
        assert_eq!(record.state, TransactionState::Pending);
        assert_eq!(record.initiator, initiator);
        // state_history initialized with Pending
        assert_eq!(record.state_history.len(), 1);
        assert_eq!(record.state_history.get(0).unwrap().0, TransactionState::Pending);
    }

    #[test]
    fn test_start_transaction() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        let result = tracker.start_transaction(1, &env);

        assert!(result.is_ok());
        let record = result.unwrap();
        assert_eq!(record.state, TransactionState::InProgress);
        assert_eq!(record.state_history.len(), 2);
    }

    #[test]
    fn test_complete_transaction() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();
        let result = tracker.complete_transaction(1, &env);

        assert!(result.is_ok());
        let record = result.unwrap();
        assert_eq!(record.state, TransactionState::Completed);
        assert_eq!(record.state_history.len(), 3);
    }

    #[test]
    fn test_fail_transaction() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        let error_msg = String::from_str(&env, "Test error");
        let result = tracker.fail_transaction(1, error_msg, &env);

        assert!(result.is_ok());
        let record = result.unwrap();
        assert_eq!(record.state, TransactionState::Failed);
        assert!(record.error_message.is_some());
    }

    #[test]
    fn test_get_transaction_state() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        let result = tracker.get_transaction_state(1, &env);

        assert!(result.is_ok());
        let state = result.unwrap();
        assert!(state.is_some());
        assert_eq!(state.unwrap().state, TransactionState::Pending);
    }

    #[test]
    fn test_get_transactions_by_state() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.create_transaction(2, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();

        let result = tracker.get_transactions_by_state(TransactionState::Pending);
        assert!(result.is_ok());
        let transactions = result.unwrap();
        assert_eq!(transactions.len(), 1);
    }

    #[test]
    fn test_get_all_transactions() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.create_transaction(2, initiator.clone(), &env).ok();

        let result = tracker.get_all_transactions();
        assert!(result.is_ok());
        let transactions = result.unwrap();
        assert_eq!(transactions.len(), 2);
    }

    #[test]
    fn test_cache_size() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.create_transaction(2, initiator.clone(), &env).ok();

        assert_eq!(tracker.cache_size(), 2);
    }

    #[test]
    fn test_clear_cache() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        let clear_result = tracker.clear_cache();

        assert!(clear_result.is_ok());
        assert_eq!(tracker.cache_size(), 0);
    }

    #[test]
    fn test_is_valid_transition() {
        assert!(TransactionState::Pending.is_valid_transition(TransactionState::InProgress));
        assert!(TransactionState::InProgress.is_valid_transition(TransactionState::Completed));
        assert!(TransactionState::InProgress.is_valid_transition(TransactionState::Failed));

        assert!(!TransactionState::Pending.is_valid_transition(TransactionState::Completed));
        assert!(!TransactionState::Pending.is_valid_transition(TransactionState::Failed));
        assert!(!TransactionState::Completed.is_valid_transition(TransactionState::InProgress));
        assert!(!TransactionState::Failed.is_valid_transition(TransactionState::InProgress));
        assert!(!TransactionState::Completed.is_valid_transition(TransactionState::Pending));
    }

    #[test]
    fn test_advance_transaction_state_legal() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();

        let r = tracker.advance_transaction_state(1, TransactionState::InProgress, &env);
        assert!(r.is_ok());
        assert_eq!(r.unwrap().state, TransactionState::InProgress);

        let r = tracker.advance_transaction_state(1, TransactionState::Completed, &env);
        assert!(r.is_ok());
        assert_eq!(r.unwrap().state, TransactionState::Completed);
    }

    #[test]
    fn test_advance_transaction_state_illegal() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.advance_transaction_state(1, TransactionState::InProgress, &env).ok();
        tracker.advance_transaction_state(1, TransactionState::Completed, &env).ok();

        // Completed → InProgress must be rejected
        let r = tracker.advance_transaction_state(1, TransactionState::InProgress, &env);
        assert!(r.is_err());
    }

    // -----------------------------------------------------------------------
    // Backward / same-state transition guard
    // -----------------------------------------------------------------------

    #[test]
    fn test_backward_transition_completed_to_pending_rejected() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();
        tracker.complete_transaction(1, &env).ok();

        let r = tracker.advance_transaction_state(1, TransactionState::Pending, &env);
        assert!(r.is_err());
    }

    #[test]
    fn test_backward_transition_failed_to_in_progress_rejected() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();
        tracker.fail_transaction(1, String::from_str(&env, "err"), &env).ok();

        let r = tracker.advance_transaction_state(1, TransactionState::InProgress, &env);
        assert!(r.is_err());
    }

    #[test]
    fn test_same_state_transition_rejected() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();

        // Pending → Pending is not a valid transition
        let r = tracker.advance_transaction_state(1, TransactionState::Pending, &env);
        assert!(r.is_err());
    }

    #[test]
    fn test_pending_to_completed_directly_rejected() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();

        let r = tracker.advance_transaction_state(1, TransactionState::Completed, &env);
        assert!(r.is_err());
    }

    // -----------------------------------------------------------------------
    // Audit log entries for success and failure
    // -----------------------------------------------------------------------

    #[test]
    fn test_audit_log_entry_on_successful_transition() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();

        assert_eq!(tracker.audit_log.len(), 1);
        let entry = &tracker.audit_log[0];
        assert_eq!(entry.transaction_id, 1);
        assert_eq!(entry.from_state, TransactionState::Pending);
        assert_eq!(entry.to_state, TransactionState::InProgress);
        assert!(entry.success);
    }

    #[test]
    fn test_audit_log_entry_on_failed_transition() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();
        tracker.complete_transaction(1, &env).ok();

        // Illegal: Completed → Pending
        let _ = tracker.advance_transaction_state(1, TransactionState::Pending, &env);

        // 2 successful + 1 failed
        assert_eq!(tracker.audit_log.len(), 3);
        let failed_entry = &tracker.audit_log[2];
        assert_eq!(failed_entry.from_state, TransactionState::Completed);
        assert_eq!(failed_entry.to_state, TransactionState::Pending);
        assert!(!failed_entry.success);
    }

    #[test]
    fn test_audit_log_records_all_transitions() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();
        tracker.complete_transaction(1, &env).ok();

        assert_eq!(tracker.audit_log.len(), 2);
        assert!(tracker.audit_log[0].success);
        assert!(tracker.audit_log[1].success);
    }

    // -----------------------------------------------------------------------
    // state_history accuracy
    // -----------------------------------------------------------------------

    #[test]
    fn test_state_history_reflects_full_progression() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();
        tracker.complete_transaction(1, &env).ok();

        let record = tracker.get_transaction_state(1, &env).unwrap().unwrap();
        assert_eq!(record.state_history.len(), 3);
        assert_eq!(record.state_history.get(0).unwrap().0, TransactionState::Pending);
        assert_eq!(record.state_history.get(1).unwrap().0, TransactionState::InProgress);
        assert_eq!(record.state_history.get(2).unwrap().0, TransactionState::Completed);
    }

    #[test]
    fn test_state_history_not_updated_on_illegal_transition() {
        let env = Env::default();
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).ok();
        tracker.start_transaction(1, &env).ok();
        // Illegal: InProgress → Pending
        let _ = tracker.advance_transaction_state(1, TransactionState::Pending, &env);

        let record = tracker.get_transaction_state(1, &env).unwrap().unwrap();
        // Only Pending + InProgress — illegal attempt must not append
        assert_eq!(record.state_history.len(), 2);
        assert_eq!(record.state.as_str(), "in_progress");
    }
}
