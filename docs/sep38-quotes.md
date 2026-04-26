# SEP-38 Quotes (Anchor RFQ)

[SEP-38](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0038.md)
defines a Request for Quote (RFQ) protocol that lets clients ask anchors for
firm exchange rates before committing to a transaction. AnchorKit's `sep38`
module normalises the anchor's HTTP responses.

## Overview

The SEP-38 flow is:

1. Client calls `/prices` to get indicative rates for an asset pair.
2. Client calls `/quote` to request a firm (binding) quote.
3. Anchor returns a quote with an `expires_at` timestamp.
4. Client uses the `quote_id` when initiating a SEP-6 or SEP-24 transaction.

## Fetching Indicative Prices

```rust
use anchorkit::sep38::{fetch_prices, RawPrice};

// Populate from the anchor's /prices endpoint.
let raw = RawPrice {
    buy_asset: "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into(),
    sell_asset: "iso4217:USD".into(),
    price: "1.02".into(),
};

let price = fetch_prices(raw).expect("invalid price response");
println!("1 {} = {} {}", price.sell_asset, price.price, price.buy_asset);
```

## Requesting a Firm Quote

```rust
use anchorkit::sep38::{request_firm_quote, RawFirmQuote};

let raw = RawFirmQuote {
    id: "quote-abc123".into(),
    expires_at: "1700100000".into(), // Unix timestamp
    price: "1.02".into(),
    sell_amount: "100".into(),
    buy_amount: "102".into(),
};

let quote = request_firm_quote(raw).expect("invalid quote response");
println!("Quote ID: {}", quote.id);
println!("Rate: {} (expires {})", quote.price, quote.expires_at);
```

## Checking Quote Expiry

Always check whether a quote has expired before using it:

```rust
use anchorkit::sep38::{is_quote_expired, FirmQuote};

let quote = FirmQuote {
    id: "quote-abc123".into(),
    expires_at: "1700100000".into(),
    price: "1.02".into(),
    sell_amount: "100".into(),
    buy_amount: "102".into(),
};

let now: u64 = 1_700_050_000; // current Unix timestamp

if is_quote_expired(&quote, now) {
    println!("Quote has expired, request a new one");
} else {
    println!("Quote is still valid");
}
```

## On-Chain Quote Submission

Firm quotes can be submitted to the `AnchorKitContract` for routing:

```rust,no_run
// Submit a quote with a tracing request ID.
contract.quote_with_request_id(
    env,
    request_id,
    anchor_address,
    "iso4217:USD".into(),
    "USDC:GA5Z...".into(),
    100_000_000, // amount in stroops
    50,          // fee in basis points (0.5%)
    10_000_000,  // min amount
    1_000_000_000, // max amount
    expires_at,
);
```

## Error Handling

`fetch_prices` and `request_firm_quote` currently always return `Ok(...)`.
Future versions may validate that `price` is a valid decimal string and return
`ErrorCode::InvalidQuote` on malformed data.

`is_quote_expired` returns `false` (not expired) when `expires_at` cannot be
parsed as a `u64`, to avoid false positives on ISO 8601 timestamp strings.

## On-Chain Quote Validation

The `AnchorKitContract` validates quotes on-chain:

| Error Code | Condition |
|------------|-----------|
| `ErrorCode::InvalidQuote` | Quote fields fail on-chain validation |
| `ErrorCode::StaleQuote` | Quote's `valid_until` has passed |
| `ErrorCode::NoQuotesAvailable` | No quotes found for the requested pair |
| `ErrorCode::ServicesNotConfigured` | Anchor has not configured the Quotes service |
