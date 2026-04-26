use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env, String,
    Symbol, Vec,
};

use crate::deterministic_hash::{compute_payload_hash, verify_payload_hash};
use crate::errors::ErrorCode;
use crate::rate_limiter::RateLimiter;
use crate::sep10_jwt;
use crate::transaction_state_tracker::{TransactionState, TransactionStateRecord};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub struct Session {
    pub session_id: u64,
    pub initiator: Address,
    pub created_at: u64,
    pub nonce: u64,
    pub operation_count: u64,
 feat/session-expiry-check
    pub session_ttl_seconds: u64,

    pub closed: bool,
 main
}

#[contracttype]
#[derive(Clone)]
pub struct Quote {
    pub quote_id: u64,
    pub anchor: Address,
    pub base_asset: String,
    pub quote_asset: String,
    pub rate: u64,
    pub fee_percentage: u32,
    pub minimum_amount: u64,
    pub maximum_amount: u64,
    pub valid_until: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct OperationContext {
    pub session_id: u64,
    pub operation_index: u64,
    pub operation_type: String,
    pub timestamp: u64,
    pub status: String,
    pub result_data: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct AuditLog {
    pub log_id: u64,
    pub session_id: u64,
    pub actor: Address,
    pub operation: OperationContext,
}

#[contracttype]
#[derive(Clone)]
pub struct RequestId {
    pub id: Bytes,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct Attestation {
    pub id: u64,
    pub issuer: Address,
    pub subject: Address,
    pub timestamp: u64,
    pub payload_hash: Bytes,
    pub signature: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub struct TracingSpan {
    pub request_id: RequestId,
    pub operation: String,
    pub actor: Address,
    pub started_at: u64,
    pub completed_at: u64,
    pub status: String,
}

#[contracttype]
#[derive(Clone)]
pub struct AnchorServices {
    pub anchor: Address,
    pub services: Vec<u32>,
}

pub const SERVICE_DEPOSITS: u32 = 1;
pub const SERVICE_WITHDRAWALS: u32 = 2;
pub const SERVICE_QUOTES: u32 = 3;
pub const SERVICE_KYC: u32 = 4;

/// Typed representation of a service capability an anchor can support.
///
/// Each variant maps to a stable `u32` discriminant stored on-chain.
/// Use [`ServiceType::as_u32`] to convert before passing to contract functions.
#[derive(Clone, PartialEq)]
pub enum ServiceType {
    Deposits,
    Withdrawals,
    Quotes,
    KYC,
}

impl ServiceType {
    pub fn as_u32(&self) -> u32 {
        match self {
            ServiceType::Deposits => SERVICE_DEPOSITS,
            ServiceType::Withdrawals => SERVICE_WITHDRAWALS,
            ServiceType::Quotes => SERVICE_QUOTES,
            ServiceType::KYC => SERVICE_KYC,
        }
    }
}

// ---------------------------------------------------------------------------
// Routing types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub struct RoutingAnchorMeta {
    pub anchor: Address,
    pub reputation_score: u32,
    pub average_settlement_time: u64,
    pub liquidity_score: u32,
    pub uptime_percentage: u32,
    pub total_volume: u64,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct RoutingRequest {
    pub base_asset: String,
    pub quote_asset: String,
    pub amount: u64,
    pub operation_type: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct RoutingOptions {
    pub request: RoutingRequest,
    pub strategy: Vec<Symbol>,
    pub min_reputation: u32,
    pub max_anchors: u32,
    pub require_kyc: bool,
    pub require_compliance: bool,
    pub subject: Address,
}

/// Composite weighted routing strategy.
/// `fee_weight + speed_weight + reputation_weight` must equal 1.0.
pub struct WeightedRoutingStrategy {
    pub fee_weight: f32,
    pub speed_weight: f32,
    pub reputation_weight: f32,
}

impl WeightedRoutingStrategy {
    /// Validate that weights sum to 1.0 (within floating-point tolerance).
    pub fn validate(&self) -> bool {
        let sum = self.fee_weight + self.speed_weight + self.reputation_weight;
        (sum - 1.0_f32).abs() < 1e-4
    }

    /// Compute a normalized composite score in [0.0, 1.0].
    /// Lower fee and faster settlement are better; higher reputation is better.
    /// Each dimension is normalised against the provided max values.
    pub fn score_anchor(
        &self,
        fee_pct: u32,
        settlement_time: u64,
        reputation: u32,
        max_fee: u32,
        max_settlement: u64,
        max_reputation: u32,
    ) -> f32 {
        let fee_score = if max_fee == 0 {
            1.0_f32
        } else {
            1.0_f32 - (fee_pct as f32 / max_fee as f32)
        };
        let speed_score = if max_settlement == 0 {
            1.0_f32
        } else {
            1.0_f32 - (settlement_time as f32 / max_settlement as f32)
        };
        let rep_score = if max_reputation == 0 {
            0.0_f32
        } else {
            reputation as f32 / max_reputation as f32
        };
        self.fee_weight * fee_score
            + self.speed_weight * speed_score
            + self.reputation_weight * rep_score
    }
}

// ---------------------------------------------------------------------------
// KYC and Compliance types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum KycStatus {
    NotSubmitted = 0,
    Pending = 1,
    Approved = 2,
    Rejected = 3,
    Expired = 4,
}

#[contracttype]
#[derive(Clone)]
pub struct ComplianceCheck {
    pub subject: Address,
    pub check_type: String,
    pub result: u32,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct KycRecord {
    pub subject: Address,
    pub status: u32,
    pub submitted_at: u64,
    pub reviewed_at: Option<u64>,
    pub expiry: Option<u64>,
    pub rejection_reason_hash: Option<Bytes>,
}

// ---------------------------------------------------------------------------
// Metadata cache types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub struct AnchorMetadata {
    pub anchor: Address,
    pub reputation_score: u32,
    pub liquidity_score: u32,
    pub uptime_percentage: u32,
    pub total_volume: u64,
    pub average_settlement_time: u64,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct MetadataCache {
    pub metadata: AnchorMetadata,
    pub cached_at: u64,
    pub ttl_seconds: u64,
    /// Grace period after `ttl_seconds` during which stale data may be served.
    pub stale_ttl_seconds: u64,
    /// Set to `true` when the entry is within the stale window; caller should refresh.
    pub needs_refresh: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct CapabilitiesCache {
    pub toml_url: String,
    pub capabilities: String,
    pub cached_at: u64,
    pub ttl_seconds: u64,
}

// ---------------------------------------------------------------------------
// Anchor Info Discovery types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub struct AssetInfo {
    pub code: String,
    pub issuer: String,
    pub deposit_enabled: bool,
    pub withdrawal_enabled: bool,
    pub deposit_fee_fixed: u64,
    pub deposit_fee_percent: u32,
    pub withdrawal_fee_fixed: u64,
    pub withdrawal_fee_percent: u32,
    pub deposit_min_amount: u64,
    pub deposit_max_amount: u64,
    pub withdrawal_min_amount: u64,
    pub withdrawal_max_amount: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct StellarToml {
    pub version: String,
    pub network_passphrase: String,
    pub accounts: Vec<String>,
    pub signing_key: String,
    pub currencies: Vec<AssetInfo>,
    pub transfer_server: String,
    pub transfer_server_sep0024: String,
    pub kyc_server: String,
    pub web_auth_endpoint: String,
}

#[contracttype]
#[derive(Clone)]
pub struct CachedToml {
    pub toml: StellarToml,
    pub cached_at: u64,
    pub ttl_seconds: u64,
}

const MIN_TEMP_TTL: u32 = 15; // min_temp_entry_ttl - 1

// ---------------------------------------------------------------------------
// Event structs
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
struct SessionCreatedEvent {
    session_id: u64,
    initiator: Address,
    timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
struct SessionClosedEvent {
    session_id: u64,
    initiator: Address,
    timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
struct QuoteSubmitEvent {
    quote_id: u64,
    anchor: Address,
    base_asset: String,
    quote_asset: String,
    rate: u64,
    valid_until: u64,
}

#[contracttype]
#[derive(Clone)]
struct QuoteReceivedEvent {
    quote_id: u64,
    receiver: Address,
    timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
struct AuditLogEvent {
    log_id: u64,
    session_id: u64,
    operation_index: u64,
    operation_type: String,
    status: String,
}

#[contracttype]
#[derive(Clone)]
struct AttestEvent {
    payload_hash: Bytes,
    timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct EndpointUpdated {
    pub attestor: Address,
    pub endpoint: String,
}

#[contracttype]
#[derive(Clone)]
struct TxStateChangedEvent {
    transaction_id: u64,
    old_state: u32,
    new_state: u32,
    timestamp: u64,
}

// ---------------------------------------------------------------------------
// TTLs (in ledgers)
// ---------------------------------------------------------------------------
const PERSISTENT_TTL: u32 = 1_555_200;
const SPAN_TTL: u32 = 17_280;
const INSTANCE_TTL: u32 = 518_400;

/// Default session lifetime in seconds (1 hour). Used when session_ttl_seconds is zero.
pub const DEFAULT_SESSION_TTL: u64 = 3600;

/// Minimum TTL for replay-protection entries (7 days in ledgers at ~5 s/ledger).
pub const REPLAY_TTL: u32 = 120_960;

// ---------------------------------------------------------------------------
// Storage key helpers
// ---------------------------------------------------------------------------

fn admin_key(env: &Env) -> soroban_sdk::Vec<soroban_sdk::Symbol> {
    soroban_sdk::vec![env, symbol_short!("ADMIN")]
}

fn kyc_record_key(subject: &Address) -> (Symbol, Address) {
    (symbol_short!("KYC"), subject.clone())
}

fn compliance_check_key(subject: &Address, check_type: &String) -> (Symbol, Address, String) {
    (symbol_short!("COMP"), subject.clone(), check_type.clone())
}

fn anchor_meta_opt(env: &Env, anchor: &Address) -> Option<RoutingAnchorMeta> {
    env.storage().persistent().get(&(symbol_short!("ANCHMETA"), anchor.clone()))
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct AnchorKitContract;

#[contractimpl]
impl AnchorKitContract {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        let inst = env.storage().instance();
        if inst.has(&admin_key(&env)) {
            panic_with_error!(&env, ErrorCode::AlreadyInitialized);
        }
        inst.set(&admin_key(&env), &admin);
        inst.extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&admin_key(&env))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::NotInitialized))
    }

    // -----------------------------------------------------------------------
    // Request ID generation
    // -----------------------------------------------------------------------

    /// Generate a deterministic request ID: sha256(timestamp_u64_be || sequence_number_u32_be)[:16]
    pub fn generate_request_id(env: Env) -> RequestId {
        let ts = env.ledger().timestamp();
        let seq = env.ledger().sequence() as u32;

        // Build input: 8-byte timestamp || 4-byte sequence number (big-endian)
        let mut input = Bytes::new(&env);
        for b in ts.to_be_bytes().iter() {
            input.push_back(*b);
        }
        for b in seq.to_be_bytes().iter() {
            input.push_back(*b);
        }

        let hash = env.crypto().sha256(&input);
        let mut id = Bytes::new(&env);
        for i in 0..16u32 {
            id.push_back(hash.get(i).unwrap());
        }

        RequestId { id, created_at: ts }
    }

    // -----------------------------------------------------------------------
    // Attestor management
    // -----------------------------------------------------------------------

    /// Stores the 32-byte Ed25519 public key used to verify SEP-10 JWTs for `issuer`
    /// (the anchor identity whose signing key appears in stellar.toml / SEP-10 flow).
    pub fn set_sep10_jwt_verifying_key(env: Env, issuer: Address, public_key: Bytes) {
        Self::require_admin(&env);
        if public_key.len() != 32 {
            panic_with_error!(&env, ErrorCode::ValidationError);
        }
        let storage_key = (symbol_short!("SEP10KEY"), issuer.clone());
        env.storage().persistent().set(&storage_key, &public_key);
        env.storage()
            .persistent()
            .extend_ttl(&storage_key, PERSISTENT_TTL, PERSISTENT_TTL);
    }

    /// Configure the maximum JWT length accepted by `verify_sep10_jwt` (issue #64).
    /// Must be between 2048 and 16384. Admin-only.
    pub fn set_jwt_max_len(env: Env, max_len: u32) {
        Self::require_admin(&env);
        if max_len < sep10_jwt::MAX_JWT_LEN || max_len > 16384 {
            panic_with_error!(&env, ErrorCode::ValidationError);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("JWTMAXLEN"), &max_len);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
    }

    /// Return the currently configured JWT max length (defaults to 2048).
    pub fn get_jwt_max_len(env: Env) -> u32 {
        env.storage()
            .instance()
            .get::<_, u32>(&symbol_short!("JWTMAXLEN"))
            .unwrap_or(sep10_jwt::MAX_JWT_LEN)
    }

    /// Configure the clock skew tolerance (seconds) used by `verify_sep10_jwt`. Admin-only.
    /// Falls back to 60 s when not set. Maximum allowed value is 300 s.
    pub fn set_jwt_skew(env: Env, skew_seconds: u64) {
        Self::require_admin(&env);
        if skew_seconds > 300 {
            panic_with_error!(&env, ErrorCode::ValidationError);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("JWTSKEW"), &skew_seconds);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
    }

    /// Return the currently configured JWT clock skew tolerance in seconds (defaults to 60).
    pub fn get_jwt_skew(env: Env) -> u64 {
        env.storage()
            .instance()
            .get::<_, u64>(&symbol_short!("JWTSKEW"))
            .unwrap_or(sep10_jwt::DEFAULT_CLOCK_SKEW)
    }

    /// Verifies a SEP-10 JWT (JWS compact, EdDSA) using the stored key for `issuer`: signature, `exp`, and `sub`.
    pub fn verify_sep10_token(env: Env, token: String, issuer: Address) {
        let pk: Bytes = env
            .storage()
            .persistent()
            .get(&(symbol_short!("SEP10KEY"), issuer.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::InvalidSep10Token));
        if sep10_jwt::verify_sep10_jwt(&env, &token, &pk, None).is_err() {
            panic_with_error!(&env, ErrorCode::InvalidSep10Token);
        }
    }

    fn verify_sep10_token_matches_attestor(
        env: &Env,
        token: &String,
        issuer: &Address,
        attestor: &Address,
    ) {
        let pk: Bytes = env
            .storage()
            .persistent()
            .get(&(symbol_short!("SEP10KEY"), issuer.clone()))
            .unwrap_or_else(|| panic_with_error!(env, ErrorCode::InvalidSep10Token));
        let expected = attestor.to_string();
        if sep10_jwt::verify_sep10_jwt(env, token, &pk, Some(&expected)).is_err() {
            panic_with_error!(env, ErrorCode::InvalidSep10Token);
        }
    }

    /// Verify a SEP-10 JWT for an arbitrary subject without going through the
    /// attestor registration flow. Useful for off-chain clients that hold a
    /// stored verifying key and want to confirm token ownership.
    ///
    /// Panics with `InvalidSep10Token` if:
    /// - no verifying key is stored for `issuer`
    /// - the token signature, expiry, or `sub` claim does not match `subject`
    pub fn verify_sep10_token_for_subject(
        env: Env,
        token: String,
        issuer: Address,
        subject: Address,
    ) {
        let pk: Bytes = env
            .storage()
            .persistent()
            .get(&(symbol_short!("SEP10KEY"), issuer.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::InvalidSep10Token));
        let expected = subject.to_string();
        if sep10_jwt::verify_sep10_jwt(&env, &token, &pk, Some(&expected)).is_err() {
            panic_with_error!(&env, ErrorCode::InvalidSep10Token);
        }
    }

    pub fn register_attestor(env: Env, attestor: Address, sep10_token: String, sep10_issuer: Address, public_key: BytesN<32>) {
        Self::require_admin(&env);
        Self::verify_sep10_token_matches_attestor(&env, &sep10_token, &sep10_issuer, &attestor);
        let key = (symbol_short!("ATTESTOR"), attestor.clone());
        if env.storage().persistent().has(&key) {
            panic_with_error!(&env, ErrorCode::AttestorAlreadyRegistered);
        }
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
        let pk_key = (symbol_short!("ATPUBKEY"), attestor.clone());
        env.storage().persistent().set(&pk_key, &public_key);
        env.storage()
            .persistent()
            .extend_ttl(&pk_key, PERSISTENT_TTL, PERSISTENT_TTL);
        env.events().publish(
            (symbol_short!("attestor"), symbol_short!("added"), attestor),
            (),
        );
    }

    pub fn revoke_attestor(env: Env, attestor: Address) {
        Self::require_admin(&env);
        let key = (symbol_short!("ATTESTOR"), attestor.clone());
        if !env.storage().persistent().has(&key) {
            panic_with_error!(&env, ErrorCode::AttestorNotRegistered);
        }
        env.storage().persistent().remove(&key);
        let pk_key = (symbol_short!("ATPUBKEY"), attestor.clone());
        env.storage().persistent().remove(&pk_key);
        env.events().publish(
            (symbol_short!("attestor"), symbol_short!("removed"), attestor),
            (),
        );
    }

pub fn is_attestor(env: Env, attestor: Address) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&(symbol_short!("ATTESTOR"), attestor))
            .unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Attestor endpoint management
    // -----------------------------------------------------------------------

    /// Set the attestor&#39;s HTTPS endpoint URL (validated via validate_anchor_domain).
    /// Only the attestor themselves can update their endpoint.
    pub fn set_endpoint(env: Env, attestor: Address, endpoint: String) {
        attestor.require_auth();
        Self::check_attestor(&env, &attestor);
        crate::validate_anchor_domain(endpoint.as_str()).map_err(|_| panic_with_error!(&env, ErrorCode::InvalidEndpointFormat))?;
        let key = (symbol_short!("ENDPOINT"), attestor.clone());
        env.storage().persistent().set(&key, &endpoint);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
        env.events().publish(
            (symbol_short!("endpoint"), symbol_short!("updated")),
            EndpointUpdated {
                attestor,
                endpoint,
            },
        );
    }

    /// Retrieve the attestor&#39;s stored endpoint URL.
    pub fn get_endpoint(env: Env, attestor: Address) -> String {
        if !Self::is_attestor(env.clone(), attestor.clone()) {
            panic_with_error!(&env, ErrorCode::AttestorNotRegistered);
        }
        env.storage().persistent()
            .get::<_, String>(&(symbol_short!("ENDPOINT"), attestor))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestorNotRegistered))
    }

    // -----------------------------------------------------------------------
    // Webhook endpoint management
    // -----------------------------------------------------------------------

    /// Register a webhook URL for an attestor (attestor-auth).
    /// Validates the webhook URL via validate_anchor_domain.
    pub fn register_webhook(env: Env, attestor: Address, webhook_url: String) {
        attestor.require_auth();
        Self::check_attestor(&env, &attestor);
        crate::validate_anchor_domain(webhook_url.as_str()).map_err(|_| panic_with_error!(&env, ErrorCode::InvalidEndpointFormat))?;
        let key = (symbol_short!("WEBHOOK"), attestor.clone());
        env.storage().persistent().set(&key, &webhook_url);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
        env.events().publish(
            (symbol_short!("webhook"), symbol_short!("registered")),
            EndpointUpdated {
                attestor,
                endpoint: webhook_url,
            },
        );
    }

    /// Retrieve the webhook URL for an attestor.
    pub fn get_webhook_url(env: Env, attestor: Address) -> String {
        if !Self::is_attestor(env.clone(), attestor.clone()) {
            panic_with_error!(&env, ErrorCode::AttestorNotRegistered);
        }
        env.storage().persistent()
            .get::<_, String>(&(symbol_short!("WEBHOOK"), attestor))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestorNotRegistered))
    }

    // -----------------------------------------------------------------------
    // Service configuration
    // -----------------------------------------------------------------------

    pub fn configure_services(env: Env, anchor: Address, services: Vec<u32>) {
        anchor.require_auth();
        if !env
            .storage()
            .persistent()
            .has(&(symbol_short!("ATTESTOR"), anchor.clone()))
        {
            panic_with_error!(&env, ErrorCode::AttestorNotRegistered);
        }
        if services.is_empty() {
            panic_with_error!(&env, ErrorCode::InvalidServiceType);
        }
        let mut seen = Vec::new(&env);
        for s in services.iter() {
            if seen.contains(&s) {
                panic_with_error!(&env, ErrorCode::InvalidServiceType);
            }
            seen.push_back(s);
        }
        let record = AnchorServices {
            anchor: anchor.clone(),
            services: services.clone(),
        };
        let key = (symbol_short!("SERVICES"), anchor.clone());
        env.storage().persistent().set(&key, &record);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
        env.events()
            .publish((symbol_short!("services"), symbol_short!("config")), record);
    }

    pub fn get_supported_services(env: Env, anchor: Address) -> AnchorServices {
        env.storage()
            .persistent()
            .get::<_, AnchorServices>(&(symbol_short!("SERVICES"), anchor))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::ServicesNotConfigured))
    }

    pub fn supports_service(env: Env, anchor: Address, service: u32) -> bool {
        let record = env
            .storage()
            .persistent()
            .get::<_, AnchorServices>(&(symbol_short!("SERVICES"), anchor))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::ServicesNotConfigured));
        record.services.contains(&service)
    }

    // -----------------------------------------------------------------------
    // Attestation submission (plain)
    // -----------------------------------------------------------------------

    pub fn submit_attestation(
        env: Env,
        issuer: Address,
        subject: Address,
        timestamp: u64,
        payload_hash: Bytes,
        signature: Bytes,
    ) -> u64 {
        issuer.require_auth();
        Self::check_attestor(&env, &issuer);
        Self::verify_attestation_signature(&env, &issuer, &payload_hash, &signature);
        Self::enforce_rate_limit(&env, &issuer);
        Self::check_timestamp(&env, timestamp);

        let config = RateLimiter::get_config(&env);
        if RateLimiter::check_and_increment(&env, &issuer, &config).is_err() {
            panic_with_error!(&env, ErrorCode::RateLimitExceeded);
        }

        let used_key = (symbol_short!("USED"), issuer.clone(), payload_hash.clone());
        if env.storage().persistent().has(&used_key) {
            panic_with_error!(&env, ErrorCode::ReplayAttack);
        }

        let id = Self::next_attestation_id(&env);
        Self::store_attestation(
            &env,
            id,
            issuer.clone(),
            subject.clone(),
            timestamp,
            payload_hash.clone(),
            signature,
        );

        env.storage().persistent().set(&used_key, &timestamp);
        env.storage()
            .persistent()
            .extend_ttl(&used_key, REPLAY_TTL, REPLAY_TTL);

        env.events().publish(
            (
                symbol_short!("attest"),
                symbol_short!("recorded"),
                id,
                subject,
            ),
            AttestEvent { payload_hash, timestamp },
        );

        id
    }

    // -----------------------------------------------------------------------
    // Attestation submission with KYC enforcement
    // -----------------------------------------------------------------------

    pub fn submit_attestation_with_kyc_check(
        env: Env,
        issuer: Address,
        subject: Address,
        timestamp: u64,
        payload_hash: Bytes,
        signature: Bytes,
        require_kyc: bool,
    ) -> u64 {
        issuer.require_auth();
        Self::check_attestor(&env, &issuer);
        Self::verify_attestation_signature(&env, &issuer, &payload_hash, &signature);
        Self::check_timestamp(&env, timestamp);

        // Check KYC if required
        if require_kyc {
            let kyc_status = Self::get_kyc_status(env.clone(), subject.clone());
            
            // Only Approved status allows attestation
            if kyc_status != KycStatus::Approved {
                match kyc_status {
                    KycStatus::Pending => panic_with_error!(&env, ErrorCode::KycPending),
                    KycStatus::Rejected => panic_with_error!(&env, ErrorCode::KycRejected),
                    KycStatus::Expired => panic_with_error!(&env, ErrorCode::ComplianceNotMet),
                    KycStatus::NotSubmitted => panic_with_error!(&env, ErrorCode::KycNotFound),
                    _ => panic_with_error!(&env, ErrorCode::ComplianceNotMet),
                }
            }
        }

        let used_key = (symbol_short!("USED"), issuer.clone(), payload_hash.clone());
        if env.storage().persistent().has(&used_key) {
            panic_with_error!(&env, ErrorCode::ReplayAttack);
        }

        let id = Self::next_attestation_id(&env);
        Self::store_attestation(
            &env,
            id,
            issuer.clone(),
            subject.clone(),
            timestamp,
            payload_hash.clone(),
            signature,
        );

        env.storage().persistent().set(&used_key, &timestamp);
        env.storage()
            .persistent()
            .extend_ttl(&used_key, REPLAY_TTL, REPLAY_TTL);

        env.events().publish(
            (
                symbol_short!("attest"),
                symbol_short!("recorded"),
                id,
                subject,
            ),
            AttestEvent { payload_hash: payload_hash.clone(), timestamp },
        );

        env.events().publish(
            (symbol_short!("webhook"), symbol_short!("event")),
            WebhookEvent {
                event_type: String::from_str(&env, "attestation_submitted"),
                transaction_id: id,
                timestamp,
                payload_hash,
            },
        );

        id
    }

    // -----------------------------------------------------------------------
    // Attestation submission with request ID + tracing span
    // -----------------------------------------------------------------------

    pub fn submit_with_request_id(
        env: Env,
        request_id: RequestId,
        issuer: Address,
        subject: Address,
        timestamp: u64,
        payload_hash: Bytes,
        signature: Bytes,
    ) -> u64 {
        issuer.require_auth();
        Self::check_attestor(&env, &issuer);
        Self::verify_attestation_signature(&env, &issuer, &payload_hash, &signature);
        Self::enforce_rate_limit(&env, &issuer);
        Self::check_timestamp(&env, timestamp);

        let used_key = (symbol_short!("USED"), issuer.clone(), payload_hash.clone());
        if env.storage().persistent().has(&used_key) {
            panic_with_error!(&env, ErrorCode::ReplayAttack);
        }

        let id = Self::next_attestation_id(&env);
        Self::store_attestation(
            &env,
            id,
            issuer.clone(),
            subject.clone(),
            timestamp,
            payload_hash.clone(),
            signature,
        );

        env.storage().persistent().set(&used_key, &timestamp);
        env.storage()
            .persistent()
            .extend_ttl(&used_key, REPLAY_TTL, REPLAY_TTL);

        let now = env.ledger().timestamp();
        Self::store_span(
            &env,
            &request_id,
            String::from_str(&env, "submit_attestation"),
            issuer.clone(),
            now,
            String::from_str(&env, "success"),
        );

        env.events().publish(
            (
                symbol_short!("attest"),
                symbol_short!("recorded"),
                id,
                subject,
            ),
            AttestEvent { payload_hash: payload_hash.clone(), timestamp },
        );

        env.events().publish(
            (symbol_short!("webhook"), symbol_short!("event")),
            WebhookEvent {
                event_type: String::from_str(&env, "attestation_submitted"),
                transaction_id: id,
                timestamp,
                payload_hash,
            },
        );

        id
    }

    // -----------------------------------------------------------------------
    // Quote submission with request ID + tracing span
    // -----------------------------------------------------------------------

    #[allow(unused_variables)]
    pub fn quote_with_request_id(
        env: Env,
        request_id: RequestId,
        anchor: Address,
        from_asset: String,
        to_asset: String,
        amount: u64,
        fee_bps: u32,
        min_amount: u64,
        max_amount: u64,
        expires_at: u64,
    ) {
        anchor.require_auth();

        let services_record = env
            .storage()
            .persistent()
            .get::<_, AnchorServices>(&(symbol_short!("SERVICES"), anchor.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::ServicesNotConfigured));
        if !services_record.services.contains(&SERVICE_QUOTES) {
            panic_with_error!(&env, ErrorCode::ServicesNotConfigured);
        }

        let now = env.ledger().timestamp();
        Self::store_span(
            &env,
            &request_id,
            String::from_str(&env, "submit_quote"),
            anchor,
            now,
            String::from_str(&env, "success"),
        );
    }

    // -----------------------------------------------------------------------
    // Tracing span retrieval
    // -----------------------------------------------------------------------

    pub fn get_tracing_span(env: Env, request_id_bytes: Bytes) -> Option<TracingSpan> {
        env.storage()
            .temporary()
            .get::<_, TracingSpan>(&(symbol_short!("SPAN"), request_id_bytes))
    }

    // -----------------------------------------------------------------------
    // Attestation retrieval
    // -----------------------------------------------------------------------

    pub fn get_attestation(env: Env, id: u64) -> Attestation {
        env.storage()
            .persistent()
            .get::<_, Attestation>(&(symbol_short!("ATTEST"), id))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestationNotFound))
    }

    /// Returns the original submission timestamp for a given (issuer, payload_hash) pair,
    /// or panics with `AttestationNotFound` if no such attestation has been submitted.
    pub fn get_attestation_by_hash(env: Env, issuer: Address, payload_hash: Bytes) -> u64 {
        let used_key = (symbol_short!("USED"), issuer, payload_hash);
        env.storage()
            .persistent()
            .get::<_, u64>(&used_key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestationNotFound))
    }

    // -----------------------------------------------------------------------
    // Deterministic hash utilities (#192)
    // -----------------------------------------------------------------------

    /// Compute a canonical SHA-256 hash for an attestation payload.
    /// Field order: subject || timestamp (8-byte BE) || data.
    pub fn compute_payload_hash(
        env: Env,
        subject: Address,
        timestamp: u64,
        data: Bytes,
    ) -> BytesN<32> {
        compute_payload_hash(&env, &subject, timestamp, &data)
    }

    /// Verify that the hash stored in an attestation matches the expected hash.
    pub fn verify_payload_hash(env: Env, attestation_id: u64, expected_hash: BytesN<32>) -> bool {
        let attestation = env
            .storage()
            .persistent()
            .get::<_, Attestation>(&(symbol_short!("ATTEST"), attestation_id))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestationNotFound));

        // Convert stored Bytes payload_hash to BytesN<32>
        let stored: BytesN<32> = attestation.payload_hash.try_into().unwrap_or_else(|_| {
            panic_with_error!(&env, ErrorCode::ValidationError)
        });
        verify_payload_hash(&stored, &expected_hash)
    }

    // -----------------------------------------------------------------------
    // Session management
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // KYC data management
    // -----------------------------------------------------------------------

    /// Submit KYC data for a subject. Stores SHA-256 hash of KYC payload.
    /// Never stores raw PII, only the hash. Creates a KycRecord with Pending status.
    /// Requires attestor authorization.
    pub fn submit_kyc(
        env: Env,
        subject: Address,
        data_hash: Bytes,
        attestor: Address,
    ) {
        attestor.require_auth();
        Self::check_attestor(&env, &attestor);

        let now = env.ledger().timestamp();
        let key = kyc_record_key(&subject);

        // Check if KYC already submitted
        if env.storage().persistent().has(&key) {
            panic_with_error!(&env, ErrorCode::ComplianceNotMet);
        }

        // Create new KYC record with Pending status
        let record = KycRecord {
            subject: subject.clone(),
            status: KycStatus::Pending as u32,
            submitted_at: now,
            reviewed_at: None,
            expiry: None,
            rejection_reason_hash: None,
        };

        env.storage().persistent().set(&key, &record);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);

        // Store the data hash separately for reference
        let data_key = (symbol_short!("KYCDATA"), subject.clone());
        env.storage().persistent().set(&data_key, &data_hash);
        env.storage().persistent().extend_ttl(&data_key, PERSISTENT_TTL, PERSISTENT_TTL);

        env.events().publish(
            (symbol_short!("kyc"), symbol_short!("submitted"), subject),
            WebhookEvent {
                event_type: String::from_str(&env, "kyc_submitted"),
                transaction_id: 0,
                timestamp: now,
                payload_hash: data_hash,
            },
        );
    }

    /// Approve KYC for a subject. Requires admin authorization.
    /// Transitions status from Pending to Approved and sets reviewed_at timestamp.
    pub fn approve_kyc(env: Env, subject: Address) {
        Self::require_admin(&env);
        let now = env.ledger().timestamp();
        let key = kyc_record_key(&subject);

        let mut record: KycRecord = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::KycNotFound));

        // Only allow approval from Pending state
        if record.status != KycStatus::Pending as u32 {
            panic_with_error!(&env, ErrorCode::IllegalTransition);
        }

        record.status = KycStatus::Approved as u32;
        record.reviewed_at = Some(now);

        env.storage().persistent().set(&key, &record);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);

        env.events().publish(
            (symbol_short!("kyc"), symbol_short!("approved"), subject),
            WebhookEvent {
                event_type: String::from_str(&env, "kyc_approved"),
                transaction_id: 0,
                timestamp: now,
                payload_hash: Bytes::new(&env),
            },
        );
    }

    /// Reject KYC for a subject. Requires admin authorization.
    /// Transitions status from Pending to Rejected and stores rejection reason hash.
    pub fn reject_kyc(env: Env, subject: Address, reason_hash: Bytes) {
        Self::require_admin(&env);
        let now = env.ledger().timestamp();
        let key = kyc_record_key(&subject);

        let mut record: KycRecord = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::KycNotFound));

        // Only allow rejection from Pending state
        if record.status != KycStatus::Pending as u32 {
            panic_with_error!(&env, ErrorCode::IllegalTransition);
        }

        record.status = KycStatus::Rejected as u32;
        record.reviewed_at = Some(now);
        record.rejection_reason_hash = Some(reason_hash.clone());

        env.storage().persistent().set(&key, &record);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);

        env.events().publish(
            (symbol_short!("kyc"), symbol_short!("rejected"), subject),
            WebhookEvent {
                event_type: String::from_str(&env, "kyc_rejected"),
                transaction_id: 0,
                timestamp: now,
                payload_hash: reason_hash,
            },
        );
    }

    /// Get the KYC status for a subject.
    /// Returns KycStatus enum value. Returns NotSubmitted if no record exists.
    pub fn get_kyc_status(env: Env, subject: Address) -> KycStatus {
        let key = kyc_record_key(&subject);

        // Return NotSubmitted if no KYC record exists
        if !env.storage().persistent().has(&key) {
            return KycStatus::NotSubmitted;
        }

        let record: KycRecord = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::KycNotFound));

        // Check if KYC has expired
        if let Some(expiry) = record.expiry {
            if env.ledger().timestamp() > expiry {
                return KycStatus::Expired;
            }
        }

        match record.status {
            0 => KycStatus::NotSubmitted,
            1 => KycStatus::Pending,
            2 => KycStatus::Approved,
            3 => KycStatus::Rejected,
            4 => KycStatus::Expired,
            _ => KycStatus::NotSubmitted,
        }
    }

    // -----------------------------------------------------------------------
    // Compliance check recording (#37)
    // -----------------------------------------------------------------------

    /// Record a compliance check result for a subject (admin-only).
    /// Stores a `ComplianceCheck` record and emits a `compliance_checked` event.
    pub fn record_compliance_check(
        env: Env,
        subject: Address,
        check_type: String,
        passed: bool,
    ) {
        Self::require_admin(&env);
        let now = env.ledger().timestamp();
        let record = ComplianceCheck {
            subject: subject.clone(),
            check_type: check_type.clone(),
            result: if passed { 1u32 } else { 0u32 },
            timestamp: now,
        };
        let key = compliance_check_key(&subject, &check_type);
        env.storage().persistent().set(&key, &record);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
        env.events().publish(
            (symbol_short!("compliance"), symbol_short!("checked"), subject),
            record,
        );
    }

    pub fn create_session(env: Env, initiator: Address) -> u64 {
        initiator.require_auth();
        let inst = env.storage().instance();
        let scnt_key = soroban_sdk::vec![&env, symbol_short!("SCNT")];
        let session_id: u64 = inst.get(&scnt_key).unwrap_or(0u64);
        inst.set(&scnt_key, &(session_id + 1));
        inst.extend_ttl(INSTANCE_TTL, INSTANCE_TTL);

        let now = env.ledger().timestamp();
        let session = Session {
            session_id,
            initiator: initiator.clone(),
            created_at: now,
            nonce: 0,
            operation_count: 0,
            session_ttl_seconds: DEFAULT_SESSION_TTL,
            closed: false,
        };
        let sess_key = (symbol_short!("SESS"), session_id);
        env.storage().persistent().set(&sess_key, &session);
        env.storage().persistent().extend_ttl(&sess_key, PERSISTENT_TTL, PERSISTENT_TTL);

        let snonce_key = (symbol_short!("SNONCE"), session_id);
        env.storage().persistent().set(&snonce_key, &0u64);
        env.storage().persistent().extend_ttl(&snonce_key, PERSISTENT_TTL, PERSISTENT_TTL);

        env.events().publish(
            (symbol_short!("session"), symbol_short!("created"), session_id),
            SessionCreatedEvent { session_id, initiator, timestamp: now },
        );

        session_id
    }

    pub fn close_session(env: Env, session_id: u64, initiator: Address) {
        initiator.require_auth();
        let sess_key = (symbol_short!("SESS"), session_id);
        let mut session: Session = env
            .storage()
            .persistent()
            .get(&sess_key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestationNotFound));
        Self::validate_session(&env, &session);
        session.closed = true;
        env.storage().persistent().set(&sess_key, &session);
        let now = env.ledger().timestamp();
        env.events().publish(
            (symbol_short!("session"), symbol_short!("closed"), session_id),
            SessionClosedEvent { session_id, initiator, timestamp: now },
        );
    }

    fn require_session_open(env: &Env, session_id: u64) {
        let sess_key = (symbol_short!("SESS"), session_id);
        let session: Session = env
            .storage()
            .persistent()
            .get(&sess_key)
            .unwrap_or_else(|| panic_with_error!(env, ErrorCode::AttestationNotFound));
        Self::validate_session(env, &session);
    }

    // -----------------------------------------------------------------------
    // Quote management
    // -----------------------------------------------------------------------

    pub fn submit_quote(
        env: Env,
        anchor: Address,
        base_asset: String,
        quote_asset: String,
        rate: u64,
        fee_percentage: u32,
        minimum_amount: u64,
        maximum_amount: u64,
        valid_until: u64,
    ) -> u64 {
        anchor.require_auth();
        if fee_percentage > 10_000 {
            panic_with_error!(&env, ErrorCode::InvalidQuote);
        }
        let inst = env.storage().instance();
        let qcnt_key = soroban_sdk::vec![&env, symbol_short!("QCNT")];
        let next: u64 = inst.get(&qcnt_key).unwrap_or(0u64) + 1;
        inst.set(&qcnt_key, &next);
        inst.extend_ttl(INSTANCE_TTL, INSTANCE_TTL);

        let quote = Quote {
            quote_id: next,
            anchor: anchor.clone(),
            base_asset: base_asset.clone(),
            quote_asset: quote_asset.clone(),
            rate,
            fee_percentage,
            minimum_amount,
            maximum_amount,
            valid_until,
        };
        let q_key = (symbol_short!("QUOTE"), anchor.clone(), next);
        env.storage().persistent().set(&q_key, &quote);
        env.storage().persistent().extend_ttl(&q_key, PERSISTENT_TTL, PERSISTENT_TTL);

        let lq_key = (symbol_short!("LATESTQ"), anchor.clone());
        env.storage().persistent().set(&lq_key, &next);
        env.storage().persistent().extend_ttl(&lq_key, PERSISTENT_TTL, PERSISTENT_TTL);

        env.events().publish(
            (symbol_short!("quote"), symbol_short!("submit"), next),
            QuoteSubmitEvent {
                quote_id: next,
                anchor,
                base_asset,
                quote_asset,
                rate,
                valid_until,
            },
        );

        next
    }

    pub fn receive_quote(env: Env, receiver: Address, anchor: Address, quote_id: u64) -> Quote {
        receiver.require_auth();
        let q_key = (symbol_short!("QUOTE"), anchor.clone(), quote_id);
        let quote: Quote = env.storage().persistent().get(&q_key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestationNotFound));

        env.events().publish(
            (symbol_short!("quote"), symbol_short!("received"), quote_id),
            QuoteReceivedEvent {
                quote_id,
                receiver,
                timestamp: env.ledger().timestamp(),
            },
        );

        quote
    }

    // -----------------------------------------------------------------------
    // Session-aware attestation
    // -----------------------------------------------------------------------

    pub fn submit_attestation_with_session(
        env: Env,
        session_id: u64,
        issuer: Address,
        subject: Address,
        timestamp: u64,
        payload_hash: Bytes,
        signature: Bytes,
    ) -> u64 {
        issuer.require_auth();
        Self::require_session_open(&env, session_id);
        Self::check_attestor(&env, &issuer);
        Self::verify_attestation_signature(&env, &issuer, &payload_hash, &signature);
        Self::enforce_rate_limit(&env, &issuer);
        Self::check_timestamp(&env, timestamp);

        let used_key = (symbol_short!("USED"), issuer.clone(), payload_hash.clone());
        if env.storage().persistent().has(&used_key) {
            panic_with_error!(&env, ErrorCode::ReplayAttack);
        }

        let id = Self::next_attestation_id(&env);
        Self::store_attestation(
            &env,
            id,
            issuer.clone(),
            subject.clone(),
            timestamp,
            payload_hash.clone(),
            signature,
        );

        env.storage().persistent().set(&used_key, &timestamp);
        env.storage().persistent().extend_ttl(&used_key, REPLAY_TTL, REPLAY_TTL);

        // Increment session nonce
        let sess_key = (symbol_short!("SESS"), session_id);
        let mut session: Session = env
            .storage()
            .persistent()
            .get(&sess_key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestationNotFound));
        session.nonce += 1;
        env.storage().persistent().set(&sess_key, &session);
        env.storage().persistent().extend_ttl(&sess_key, PERSISTENT_TTL, PERSISTENT_TTL);

        // Get and increment session operation count
        let sopcnt_key = (symbol_short!("SOPCNT"), session_id);
        let op_index: u64 = env.storage().persistent().get(&sopcnt_key).unwrap_or(0u64);
        env.storage().persistent().set(&sopcnt_key, &(op_index + 1));
        env.storage().persistent().extend_ttl(&sopcnt_key, PERSISTENT_TTL, PERSISTENT_TTL);

        // Write audit log
        let inst = env.storage().instance();
        let acnt_key = soroban_sdk::vec![&env, symbol_short!("ACNT")];
        let log_id: u64 = inst.get(&acnt_key).unwrap_or(0u64);
        inst.set(&acnt_key, &(log_id + 1));
        inst.extend_ttl(INSTANCE_TTL, INSTANCE_TTL);

        let now = env.ledger().timestamp();
        let audit = AuditLog {
            log_id,
            session_id,
            actor: issuer.clone(),
            operation: OperationContext {
                session_id,
                operation_index: op_index,
                operation_type: String::from_str(&env, "attest"),
                timestamp: now,
                status: String::from_str(&env, "success"),
                result_data: id,
            },
        };
        let audit_key = (symbol_short!("AUDIT"), log_id);
        env.storage().persistent().set(&audit_key, &audit);
        env.storage().persistent().extend_ttl(&audit_key, PERSISTENT_TTL, PERSISTENT_TTL);
        let slog_key = (symbol_short!("SLOG"), session_id, op_index);
        env.storage().persistent().set(&slog_key, &log_id);
        env.storage().persistent().extend_ttl(&slog_key, PERSISTENT_TTL, PERSISTENT_TTL);

        env.events().publish(
            (
                symbol_short!("attest"),
                symbol_short!("recorded"),
                id,
                subject,
            ),
            AttestEvent { payload_hash, timestamp },
        );

        env.events().publish(
            (symbol_short!("audit"), symbol_short!("logged"), log_id),
            AuditLogEvent {
                log_id,
                session_id,
                operation_index: op_index,
                operation_type: String::from_str(&env, "attest"),
                status: String::from_str(&env, "success"),
            },
        );

        id
    }

    pub fn register_attestor_with_session(env: Env, session_id: u64, attestor: Address, public_key: BytesN<32>) {
        Self::require_admin(&env);
        Self::require_session_open(&env, session_id);
        let key = (symbol_short!("ATTESTOR"), attestor.clone());
        if env.storage().persistent().has(&key) {
            panic_with_error!(&env, ErrorCode::AttestorAlreadyRegistered);
        }
        env.storage().persistent().set(&key, &true);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
        let pk_key = (symbol_short!("ATPUBKEY"), attestor.clone());
        env.storage().persistent().set(&pk_key, &public_key);
        env.storage().persistent().extend_ttl(&pk_key, PERSISTENT_TTL, PERSISTENT_TTL);

        let sopcnt_key = (symbol_short!("SOPCNT"), session_id);
        let op_index: u64 = env.storage().persistent().get(&sopcnt_key).unwrap_or(0u64);
        env.storage().persistent().set(&sopcnt_key, &(op_index + 1));
        env.storage().persistent().extend_ttl(&sopcnt_key, PERSISTENT_TTL, PERSISTENT_TTL);

        let inst = env.storage().instance();
        let acnt_key = soroban_sdk::vec![&env, symbol_short!("ACNT")];
        let log_id: u64 = inst.get(&acnt_key).unwrap_or(0u64);
        inst.set(&acnt_key, &(log_id + 1));
        inst.extend_ttl(INSTANCE_TTL, INSTANCE_TTL);

        let admin: Address = inst
            .get::<_, Address>(&admin_key(&env))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::NotInitialized));
        let now = env.ledger().timestamp();
        let audit = AuditLog {
            log_id,
            session_id,
            actor: admin,
            operation: OperationContext {
                session_id,
                operation_index: op_index,
                operation_type: String::from_str(&env, "register"),
                timestamp: now,
                status: String::from_str(&env, "success"),
                result_data: 0,
            },
        };
        let audit_key = (symbol_short!("AUDIT"), log_id);
        env.storage().persistent().set(&audit_key, &audit);
        env.storage().persistent().extend_ttl(&audit_key, PERSISTENT_TTL, PERSISTENT_TTL);
        let slog_key = (symbol_short!("SLOG"), session_id, op_index);
        env.storage().persistent().set(&slog_key, &log_id);
        env.storage().persistent().extend_ttl(&slog_key, PERSISTENT_TTL, PERSISTENT_TTL);

        env.events().publish(
            (symbol_short!("attestor"), symbol_short!("added"), attestor),
            (),
        );
        env.events().publish(
            (symbol_short!("audit"), symbol_short!("logged"), log_id),
            AuditLogEvent {
                log_id,
                session_id,
                operation_index: op_index,
                operation_type: String::from_str(&env, "register"),
                status: String::from_str(&env, "success"),
            },
        );
    }

    pub fn revoke_attestor_with_session(env: Env, session_id: u64, attestor: Address) {
        Self::require_admin(&env);
        Self::require_session_open(&env, session_id);
        let key = (symbol_short!("ATTESTOR"), attestor.clone());
        if !env.storage().persistent().has(&key) {
            panic_with_error!(&env, ErrorCode::AttestorNotRegistered);
        }
        env.storage().persistent().remove(&key);
        let pk_key = (symbol_short!("ATPUBKEY"), attestor.clone());
        env.storage().persistent().remove(&pk_key);

        let sopcnt_key = (symbol_short!("SOPCNT"), session_id);
        let op_index: u64 = env.storage().persistent().get(&sopcnt_key).unwrap_or(0u64);
        env.storage().persistent().set(&sopcnt_key, &(op_index + 1));
        env.storage().persistent().extend_ttl(&sopcnt_key, PERSISTENT_TTL, PERSISTENT_TTL);

        let inst = env.storage().instance();
        let acnt_key = soroban_sdk::vec![&env, symbol_short!("ACNT")];
        let log_id: u64 = inst.get(&acnt_key).unwrap_or(0u64);
        inst.set(&acnt_key, &(log_id + 1));
        inst.extend_ttl(INSTANCE_TTL, INSTANCE_TTL);

        let admin: Address = inst
            .get::<_, Address>(&admin_key(&env))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::NotInitialized));
        let now = env.ledger().timestamp();
        let audit = AuditLog {
            log_id,
            session_id,
            actor: admin,
            operation: OperationContext {
                session_id,
                operation_index: op_index,
                operation_type: String::from_str(&env, "revoke"),
                timestamp: now,
                status: String::from_str(&env, "success"),
                result_data: 0,
            },
        };
        let audit_key = (symbol_short!("AUDIT"), log_id);
        env.storage().persistent().set(&audit_key, &audit);
        env.storage().persistent().extend_ttl(&audit_key, PERSISTENT_TTL, PERSISTENT_TTL);
        let slog_key = (symbol_short!("SLOG"), session_id, op_index);
        env.storage().persistent().set(&slog_key, &log_id);
        env.storage().persistent().extend_ttl(&slog_key, PERSISTENT_TTL, PERSISTENT_TTL);

        env.events().publish(
            (symbol_short!("attestor"), symbol_short!("removed"), attestor),
            (),
        );
        env.events().publish(
            (symbol_short!("audit"), symbol_short!("logged"), log_id),
            AuditLogEvent {
                log_id,
                session_id,
                operation_index: op_index,
                operation_type: String::from_str(&env, "revoke"),
                status: String::from_str(&env, "success"),
            },
        );
    }

    pub fn get_session(env: Env, session_id: u64) -> Session {
        env.storage()
            .persistent()
            .get::<_, Session>(&(symbol_short!("SESS"), session_id))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestationNotFound))
    }

    pub fn get_audit_log(env: Env, log_id: u64) -> AuditLog {
        env.storage()
            .persistent()
            .get::<_, AuditLog>(&(symbol_short!("AUDIT"), log_id))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestationNotFound))
    }

    pub fn get_session_audit_logs(env: Env, session_id: u64, limit: u64) -> Vec<AuditLog> {
        let total: u64 = env
            .storage()
            .persistent()
            .get(&(symbol_short!("SOPCNT"), session_id))
            .unwrap_or(0u64);
        let mut results = Vec::new(&env);
        let start = if total > limit { total - limit } else { 0 };
        for i in start..total {
            let slog_key = (symbol_short!("SLOG"), session_id, i);
            if let Some(log_id) = env.storage().persistent().get::<_, u64>(&slog_key) {
                let audit_key = (symbol_short!("AUDIT"), log_id);
                if let Some(entry) = env.storage().persistent().get::<_, AuditLog>(&audit_key) {
                    results.push_back(entry);
                }
            }
        }
        results
    }

    pub fn get_session_operation_count(env: Env, session_id: u64) -> u64 {
        env.storage()
            .persistent()
            .get::<_, u64>(&(symbol_short!("SOPCNT"), session_id))
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Metadata cache
    // -----------------------------------------------------------------------

    pub fn cache_metadata(env: Env, anchor: Address, metadata: AnchorMetadata, ttl_seconds: u64) {
        Self::require_admin(&env);
        let now = env.ledger().timestamp();
        let entry = MetadataCache {
            metadata,
            cached_at: now,
            ttl_seconds,
            stale_ttl_seconds: 0,
            needs_refresh: false,
        };
        let key = (symbol_short!("METACACHE"), anchor);
        let ledger_ttl = if ttl_seconds as u32 > MIN_TEMP_TTL { ttl_seconds as u32 } else { MIN_TEMP_TTL };
        env.storage().temporary().set(&key, &entry);
        env.storage().temporary().extend_ttl(&key, ledger_ttl, ledger_ttl);
    }

    pub fn get_cached_metadata(env: Env, anchor: Address) -> AnchorMetadata {
        let key = (symbol_short!("METACACHE"), anchor);
        let entry: MetadataCache = env.storage().temporary().get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::CacheNotFound));
        let now = env.ledger().timestamp();
        if entry.cached_at + entry.ttl_seconds <= now {
            panic_with_error!(&env, ErrorCode::CacheExpired);
        }
        entry.metadata
    }

    pub fn refresh_metadata_cache(env: Env, anchor: Address) {
        Self::require_admin(&env);
        let key = (symbol_short!("METACACHE"), anchor);
        env.storage().temporary().remove(&key);
    }

    /// Store a metadata entry with a stale-while-revalidate grace period.
    /// After `ttl_seconds` the entry becomes stale; after `ttl_seconds + stale_ttl_seconds`
    /// it is fully expired and `get_cached_metadata_swr` will return an error.
    pub fn cache_metadata_swr(
        env: Env,
        anchor: Address,
        metadata: AnchorMetadata,
        ttl_seconds: u64,
        stale_ttl_seconds: u64,
    ) {
        Self::require_admin(&env);
        let now = env.ledger().timestamp();
        let entry = MetadataCache {
            metadata,
            cached_at: now,
            ttl_seconds,
            stale_ttl_seconds,
            needs_refresh: false,
        };
        let key = (symbol_short!("METACACHE"), anchor);
        let total_ttl = ttl_seconds.saturating_add(stale_ttl_seconds);
        let ledger_ttl = if total_ttl as u32 > MIN_TEMP_TTL { total_ttl as u32 } else { MIN_TEMP_TTL };
        env.storage().temporary().set(&key, &entry);
        env.storage().temporary().extend_ttl(&key, ledger_ttl, ledger_ttl);
    }

    /// Retrieve a metadata entry using the stale-while-revalidate policy.
    ///
    /// Returns `(metadata, needs_refresh)`:
    /// - `needs_refresh = false` → entry is fresh (within primary TTL)
    /// - `needs_refresh = true`  → entry is stale (within grace period); caller should refresh
    ///
    /// Panics with `CacheExpired` once both TTLs have elapsed, or `CacheNotFound` if absent.
    pub fn get_cached_metadata_swr(env: Env, anchor: Address) -> (AnchorMetadata, bool) {
        let key = (symbol_short!("METACACHE"), anchor.clone());
        let mut entry: MetadataCache = env.storage().temporary().get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::CacheNotFound));
        let now = env.ledger().timestamp();
        let age = now.saturating_sub(entry.cached_at);

        if age <= entry.ttl_seconds {
            // Fresh
            (entry.metadata, false)
        } else if age <= entry.ttl_seconds.saturating_add(entry.stale_ttl_seconds) {
            // Stale — mark needs_refresh and persist the flag
            entry.needs_refresh = true;
            env.storage().temporary().set(&key, &entry);
            (entry.metadata, true)
        } else {
            panic_with_error!(&env, ErrorCode::CacheExpired);
        }
    }

    /// Unconditionally replace the cached metadata entry, resetting both TTL clocks.
    pub fn force_refresh_metadata(
        env: Env,
        anchor: Address,
        metadata: AnchorMetadata,
        ttl_seconds: u64,
        stale_ttl_seconds: u64,
    ) {
        Self::require_admin(&env);
        let now = env.ledger().timestamp();
        let entry = MetadataCache {
            metadata,
            cached_at: now,
            ttl_seconds,
            stale_ttl_seconds,
            needs_refresh: false,
        };
        let key = (symbol_short!("METACACHE"), anchor);
        let total_ttl = ttl_seconds.saturating_add(stale_ttl_seconds);
        let ledger_ttl = if total_ttl as u32 > MIN_TEMP_TTL { total_ttl as u32 } else { MIN_TEMP_TTL };
        env.storage().temporary().set(&key, &entry);
        env.storage().temporary().extend_ttl(&key, ledger_ttl, ledger_ttl);
    }

    // -----------------------------------------------------------------------
    // Capabilities cache
    // -----------------------------------------------------------------------

    pub fn cache_capabilities(env: Env, anchor: Address, toml_url: String, capabilities: String, ttl_seconds: u64) {
        Self::require_admin(&env);
        let now = env.ledger().timestamp();
        let entry = CapabilitiesCache { toml_url, capabilities, cached_at: now, ttl_seconds };
        let key = (symbol_short!("CAPCACHE"), anchor);
        let ledger_ttl = if ttl_seconds as u32 > MIN_TEMP_TTL { ttl_seconds as u32 } else { MIN_TEMP_TTL };
        env.storage().temporary().set(&key, &entry);
        env.storage().temporary().extend_ttl(&key, ledger_ttl, ledger_ttl);
    }

    pub fn get_cached_capabilities(env: Env, anchor: Address) -> CapabilitiesCache {
        let key = (symbol_short!("CAPCACHE"), anchor);
        let entry: CapabilitiesCache = env.storage().temporary().get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::CacheNotFound));
        let now = env.ledger().timestamp();
        if entry.cached_at + entry.ttl_seconds <= now {
            panic_with_error!(&env, ErrorCode::CacheExpired);
        }
        entry
    }

    pub fn refresh_capabilities_cache(env: Env, anchor: Address) {
        Self::require_admin(&env);
        let key = (symbol_short!("CAPCACHE"), anchor);
        env.storage().temporary().remove(&key);
    }

    // -----------------------------------------------------------------------
    // Routing
    // -----------------------------------------------------------------------

    pub fn get_quote(env: Env, anchor: Address, quote_id: u64) -> Quote {
        let key = (symbol_short!("QUOTE"), anchor.clone(), quote_id);
        env.storage().persistent().get::<_, Quote>(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::NoQuotesAvailable))
    }

    pub fn set_anchor_metadata(
        env: Env,
        anchor: Address,
        reputation_score: u32,
        average_settlement_time: u64,
        liquidity_score: u32,
        uptime_percentage: u32,
        total_volume: u64,
    ) {
        Self::require_admin(&env);
        let meta = RoutingAnchorMeta {
            anchor: anchor.clone(),
            reputation_score,
            average_settlement_time,
            liquidity_score,
            uptime_percentage,
            total_volume,
            is_active: true,
        };
        let meta_key = (symbol_short!("ANCHMETA"), anchor.clone());
        env.storage().persistent().set(&meta_key, &meta);
        env.storage().persistent().extend_ttl(&meta_key, PERSISTENT_TTL, PERSISTENT_TTL);

        // Maintain ANCHLIST
        let list_key = soroban_sdk::vec![&env, symbol_short!("ANCHLIST")];
        let mut list: Vec<Address> = env.storage().persistent()
            .get::<_, Vec<Address>>(&list_key)
            .unwrap_or_else(|| Vec::new(&env));
        if !list.contains(&anchor) {
            list.push_back(anchor);
            env.storage().persistent().set(&list_key, &list);
            env.storage().persistent().extend_ttl(&list_key, PERSISTENT_TTL, PERSISTENT_TTL);
        }
    }

    /// Reactivate a previously deactivated anchor (admin-only). Sets `is_active = true`.
    pub fn reactivate_anchor(env: Env, anchor: Address) {
        Self::require_admin(&env);
        let meta_key = (symbol_short!("ANCHMETA"), anchor.clone());
        let mut meta: RoutingAnchorMeta = env
            .storage()
            .persistent()
            .get(&meta_key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestorNotRegistered));
        meta.is_active = true;
        env.storage().persistent().set(&meta_key, &meta);
        env.storage()
            .persistent()
            .extend_ttl(&meta_key, PERSISTENT_TTL, PERSISTENT_TTL);
    }

    /// Return the full `RoutingAnchorMeta` for an anchor.
    pub fn get_anchor_metadata(env: Env, anchor: Address) -> RoutingAnchorMeta {
        env.storage()
            .persistent()
            .get::<_, RoutingAnchorMeta>(&(symbol_short!("ANCHMETA"), anchor))
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::AttestorNotRegistered))
    }

    /// Return all anchors in ANCHLIST where `is_active == true`.
    pub fn list_active_anchors(env: Env) -> Vec<Address> {
        let list_key = soroban_sdk::vec![&env, symbol_short!("ANCHLIST")];
        let anchors: Vec<Address> = env
            .storage()
            .persistent()
            .get::<_, Vec<Address>>(&list_key)
            .unwrap_or_else(|| Vec::new(&env));
        let mut active = Vec::new(&env);
        for anchor in anchors.iter() {
            let meta_key = (symbol_short!("ANCHMETA"), anchor.clone());
            if let Some(meta) = env
                .storage()
                .persistent()
                .get::<_, RoutingAnchorMeta>(&meta_key)
            {
                if meta.is_active {
                    active.push_back(anchor);
                }
            }
        }
        active
    }

    pub fn route_transaction(env: Env, options: RoutingOptions) -> Quote {
        let now = env.ledger().timestamp();
        let list_key = soroban_sdk::vec![&env, symbol_short!("ANCHLIST")];
        let anchors: Vec<Address> = env.storage().persistent()
            .get::<_, Vec<Address>>(&list_key)
            .unwrap_or_else(|| Vec::new(&env));

        // Collect valid quotes from active anchors
        let mut candidates: Vec<Quote> = Vec::new(&env);
        for anchor in anchors.iter() {
            // Check reputation filter
            let meta_key = (symbol_short!("ANCHMETA"), anchor.clone());
            let meta: RoutingAnchorMeta = match env.storage().persistent().get(&meta_key) {
                Some(m) => m,
                None => continue,
            };
            if !meta.is_active { continue; }
            if meta.reputation_score < options.min_reputation { continue; }

            // Get latest quote for this anchor
            let lq_key = (symbol_short!("LATESTQ"), anchor.clone());
            let quote_id: u64 = match env.storage().persistent().get(&lq_key) {
                Some(id) => id,
                None => continue,
            };
            let q_key = (symbol_short!("QUOTE"), anchor.clone(), quote_id);
            let quote: Quote = match env.storage().persistent().get(&q_key) {
                Some(q) => q,
                None => continue,
            };

            // Filter expired quotes
            if quote.valid_until <= now { continue; }

            // Filter by amount limits
            if options.request.amount < quote.minimum_amount
                || options.request.amount > quote.maximum_amount
            {
                continue;
            }

            candidates.push_back(quote);
        }

        if candidates.is_empty() {
            panic_with_error!(&env, ErrorCode::NoQuotesAvailable);
        }

        // Enforce compliance check (#38)
        if options.require_compliance {
            // Look for any passing compliance record for this subject
            // We check the generic "kyc" check_type as the standard compliance gate
            let comp_key = compliance_check_key(&options.subject, &String::from_str(&env, "kyc"));
            let passed = env.storage().persistent()
                .get::<_, ComplianceCheck>(&comp_key)
                .map(|r| r.result == 1u32)
                .unwrap_or(false);
            if !passed {
                panic_with_error!(&env, ErrorCode::ComplianceNotMet);
            }
        }

        // Apply strategy: pick best candidate
        let strategy_sym = options.strategy.get(0)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::NoQuotesAvailable));

        let lowest_fee_sym = Symbol::new(&env, "LowestFee");
        let fastest_sym = Symbol::new(&env, "FastestSettlement");
        let reputation_sym = Symbol::new(&env, "HighestReputation");

        let mut best: Quote = candidates.get(0).unwrap();

        if strategy_sym == lowest_fee_sym {
            for q in candidates.iter() {
                if q.fee_percentage < best.fee_percentage {
                    best = q;
                }
            }
        } else if strategy_sym == fastest_sym {
            // Need settlement time from metadata
            let meta_key = (symbol_short!("ANCHMETA"), best.anchor.clone());
            let mut best_time: u64 = env.storage().persistent()
                .get::<_, RoutingAnchorMeta>(&meta_key)
                .map(|m| m.average_settlement_time)
                .unwrap_or(u64::MAX);
            for q in candidates.iter() {
                let mk = (symbol_short!("ANCHMETA"), q.anchor.clone());
                let t = env.storage().persistent()
                    .get::<_, RoutingAnchorMeta>(&mk)
                    .map(|m| m.average_settlement_time)
                    .unwrap_or(u64::MAX);
                if t < best_time {
                    best_time = t;
                    best = q;
                }
            }
        } else if strategy_sym == reputation_sym {
            let meta_key = (symbol_short!("ANCHMETA"), best.anchor.clone());
            let mut best_rep: u32 = env.storage().persistent()
                .get::<_, RoutingAnchorMeta>(&meta_key)
                .map(|m| m.reputation_score)
                .unwrap_or(0);
            for q in candidates.iter() {
                let mk = (symbol_short!("ANCHMETA"), q.anchor.clone());
                let rep = env.storage().persistent()
                    .get::<_, RoutingAnchorMeta>(&mk)
                    .map(|m| m.reputation_score)
                    .unwrap_or(0);
                if rep > best_rep {
                    best_rep = rep;
                    best = q;
                }
            }
        } else if strategy_sym == Symbol::new(&env, "WeightedScore") {
            // Issue #55: weighted score = reputation(40%) + liquidity(30%) + uptime(20%) - fee(10%)
            // All component scores are u32 (0-100 range), fee_percentage is also u32.
            // Score = reputation*40 + liquidity*30 + uptime*20 + (100 - fee_pct)*10
            let weighted_score = |meta: &RoutingAnchorMeta, fee_pct: u32| -> u64 {
                let fee_factor = if fee_pct <= 100 { 100 - fee_pct } else { 0 };
                (meta.reputation_score as u64) * 40
                    + (meta.liquidity_score as u64) * 30
                    + (meta.uptime_percentage as u64) * 20
                    + (fee_factor as u64) * 10
            };
            let meta_key = (symbol_short!("ANCHMETA"), best.anchor.clone());
            let mut best_score: u64 = env.storage().persistent()
                .get::<_, RoutingAnchorMeta>(&meta_key)
                .map(|m| weighted_score(&m, best.fee_percentage))
                .unwrap_or(0);
            for q in candidates.iter() {
                let mk = (symbol_short!("ANCHMETA"), q.anchor.clone());
                let score = env.storage().persistent()
                    .get::<_, RoutingAnchorMeta>(&mk)
                    .map(|m| weighted_score(&m, q.fee_percentage))
                    .unwrap_or(0);
                if score > best_score {
                    best_score = score;
                    best = q;
                }
            }
        }

        env.events().publish(
            (symbol_short!("webhook"), symbol_short!("event")),
            WebhookEvent {
                event_type: String::from_str(&env, "transaction_routed"),
                transaction_id: best.quote_id,
                timestamp: now,
                payload_hash: Bytes::new(&env),
            },
        );

        best
    }

    /// Return up to `max_results` quotes sorted by descending weighted composite score.
    /// Weights (scaled ×1000) must sum to 1000; panics with `InvalidWeights` otherwise.
    pub fn route_anchors(
        env: Env,
        fee_weight: u32,       // scaled ×1000, e.g. 333 = 0.333
        speed_weight: u32,
        reputation_weight: u32,
        max_results: u32,
        min_reputation: u32,
    ) -> Vec<Quote> {
        let fw = fee_weight as f32 / 1000.0_f32;
        let sw = speed_weight as f32 / 1000.0_f32;
        let rw = reputation_weight as f32 / 1000.0_f32;
        let strategy = WeightedRoutingStrategy {
            fee_weight: fw,
            speed_weight: sw,
            reputation_weight: rw,
        };
        if !strategy.validate() {
            panic_with_error!(&env, ErrorCode::InvalidWeights);
        }

        let now = env.ledger().timestamp();
        let list_key = soroban_sdk::vec![&env, symbol_short!("ANCHLIST")];
        let anchors: Vec<Address> = env.storage().persistent()
            .get::<_, Vec<Address>>(&list_key)
            .unwrap_or_else(|| Vec::new(&env));

        // First pass: find max values for normalisation
        let mut max_fee: u32 = 1;
        let mut max_settlement: u64 = 1;
        let mut max_reputation: u32 = 1;

        for anchor in anchors.iter() {
            let meta: RoutingAnchorMeta = match anchor_meta_opt(&env, &anchor) {
                Some(m) if m.is_active && m.reputation_score >= min_reputation => m,
                _ => continue,
            };
            let lq_key = (symbol_short!("LATESTQ"), anchor.clone());
            let quote_id: u64 = match env.storage().persistent().get(&lq_key) {
                Some(id) => id,
                None => continue,
            };
            let q_key = (symbol_short!("QUOTE"), anchor.clone(), quote_id);
            let quote: Quote = match env.storage().persistent().get(&q_key) {
                Some(q) => q,
                None => continue,
            };
            if quote.valid_until <= now { continue; }
            if meta.average_settlement_time > max_settlement { max_settlement = meta.average_settlement_time; }
            if meta.reputation_score > max_reputation { max_reputation = meta.reputation_score; }
            if quote.fee_percentage > max_fee { max_fee = quote.fee_percentage; }
        }

        // Second pass: score into a native vec, then sort
        let mut scored: alloc::vec::Vec<(u32, Quote)> = alloc::vec::Vec::new();

        for anchor in anchors.iter() {
            let meta: RoutingAnchorMeta = match anchor_meta_opt(&env, &anchor) {
                Some(m) if m.is_active && m.reputation_score >= min_reputation => m,
                _ => continue,
            };
            let lq_key = (symbol_short!("LATESTQ"), anchor.clone());
            let quote_id: u64 = match env.storage().persistent().get(&lq_key) {
                Some(id) => id,
                None => continue,
            };
            let q_key = (symbol_short!("QUOTE"), anchor.clone(), quote_id);
            let quote: Quote = match env.storage().persistent().get(&q_key) {
                Some(q) => q,
                None => continue,
            };
            if quote.valid_until <= now { continue; }

            let score = strategy.score_anchor(
                quote.fee_percentage,
                meta.average_settlement_time,
                meta.reputation_score,
                max_fee,
                max_settlement,
                max_reputation,
            );
            scored.push(((score * 1_000_000.0_f32) as u32, quote));
        }

        // Sort descending by score
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0));

        // Return top max_results quotes as a Soroban Vec
        let limit = if max_results == 0 { 3u32 } else { max_results };
        let mut result: Vec<Quote> = Vec::new(&env);
        for (_, quote) in scored.into_iter().take(limit as usize) {
            result.push_back(quote);
        }
        result
    }


        let limit = if max_results == 0 { 3u32 } else { max_results };
        let mut result: Vec<Quote> = Vec::new(&env);
        let mut taken = 0u32;
    // -----------------------------------------------------------------------
    // Anchor Info Discovery
    // -----------------------------------------------------------------------

    pub fn fetch_anchor_info(
        env: Env,
        anchor: Address,
        toml_data: StellarToml,
        ttl_seconds: u64,
    ) {
        anchor.require_auth();
        let now = env.ledger().timestamp();
        let cached = CachedToml {
            toml: toml_data,
            cached_at: now,
            ttl_seconds,
        };
        let key = (symbol_short!("TOMLCACHE"), anchor);
        let ledger_ttl = if ttl_seconds as u32 > MIN_TEMP_TTL { ttl_seconds as u32 } else { MIN_TEMP_TTL };
        env.storage().temporary().set(&key, &cached);
        env.storage().temporary().extend_ttl(&key, ledger_ttl, ledger_ttl);
    }

    pub fn get_anchor_toml(env: Env, anchor: Address) -> StellarToml {
        let key = (symbol_short!("TOMLCACHE"), anchor);
        let cached: CachedToml = env.storage().temporary().get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ErrorCode::CacheNotFound));
        let now = env.ledger().timestamp();
        if cached.cached_at + cached.ttl_seconds <= now {
            panic_with_error!(&env, ErrorCode::CacheExpired);
        }
        cached.toml
    }

    pub fn refresh_anchor_info(env: Env, anchor: Address) {
        anchor.require_auth();
        let key = (symbol_short!("TOMLCACHE"), anchor);
        env.storage().temporary().remove(&key);
    }

    pub fn get_anchor_assets(env: Env, anchor: Address) -> Vec<String> {
        let toml = Self::get_anchor_toml(env.clone(), anchor);
        let mut assets = Vec::new(&env);
        for asset in toml.currencies.iter() {
            assets.push_back(asset.code.clone());
        }
        assets
    }

    pub fn get_anchor_asset_info(
        env: Env,
        anchor: Address,
        asset_code: String,
    ) -> AssetInfo {
        let toml = Self::get_anchor_toml(env.clone(), anchor);
        for asset in toml.currencies.iter() {
            if asset.code == asset_code {
                return asset;
            }
        }
        panic_with_error!(&env, ErrorCode::ValidationError);
    }

    pub fn get_anchor_deposit_limits(
        env: Env,
        anchor: Address,
        asset_code: String,
    ) -> (u64, u64) {
        let asset = Self::get_anchor_asset_info(env, anchor, asset_code);
        (asset.deposit_min_amount, asset.deposit_max_amount)
    }

    pub fn get_anchor_withdrawal_limits(
        env: Env,
        anchor: Address,
        asset_code: String,
    ) -> (u64, u64) {
        let asset = Self::get_anchor_asset_info(env, anchor, asset_code);
        (asset.withdrawal_min_amount, asset.withdrawal_max_amount)
    }

    pub fn get_anchor_deposit_fees(
        env: Env,
        anchor: Address,
        asset_code: String,
    ) -> (u64, u32) {
        let asset = Self::get_anchor_asset_info(env, anchor, asset_code);
        (asset.deposit_fee_fixed, asset.deposit_fee_percent)
    }

    pub fn get_anchor_withdrawal_fees(
        env: Env,
        anchor: Address,
        asset_code: String,
    ) -> (u64, u32) {
        let asset = Self::get_anchor_asset_info(env, anchor, asset_code);
        (asset.withdrawal_fee_fixed, asset.withdrawal_fee_percent)
    }

    pub fn anchor_supports_deposits(
        env: Env,
        anchor: Address,
        asset_code: String,
    ) -> bool {
        match Self::get_anchor_asset_info(env, anchor, asset_code) {
            asset => asset.deposit_enabled,
        }
    }

    pub fn anchor_supports_withdrawals(
        env: Env,
        anchor: Address,
        asset_code: String,
    ) -> bool {
        match Self::get_anchor_asset_info(env, anchor, asset_code) {
            asset => asset.withdrawal_enabled,
        }
    }

    // -----------------------------------------------------------------------
    // Transaction state
    // -----------------------------------------------------------------------

    pub fn create_transaction_record(
        env: Env,
        transaction_id: u64,
        initiator: Address,
    ) -> TransactionStateRecord {
        let now = env.ledger().timestamp();
        let mut history = soroban_sdk::Vec::new(&env);
        history.push_back((TransactionState::Pending, now));
        let record = TransactionStateRecord {
            transaction_id,
            state: TransactionState::Pending,
            initiator,
            timestamp: now,
            last_updated: now,
            error_message: None,
            state_history: history,
        };
        let key = (symbol_short!("TXSTATE"), transaction_id);
        env.storage().persistent().set(&key, &record);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
        record
    }

    // -----------------------------------------------------------------------
    // Rate limit configuration
    // -----------------------------------------------------------------------

    pub fn set_rate_limit_config(env: Env, max_submissions: u32, window_length: u32) {
        Self::require_admin(&env);
        let config = crate::rate_limiter::RateLimitConfig { max_submissions, window_length };
        RateLimiter::update_config(&env, &Self::get_admin(env.clone()), &config)
            .unwrap_or_else(|_| panic_with_error!(&env, ErrorCode::ValidationError));
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Validate that a session is neither expired nor closed.
    /// Panics with `SessionExpired` if `current_time > created_at + ttl`,
    /// or `SessionClosed` if `session.closed == true`.
    fn validate_session(env: &Env, session: &Session) {
        let ttl = if session.session_ttl_seconds == 0 {
            DEFAULT_SESSION_TTL
        } else {
            session.session_ttl_seconds
        };
        let now = env.ledger().timestamp();
        if now > session.created_at + ttl {
            panic_with_error!(env, ErrorCode::SessionExpired);
        }
        if session.closed {
            panic_with_error!(env, ErrorCode::SessionClosed);
        }
    }

    fn enforce_rate_limit(env: &Env, attestor: &Address) {
        let config = RateLimiter::get_config(env);
        if RateLimiter::check_and_increment(env, attestor, &config).is_err() {
            panic_with_error!(env, ErrorCode::RateLimitExceeded);
        }
    }

    fn require_admin(env: &Env) {
        let admin: Address = env
            .storage()
            .instance()
            .get::<_, Address>(&admin_key(env))
            .unwrap_or_else(|| panic_with_error!(env, ErrorCode::NotInitialized));
        admin.require_auth();
    }

    fn check_attestor(env: &Env, attestor: &Address) {
        if !env
            .storage()
            .persistent()
            .has(&(symbol_short!("ATTESTOR"), attestor.clone()))
        {
            panic_with_error!(env, ErrorCode::AttestorNotRegistered);
        }
    }

    fn verify_attestation_signature(env: &Env, issuer: &Address, payload_hash: &Bytes, signature: &Bytes) {
        let pk: BytesN<32> = env
            .storage()
            .persistent()
            .get(&(symbol_short!("ATPUBKEY"), issuer.clone()))
            .unwrap_or_else(|| panic_with_error!(env, ErrorCode::UnauthorizedAttestor));
        if signature.len() != 64 {
            panic_with_error!(env, ErrorCode::UnauthorizedAttestor);
        }
        if !env.crypto().ed25519_verify(&pk, payload_hash, signature) {
            panic_with_error!(env, ErrorCode::UnauthorizedAttestor);
        }
    }

    fn check_timestamp(env: &Env, timestamp: u64) {
        if timestamp == 0 {
            panic_with_error!(env, ErrorCode::InvalidTimestamp);
        }
    }

    fn next_attestation_id(env: &Env) -> u64 {
        let inst = env.storage().instance();
        let ck = soroban_sdk::vec![env, symbol_short!("COUNTER")];
        let id: u64 = inst.get(&ck).unwrap_or(0u64);
        inst.set(&ck, &(id + 1));
        inst.extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
        id
    }

    fn store_attestation(
        env: &Env,
        id: u64,
        issuer: Address,
        subject: Address,
        timestamp: u64,
        payload_hash: Bytes,
        signature: Bytes,
    ) {
        let attestation = Attestation {
            id,
            issuer,
            subject,
            timestamp,
            payload_hash,
            signature,
        };
        let key = (symbol_short!("ATTEST"), id);
        env.storage().persistent().set(&key, &attestation);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL, PERSISTENT_TTL);
    }

    fn store_span(
        env: &Env,
        request_id: &RequestId,
        operation: String,
        actor: Address,
        now: u64,
        status: String,
    ) {
        let span = TracingSpan {
            request_id: request_id.clone(),
            operation,
            actor,
            started_at: now,
            completed_at: now,
            status,
        };
        let key = (symbol_short!("SPAN"), request_id.id.clone());
        env.storage().temporary().set(&key, &span);
        env.storage()
            .temporary()
            .extend_ttl(&key, SPAN_TTL, SPAN_TTL);
    }
}
