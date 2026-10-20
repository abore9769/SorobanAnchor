/// Example demonstrating Request History Retrieval, Tracing, and Session Management
///
/// Run or validate with:
/// ```bash
/// cargo run --example request_history_example
/// cargo test --lib request_record
/// ```
///
/// This example shows how to:
/// 1. Insert, bound, and retrieve host-side request history records (`RequestRecordStore`)
/// 2. Filter retrieved history records by operation/outcome and paginate with `export_batch`
/// 3. Track on-chain operations using request IDs and sessions
/// 4. View tracing spans for operations

use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env, String, Vec};

// Import the contract types and request history store
use anchorkit::contract::{AnchorKitContract, AnchorKitContractClient};
use anchorkit::request_record::{RequestRecord, RequestRecordStore, RequestRetentionPolicy};


fn main() {
    println!("=== AnchorKit Request Tracing & Session Management Example ===\n");

    // Setup environment
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AnchorKitContract);
    let client = AnchorKitContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let attestor1 = Address::generate(&env);
    let attestor2 = Address::generate(&env);
    let subject = Address::generate(&env);

    println!("1. Initializing contract...");
    client.initialize(&admin);
    println!("   ✓ Contract initialized\n");

    println!("2. Setting up SEP-10 verification...");
    let sep10_issuer = Address::generate(&env);
    client.set_sep10_jwt_verifying_key(&sep10_issuer, &Bytes::from_slice(&env, &[0u8; 32]));
    println!("   ✓ SEP-10 verification configured\n");

    println!("3. Creating a session for grouped operations...");
    let session_id = client.create_session(&admin);
    println!("   ✓ Session created with ID: {}\n", session_id);

    println!("4. Registering attestors within session...");
    let pk1 = BytesN::from_array(&env, &[1u8; 32]);
    let pk2 = BytesN::from_array(&env, &[2u8; 32]);
    client.register_attestor_with_session(&admin, &session_id, &attestor1, &pk1);
    println!("   ✓ Attestor 1 registered");

    client.register_attestor_with_session(&admin, &session_id, &attestor2, &pk2);
    println!("   ✓ Attestor 2 registered\n");

    println!("5. Submitting attestations with request ID tracking...");
    let timestamp = env.ledger().timestamp();
    let payload_hash1 = Bytes::from_slice(&env, &[1u8; 32]);
    let payload_hash2 = Bytes::from_slice(&env, &[2u8; 32]);
    let signature = Bytes::new(&env);

    // Generate request ID for first attestation
    let request_id1 = client.generate_request_id();
    println!("   → Request ID 1 generated");

    let attestation_id1 = client.submit_with_request_id(
        &request_id1,
        &attestor1,
        &subject,
        &timestamp,
        &payload_hash1,
        &signature,
    );
    println!("   ✓ Attestation 1 submitted (ID: {})", attestation_id1);

    // Generate request ID for second attestation
    let request_id2 = client.generate_request_id();
    println!("   → Request ID 2 generated");

    let attestation_id2 = client.submit_with_request_id(
        &request_id2,
        &attestor2,
        &subject,
        &timestamp,
        &payload_hash2,
        &signature,
    );
    println!("   ✓ Attestation 2 submitted (ID: {})\n", attestation_id2);

    println!("6. Retrieving tracing spans for operations...");

    // Get tracing span for first request
    let span1 = client.get_tracing_span(&request_id1.id);
    if let Some(span) = span1 {
        println!("\n   === TRACING SPAN 1 ===");
        println!("   Operation: {:?}", span.operation);
        println!("   Started at: {}", span.started_at);
        println!("   Completed at: {}", span.completed_at);
        println!("   Status: {:?}", span.status);
        println!("   Duration: {} seconds", span.completed_at - span.started_at);
    } else {
        println!("   ⚠ No tracing span found for request 1");
    }

    // Get tracing span for second request
    let span2 = client.get_tracing_span(&request_id2.id);
    if let Some(span) = span2 {
        println!("\n   === TRACING SPAN 2 ===");
        println!("   Operation: {:?}", span.operation);
        println!("   Started at: {}", span.started_at);
        println!("   Completed at: {}", span.completed_at);
        println!("   Status: {:?}", span.status);
        println!("   Duration: {} seconds", span.completed_at - span.started_at);
    } else {
        println!("   ⚠ No tracing span found for request 2");
    }

    println!("\n7. Retrieving session information...");
    let session = client.get_session(&session_id);
    println!("\n   === SESSION DETAILS ===");
    println!("   Session ID: {}", session.session_id);
    println!("   Initiator: {:?}", session.initiator);
    println!("   Created at: {}", session.created_at);
    println!("   Nonce: {}", session.nonce);
    println!("   Operation count: {}", session.operation_count);

    // Get operation count
    let op_count = client.get_session_operation_count(&session_id);
    println!("   Total operations in session: {}", op_count);

    println!("\n8. Submitting a quote with request tracking...");

    // Configure quote service for attestor1
    let mut services = Vec::new(&env);
    services.push_back(3u32); // SERVICE_QUOTES
    client.configure_services(&attestor1, &services);

    let base_asset = String::from_str(&env, "USD");
    let quote_asset = String::from_str(&env, "USDC");
    let rate = 10000u64;
    let fee_percentage = 100u32;
    let minimum_amount = 100u64;
    let maximum_amount = 10000u64;
    let valid_until = env.ledger().timestamp() + 3600;

    // Generate request ID for quote
    let quote_request_id = client.generate_request_id();

    client.quote_with_request_id(
        &quote_request_id,
        &attestor1,
        &base_asset,
        &quote_asset,
        &rate,
        &fee_percentage,
        &minimum_amount,
        &maximum_amount,
        &valid_until,
    );
    println!("   ✓ Quote submitted with tracking");

    // Get tracing span for quote
    let quote_span = client.get_tracing_span(&quote_request_id.id);
    if let Some(span) = quote_span {
        println!("\n   === QUOTE TRACING SPAN ===");
        println!("   Operation: {:?}", span.operation);
        println!("   Status: {:?}", span.status);
        println!("   Duration: {} seconds", span.completed_at - span.started_at);
    }

    println!("\n9. Final session summary...");
    let final_session = client.get_session(&session_id);
    println!("   Total operations tracked in session: {}", final_session.operation_count);

    println!("\n10. Recording and querying bounded request history (RequestRecordStore)...");
    // Retain at most 3 records, max age 3600s, enforcing retention on every push.
    let policy = RequestRetentionPolicy::new(3, 3_600, true);
    let mut history_store = RequestRecordStore::new(policy);

    let now_secs = 1_700_000_100_u64;
    history_store.push(
        RequestRecord::new(
            1,
            1_700_000_010,
            "attest",
            "G_SYNTHETIC_ATTESTOR_1",
            "accepted",
            Some("initial attestation"),
        ),
        now_secs,
    );
    history_store.push(
        RequestRecord::new(
            2,
            1_700_000_020,
            "quote",
            "G_SYNTHETIC_ATTESTOR_1",
            "accepted",
            Some("USD/USDC quote"),
        ),
        now_secs,
    );
    history_store.push(
        RequestRecord::new(
            3,
            1_700_000_030,
            "attest",
            "G_SYNTHETIC_ATTESTOR_2",
            "rejected",
            Some("expired signature"),
        ),
        now_secs,
    );

    // Retrieve a single record by ID
    if let Some(rec) = history_store.get_by_id(1) {
        println!(
            "   ✓ Lookup by ID 1: op={}, actor={}, outcome={}",
            rec.operation, rec.actor, rec.outcome
        );
    }

    // Retrieve a bounded page (limit = 2) and filter for accepted "attest" records
    let page = history_store.export_batch(0, 2);
    println!(
        "   ✓ Paginated batch retrieved: {} records (has_more={})",
        page.records.len(),
        page.has_more
    );

    let accepted_attestations: std::vec::Vec<&RequestRecord> = page
        .records
        .iter()
        .filter(|r| r.operation == "attest" && r.outcome == "accepted")
        .collect();
    println!(
        "   ✓ Filtered accepted 'attest' records in batch: {}",
        accepted_attestations.len()
    );

    println!("\n=== Example Complete ===");
    println!("\nKey Features Demonstrated:");
    println!("✓ Request ID generation for operation tracking");
    println!("✓ Session-based grouping of related operations");
    println!("✓ Tracing span retrieval for operation monitoring");
    println!("✓ Bounded request history insertion, batch limit, and filtered retrieval");
    println!("✓ Timestamp and duration tracking");
    println!("✓ Support for multiple operation types (attestations, quotes)");
}

