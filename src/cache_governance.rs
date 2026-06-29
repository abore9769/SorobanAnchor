//! On-chain governance for stellar.toml capability cache invalidation.
//!
//! Any registered attestor can propose that a specific anchor's capability
//! cache entry be invalidated. Once a configurable quorum of distinct attestors
//! endorse the proposal the cache entry is automatically invalidated and a
//! `force_refresh` is triggered. Proposals expire after a configurable number
//! of ledgers.

use soroban_sdk::{contracttype, Address, Bytes, BytesN, Env, Vec, xdr::ToXdr};
use crate::deterministic_hash::make_storage_key;
use crate::errors::{AnchorKitError, ErrorCode};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default number of endorsements required to reach quorum.
pub const DEFAULT_QUORUM: u32 = 3;
/// Default proposal TTL: ~1 day at 5 s/ledger.
pub const DEFAULT_EXPIRY_LEDGERS: u32 = 17_280;

// ---------------------------------------------------------------------------
// Storage types
// ---------------------------------------------------------------------------

/// An on-chain cache invalidation proposal.
#[contracttype]
#[derive(Clone, Debug)]
pub struct CacheInvalidationProposal {
    /// Sequential proposal ID.
    pub proposal_id: u64,
    /// Anchor whose capability cache entry should be invalidated.
    pub anchor: Address,
    /// Attestor who created the proposal.
    pub proposer: Address,
    /// Addresses that have endorsed this proposal (no duplicates).
    pub endorsements: Vec<Address>,
    /// Ledger sequence number when the proposal was created.
    pub created_at_ledger: u32,
    /// Number of ledgers after creation before the proposal expires.
    pub expiry_ledgers: u32,
    /// Whether the proposal has already been executed.
    pub executed: bool,
}

/// Governance configuration for cache invalidation proposals.
#[contracttype]
#[derive(Clone, Debug)]
pub struct CacheGovernanceConfig {
    /// Number of endorsements required to reach quorum.
    pub quorum_threshold: u32,
    /// Proposal TTL in ledgers.
    pub proposal_expiry_ledgers: u32,
}

impl CacheGovernanceConfig {
    pub fn default_config() -> Self {
        CacheGovernanceConfig {
            quorum_threshold: DEFAULT_QUORUM,
            proposal_expiry_ledgers: DEFAULT_EXPIRY_LEDGERS,
        }
    }
}

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

fn config_key(env: &Env) -> BytesN<32> {
    make_storage_key(env, &[b"CGOV_CFG"])
}

fn proposal_count_key(env: &Env) -> BytesN<32> {
    make_storage_key(env, &[b"CGOV_CNT"])
}

fn proposal_key(env: &Env, proposal_id: u64) -> BytesN<32> {
    make_storage_key(env, &[b"CGOV_PROP", &proposal_id.to_be_bytes()])
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Read the governance configuration, returning the default if not set.
pub fn get_config(env: &Env) -> CacheGovernanceConfig {
    env.storage()
        .persistent()
        .get::<_, CacheGovernanceConfig>(&config_key(env))
        .unwrap_or_else(CacheGovernanceConfig::default_config)
}

/// Persist updated governance configuration (admin-only enforcement is in the
/// contract layer).
pub fn set_config(env: &Env, config: CacheGovernanceConfig) {
    env.storage().persistent().set(&config_key(env), &config);
}

/// Create a new cache invalidation proposal.
///
/// Returns the new `proposal_id`. The `proposer` must be a registered
/// attestor; that check is enforced by the contract layer.
pub fn propose(env: &Env, proposer: &Address, anchor: &Address) -> u64 {
    let cfg = get_config(env);
    let proposal_id: u64 = env
        .storage()
        .persistent()
        .get::<_, u64>(&proposal_count_key(env))
        .unwrap_or(0);

    // Build the initial endorsements list with the proposer as first endorser.
    let mut endorsements = Vec::new(env);
    endorsements.push_back(proposer.clone());

    let proposal = CacheInvalidationProposal {
        proposal_id,
        anchor: anchor.clone(),
        proposer: proposer.clone(),
        endorsements,
        created_at_ledger: env.ledger().sequence(),
        expiry_ledgers: cfg.proposal_expiry_ledgers,
        executed: false,
    };

    env.storage()
        .persistent()
        .set(&proposal_key(env, proposal_id), &proposal);
    env.storage()
        .persistent()
        .set(&proposal_count_key(env), &(proposal_id + 1));

    proposal_id
}

/// Add an endorsement to an existing proposal.
///
/// Duplicate endorsements from the same address are silently ignored.
/// Returns `Err(NotFound)` when the proposal does not exist.
/// Returns `Err(StaleQuote)` when the proposal is expired.
pub fn endorse(env: &Env, endorser: &Address, proposal_id: u64) -> Result<(), AnchorKitError> {
    let key = proposal_key(env, proposal_id);
    let mut proposal = env
        .storage()
        .persistent()
        .get::<_, CacheInvalidationProposal>(&key)
        .ok_or_else(AnchorKitError::cache_not_found)?;

    if is_expired(env, &proposal) {
        return Err(AnchorKitError::new(ErrorCode::StaleQuote, "proposal expired"));
    }
    if proposal.executed {
        return Err(AnchorKitError::validation_error("proposal already executed"));
    }

    // Deduplication: ignore if already endorsed by this address.
    for i in 0..proposal.endorsements.len() {
        if proposal.endorsements.get(i).unwrap() == *endorser {
            return Ok(());
        }
    }

    proposal.endorsements.push_back(endorser.clone());
    env.storage().persistent().set(&key, &proposal);
    Ok(())
}

/// Execute a proposal that has reached quorum.
///
/// Callable by anyone once quorum is met. Returns the anchor address whose
/// cache was invalidated so the contract layer can call `force_refresh_metadata`.
///
/// Returns `Err(StaleQuote)` when expired, `Err(ValidationError)` when quorum
/// has not been reached, `Err(NotFound)` when the proposal does not exist.
pub fn execute(env: &Env, proposal_id: u64) -> Result<Address, AnchorKitError> {
    let key = proposal_key(env, proposal_id);
    let mut proposal = env
        .storage()
        .persistent()
        .get::<_, CacheInvalidationProposal>(&key)
        .ok_or_else(AnchorKitError::cache_not_found)?;

    if is_expired(env, &proposal) {
        return Err(AnchorKitError::new(ErrorCode::StaleQuote, "proposal expired"));
    }
    if proposal.executed {
        return Err(AnchorKitError::validation_error("proposal already executed"));
    }

    let cfg = get_config(env);
    if proposal.endorsements.len() < cfg.quorum_threshold {
        return Err(AnchorKitError::validation_error("quorum not reached"));
    }

    proposal.executed = true;
    env.storage().persistent().set(&key, &proposal);

    Ok(proposal.anchor.clone())
}

/// Read a proposal by ID. Returns `None` if it does not exist.
pub fn get_proposal(env: &Env, proposal_id: u64) -> Option<CacheInvalidationProposal> {
    env.storage()
        .persistent()
        .get::<_, CacheInvalidationProposal>(&proposal_key(env, proposal_id))
}

/// Total number of proposals ever created.
pub fn proposal_count(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get::<_, u64>(&proposal_count_key(env))
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn is_expired(env: &Env, proposal: &CacheInvalidationProposal) -> bool {
    let current = env.ledger().sequence();
    let expiry = proposal
        .created_at_ledger
        .saturating_add(proposal.expiry_ledgers);
    current >= expiry
}
