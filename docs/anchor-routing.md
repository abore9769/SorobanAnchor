# Anchor Routing

AnchorKit's routing system selects the best anchor for a given asset pair and
operation type based on reputation, liquidity, uptime, and settlement time.
Routing is handled entirely on-chain by `AnchorKitContract`.

## How Routing Works

1. Anchors register themselves and configure their supported services.
2. The admin (or anchors themselves) submit metadata scores.
3. When a client calls `route_transaction`, the contract evaluates all active
   anchors that support the requested service and returns the best match.

## Configuring Anchor Services

An anchor must configure which services it supports before it can be routed to:

```rust,no_run
use anchorkit::contract::{SERVICE_DEPOSITS, SERVICE_WITHDRAWALS, SERVICE_QUOTES};

// Anchor configures itself to support deposits, withdrawals, and quotes.
contract.configure_services(
    env,
    anchor_address,
    vec![SERVICE_DEPOSITS, SERVICE_WITHDRAWALS, SERVICE_QUOTES],
);
```

Available service constants:

| Constant | Value | Description |
|----------|-------|-------------|
| `SERVICE_DEPOSITS` | 1 | SEP-6 / SEP-24 deposits |
| `SERVICE_WITHDRAWALS` | 2 | SEP-6 / SEP-24 withdrawals |
| `SERVICE_QUOTES` | 3 | SEP-38 firm quotes |
| `SERVICE_KYC` | 4 | KYC verification |

## Querying Supported Services

```rust,no_run
// Check if an anchor supports a specific service.
let supports_deposits = contract.supports_service(env, anchor_address, SERVICE_DEPOSITS);

// Get all services an anchor supports.
let services = contract.get_supported_services(env, anchor_address);
```

## Submitting Anchor Metadata

Metadata scores influence routing decisions:

```rust,no_run
use anchorkit::contract::AnchorMetadata;

let metadata = AnchorMetadata {
    anchor: anchor_address,
    reputation_score: 95,          // 0–100
    liquidity_score: 80,           // 0–100
    uptime_percentage: 99,         // 0–100
    total_volume: 1_000_000_000,   // in stroops
    average_settlement_time: 3600, // seconds
    is_active: true,
};

contract.cache_metadata(env, metadata, 3600); // TTL in seconds
```

## Routing a Transaction

```rust,no_run
use anchorkit::contract::{RoutingOptions, RoutingRequest, SERVICE_DEPOSITS};

let options = RoutingOptions {
    request: RoutingRequest {
        base_asset: "iso4217:USD".into(),
        quote_asset: "USDC:GA5Z...".into(),
        amount: 100_000_000,
        operation_type: SERVICE_DEPOSITS,
    },
    strategy: vec!["reputation".into()],
    min_reputation: 70,
    max_anchors: 3,
    require_kyc: false,
    require_compliance: false,
    subject: user_address,
};

let selected_anchor = contract.route_transaction(env, options);
```

## Listing Active Anchors

```rust,no_run
let active_anchors = contract.list_active_anchors(env);
for anchor in active_anchors.iter() {
    println!("Anchor: {:?}, reputation: {}", anchor.anchor, anchor.reputation_score);
}
```

## Anchor Discovery via stellar.toml

AnchorKit can cache `stellar.toml` data on-chain for fast capability lookups:

```rust,no_run
// Fetch and cache the anchor's stellar.toml.
contract.fetch_anchor_info(env, anchor_address, toml_url);

// Retrieve cached TOML data.
let toml = contract.get_anchor_toml(env, anchor_address);

// Get supported assets.
let assets = contract.get_anchor_assets(env, anchor_address);

// Get deposit limits for a specific asset.
let limits = contract.get_anchor_deposit_limits(env, anchor_address, "USDC".into());
```

## Endpoint and Webhook Management

Each attestor can register an HTTPS endpoint and webhook URL:

```rust,no_run
// Set the attestor's service endpoint (HTTPS only, validated).
contract.set_endpoint(env, attestor_address, "https://api.myanchor.com".into());

// Retrieve the endpoint.
let endpoint = contract.get_endpoint(env, attestor_address);

// Register a webhook for event notifications.
contract.register_webhook(env, attestor_address, "https://hooks.myanchor.com/events".into());
```

## Error Codes

| Code | Condition |
|------|-----------|
| `ErrorCode::ServicesNotConfigured` | Anchor has not called `configure_services` |
| `ErrorCode::AttestorNotRegistered` | Anchor address is not a registered attestor |
| `ErrorCode::InvalidServiceType` | Service ID is not one of the known constants |
| `ErrorCode::InvalidEndpointFormat` | Endpoint URL fails HTTPS validation |
| `ErrorCode::CacheExpired` | Cached metadata TTL has elapsed |
| `ErrorCode::CacheNotFound` | No cached metadata for the anchor |
