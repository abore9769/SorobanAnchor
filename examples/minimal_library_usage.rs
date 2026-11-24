//! Minimal AnchorKit library usage.
//!
//! Imports the crate and calls one pure, offline API:
//! [`anchorkit::normalize_asset_code`], which trims and uppercases a Stellar
//! asset code and rejects codes that are empty, longer than 12 characters, or
//! not ASCII-alphanumeric.
//!
//! The example uses only literal input. It makes no network calls, reads no
//! files, and does not create a Soroban `Env`.
//!
//! Run on a native target with the default `std` feature:
//!
//! ```text
//! cargo run --example minimal_library_usage
//! ```
//!
//! Expected output:
//!
//! ```text
//! " usdc " -> "USDC"
//! "BAD CODE" rejected: code=InvalidAssetCode message="Asset code is invalid" context=Some("BAD CODE")
//! ```

use anchorkit::{normalize_asset_code, AnchorKitError};

fn main() {
    // One valid code (surrounding whitespace, lowercase) and one invalid code
    // (contains a space), so both the success and error paths are shown.
    let inputs = [" usdc ", "BAD CODE"];

    for input in inputs {
        let result: Result<String, AnchorKitError> = normalize_asset_code(input);
        match result {
            Ok(code) => println!("{input:?} -> {code:?}"),
            // `AnchorKitError` exposes a stable `code`, a human-readable
            // `message`, and optional `context` (here, the rejected input).
            Err(err) => println!(
                "{input:?} rejected: code={:?} message={:?} context={:?}",
                err.code, err.message, err.context
            ),
        }
    }
}
