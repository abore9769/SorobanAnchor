# Bugfix Requirements Document

## Introduction

Four related bugs in the Stellar anchor SDK affect quote submission validation, anchor routing strategy completeness, and SEP-6 transaction list normalization. Together they leave the SDK in a state where expired quotes can be submitted without error, two routing strategies are missing or incomplete, and the SEP-6 `GET /transactions` endpoint has no normalization helper.

## Bug Analysis

### Current Behavior (Defect)

1.1 WHEN `submit_quote` is called with a `valid_until` timestamp that is already in the past (i.e. `valid_until <= env.ledger().timestamp()`) THEN the system stores the quote without error and returns a quote ID.

1.2 WHEN `route_transaction` is called with strategy `"HighestReputation"` THEN the system has no implemented branch for this strategy and falls through without selecting the highest-reputation anchor.

1.3 WHEN `route_transaction` is called with strategy `"WeightedScore"` THEN the system has no branch for this strategy and falls through without computing a weighted score.

1.4 WHEN a caller needs to normalize a list of raw SEP-6 transactions from `GET /transactions` THEN the system provides no `list_transactions` function in `sep6.rs`, requiring callers to manually iterate and call `fetch_transaction_status` for each item.

### Expected Behavior (Correct)

2.1 WHEN `submit_quote` is called with `valid_until <= env.ledger().timestamp()` THEN the system SHALL panic with `ErrorCode::StaleQuote` and not store the quote.

2.2 WHEN `route_transaction` is called with strategy `"HighestReputation"` THEN the system SHALL select the candidate quote whose anchor has the highest `reputation_score` in `RoutingAnchorMeta`.

2.3 WHEN `route_transaction` is called with strategy `"WeightedScore"` THEN the system SHALL compute a weighted score for each candidate anchor combining `reputation_score`, `liquidity_score`, `uptime_percentage`, and `fee_percentage`, and SHALL select the candidate with the highest score.

2.4 WHEN `list_transactions(raw_list)` is called with a `Vec<RawTransactionResponse>` THEN the system SHALL return a `Vec<TransactionStatusResponse>` by normalizing each entry, skipping any entry whose `transaction_id` is empty.

### Unchanged Behavior (Regression Prevention)

3.1 WHEN `submit_quote` is called with `valid_until > env.ledger().timestamp()` THEN the system SHALL CONTINUE TO store the quote and return a valid quote ID.

3.2 WHEN `route_transaction` is called with strategy `"LowestFee"` THEN the system SHALL CONTINUE TO select the candidate with the lowest `fee_percentage`.

3.3 WHEN `route_transaction` is called with strategy `"FastestSettlement"` THEN the system SHALL CONTINUE TO select the candidate with the lowest `average_settlement_time`.

3.4 WHEN `fetch_transaction_status` is called with a single `RawTransactionResponse` THEN the system SHALL CONTINUE TO normalize and return a `TransactionStatusResponse` as before.
