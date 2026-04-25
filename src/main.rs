use clap::{Parser, Subcommand};
use serde::Serialize;

// ── Key resolution ────────────────────────────────────────────────────────────

/// Resolve the signing source from flags or environment.
/// Priority: --secret-key > ANCHOR_ADMIN_SECRET > --keypair-file
fn resolve_source(secret_key: Option<&str>, keypair_file: Option<&str>) -> String {
    if let Some(sk) = secret_key {
        return sk.to_string();
    }
    if let Ok(sk) = std::env::var("ANCHOR_ADMIN_SECRET") {
        if !sk.is_empty() {
            return sk;
        }
    }
    if let Some(path) = keypair_file {
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| { eprintln!("error: cannot read keypair file '{path}': {e}"); std::process::exit(1); });
        // Support JSON {"secret_key":"S..."} or plain text
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(sk) = v.get("secret_key").and_then(|s| s.as_str()) {
                return sk.to_string();
            }
        }
        return raw.trim().to_string();
    }
    eprintln!("error: signing key required — provide --secret-key, set ANCHOR_ADMIN_SECRET, or use --keypair-file");
    std::process::exit(1);
}

// ── RPC helpers ───────────────────────────────────────────────────────────────

fn rpc_url(network: &str) -> &'static str {
    match network {
        "mainnet"   => "https://horizon.stellar.org",
        "futurenet" => "https://rpc-futurenet.stellar.org",
        _           => "https://soroban-testnet.stellar.org",
    }
}

fn passphrase(network: &str) -> &'static str {
    match network {
        "mainnet"   => "Public Global Stellar Network ; September 2015",
        "futurenet" => "Test SDF Future Network ; October 2022",
        _           => "Test SDF Network ; September 2015",
    }
}

fn stellar_invoke(
    contract_id: &str,
    source: &str,
    network: &str,
    fn_args: &[&str],
) -> String {
    let output = std::process::Command::new("stellar")
        .args(["contract", "invoke",
               "--id", contract_id,
               "--source", source,
               "--rpc-url", rpc_url(network),
               "--network-passphrase", passphrase(network),
               "--"])
        .args(fn_args)
        .output()
        .expect("failed to run stellar contract invoke — is the Stellar CLI installed?");

    if output.status.success() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr).trim());
        std::process::exit(1);
    }
}

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "anchorkit", about = "SorobanAnchor CLI")]
struct Cli {
    /// Contract ID to invoke (or set ANCHOR_CONTRACT_ID)
    #[arg(long, global = true, env = "ANCHOR_CONTRACT_ID")]
    contract_id: Option<String>,

    /// Stellar network: testnet | mainnet | futurenet (or set STELLAR_NETWORK)
    #[arg(long, global = true, env = "STELLAR_NETWORK", default_value = "testnet")]
    network: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deploy contract to a network
    Deploy {
        #[arg(long, default_value = "testnet")]
        network: String,
        #[arg(long, default_value = "default")]
        source: String,
    },
    /// Register an attestor
    Register {
        #[arg(long)] address: String,
        #[arg(long, value_delimiter = ',')] services: Vec<String>,
        #[arg(long)] contract_id: String,
        #[arg(long, default_value = "testnet")] network: String,
        #[arg(long)] secret_key: Option<String>,
        #[arg(long)] keypair_file: Option<String>,
        #[arg(long)] sep10_token: String,
        #[arg(long)] sep10_issuer: String,
    },
    /// Submit an attestation
    Attest {
        #[arg(long)] subject: String,
        #[arg(long)] payload_hash: String,
        #[arg(long)] contract_id: String,
        #[arg(long, default_value = "testnet")] network: String,
        #[arg(long)] secret_key: Option<String>,
        #[arg(long)] keypair_file: Option<String>,
        #[arg(long)] issuer: String,
        #[arg(long)] session_id: Option<u64>,
    },
    /// Get the best quote for a currency pair
    Quote {
        /// Source asset code (e.g. USDC)
        #[arg(long)] from: String,
        /// Destination asset code (e.g. XLM)
        #[arg(long)] to: String,
        /// Amount in base asset units
        #[arg(long)] amount: u64,
        #[arg(long)] contract_id: String,
        #[arg(long, default_value = "testnet")] network: String,
        #[arg(long)] secret_key: Option<String>,
        #[arg(long)] keypair_file: Option<String>,
    },
    /// Fetch SEP-6 transaction status from an anchor URL
    Status {
        /// Transaction ID to look up
        #[arg(long)] tx_id: String,
        /// Anchor base URL (e.g. https://anchor.example.com)
        #[arg(long)] anchor_url: String,
    },
    /// Revoke an attestor
    Revoke {
        #[arg(long)] address: String,
        #[arg(long)] contract_id: String,
        #[arg(long, default_value = "testnet")] network: String,
        #[arg(long)] secret_key: Option<String>,
        #[arg(long)] keypair_file: Option<String>,
    },
    /// Check environment setup
    Doctor,
}

// ── Output types (JSON) ───────────────────────────────────────────────────────

#[derive(Serialize)]
struct QuoteOutput {
    quote_id: u64,
    anchor: String,
    base_asset: String,
    quote_asset: String,
    rate: u64,
    fee_percentage: u32,
    minimum_amount: u64,
    maximum_amount: u64,
    valid_until: u64,
}

#[derive(Serialize)]
struct StatusOutput {
    transaction_id: String,
    kind: String,
    status: String,
    amount_in: Option<u64>,
    amount_out: Option<u64>,
    amount_fee: Option<u64>,
    message: Option<String>,
}

// ── Command implementations ───────────────────────────────────────────────────

fn deploy(network: &str, source: &str) {
    println!("Building WASM for {network}...");
    let build = std::process::Command::new("cargo")
        .args(["build", "--release", "--target", "wasm32-unknown-unknown",
               "--no-default-features", "--features", "wasm"])
        .status()
        .expect("failed to run cargo build");
    if !build.success() { eprintln!("WASM build failed"); std::process::exit(1); }

    let wasm = "target/wasm32-unknown-unknown/release/anchorkit.wasm";
    println!("Deploying {wasm} to {network}...");
    let output = std::process::Command::new("stellar")
        .args(["contract", "deploy", "--wasm", wasm,
               "--source", source,
               "--rpc-url", rpc_url(network),
               "--network-passphrase", passphrase(network)])
        .output()
        .expect("failed to run stellar contract deploy — is the Stellar CLI installed?");

    if output.status.success() {
        println!("Contract ID: {}", String::from_utf8_lossy(&output.stdout).trim());
    } else {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr).trim());
        std::process::exit(1);
    }
}

fn parse_services(services: &[String]) -> Vec<u32> {
    services.iter().map(|s| match s.trim() {
        "deposits"    => 1,
        "withdrawals" => 2,
        "quotes"      => 3,
        "kyc"         => 4,
        other => { eprintln!("Unknown service: {other}"); std::process::exit(1); }
    }).collect()
}

fn register(
    address: &str, services: &[String], contract_id: &str,
    network: &str, source: &str, sep10_token: &str, sep10_issuer: &str,
) {
    let service_ids = parse_services(services)
        .iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");

    stellar_invoke(contract_id, source, network, &[
        "register_attestor",
        "--attestor", address,
        "--sep10_token", sep10_token,
        "--sep10_issuer", sep10_issuer,
        "--public_key", "0000000000000000000000000000000000000000000000000000000000000000",
    ]);
    stellar_invoke(contract_id, source, network, &[
        "configure_services",
        "--anchor", address,
        "--services", &service_ids,
    ]);
    println!("Attestor {address} registered and services configured.");
}

fn attest(
    subject: &str, payload_hash: &str, contract_id: &str,
    network: &str, source: &str, issuer: &str, session_id: Option<u64>,
) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs().to_string();

    let session_str;
    let result = if let Some(sid) = session_id {
        session_str = sid.to_string();
        stellar_invoke(contract_id, source, network, &[
            "submit_attestation_with_session",
            "--session_id", &session_str,
            "--issuer", issuer, "--subject", subject,
            "--timestamp", &timestamp,
            "--payload_hash", payload_hash,
            "--signature", payload_hash,
        ])
    } else {
        stellar_invoke(contract_id, source, network, &[
            "submit_attestation",
            "--issuer", issuer, "--subject", subject,
            "--timestamp", &timestamp,
            "--payload_hash", payload_hash,
            "--signature", payload_hash,
        ])
    };
    println!("Attestation ID: {result}");
}

fn quote(from: &str, to: &str, amount: u64, contract_id: &str, network: &str, source: &str) {
    let amount_str = amount.to_string();
    // route_transaction takes a RoutingOptions XDR; pass individual fields via stellar CLI args
    let raw = stellar_invoke(contract_id, source, network, &[
        "route_transaction",
        "--base_asset", from,
        "--quote_asset", to,
        "--amount", &amount_str,
        "--operation_type", "1",   // 1 = deposit
        "--strategy", "lowest_fee",
        "--min_reputation", "0",
        "--max_anchors", "10",
        "--require_kyc", "false",
    ]);

    // The stellar CLI returns XDR or JSON; parse as JSON first, fall back to raw print
    let out: QuoteOutput = serde_json::from_str(&raw).unwrap_or_else(|_| {
        // stellar CLI may return a plain contract-encoded value; surface it as-is
        eprintln!("note: could not parse quote as JSON, raw output:\n{raw}");
        std::process::exit(1);
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

fn status(tx_id: &str, anchor_url: &str) {
    let url = format!("{}/sep6/transaction?id={}", anchor_url.trim_end_matches('/'), tx_id);
    let resp = reqwest::blocking::get(&url)
        .unwrap_or_else(|e| { eprintln!("error: request failed: {e}"); std::process::exit(1); });

    if !resp.status().is_success() {
        eprintln!("error: anchor returned HTTP {}", resp.status());
        std::process::exit(1);
    }

    let body: serde_json::Value = resp.json()
        .unwrap_or_else(|e| { eprintln!("error: invalid JSON from anchor: {e}"); std::process::exit(1); });

    // SEP-6 wraps the transaction under a "transaction" key
    let tx = body.get("transaction").unwrap_or(&body);

    let kind = tx.get("kind").and_then(|v| v.as_str()).unwrap_or("deposit").to_string();
    let out = StatusOutput {
        transaction_id: tx.get("id").and_then(|v| v.as_str()).unwrap_or(tx_id).to_string(),
        kind,
        status: tx.get("status").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
        amount_in:  tx.get("amount_in").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()),
        amount_out: tx.get("amount_out").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()),
        amount_fee: tx.get("amount_fee").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()),
        message:    tx.get("message").and_then(|v| v.as_str()).map(|s| s.to_string()),
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

fn revoke(address: &str, contract_id: &str, network: &str, source: &str) {
    stellar_invoke(contract_id, source, network, &[
        "revoke_attestor",
        "--attestor", address,
    ]);
    println!("{{\"revoked\": true, \"address\": \"{address}\"}}");
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Deploy { source } => {
            deploy(&cli.network, &source);
        }
        Commands::Register { address, services, contract_id, network, secret_key, keypair_file, sep10_token, sep10_issuer } => {
            let source = resolve_source(secret_key.as_deref(), keypair_file.as_deref());
            register(&address, &services, &contract_id, &network, &source, &sep10_token, &sep10_issuer);
        }
        Commands::Attest { subject, payload_hash, contract_id, network, secret_key, keypair_file, issuer, session_id } => {
            let source = resolve_source(secret_key.as_deref(), keypair_file.as_deref());
            attest(&subject, &payload_hash, &contract_id, &network, &source, &issuer, session_id);
        }
        Commands::Quote { from, to, amount, contract_id, network, secret_key, keypair_file } => {
            let source = resolve_source(secret_key.as_deref(), keypair_file.as_deref());
            quote(&from, &to, amount, &contract_id, &network, &source);
        }
        Commands::Status { tx_id, anchor_url } => {
            status(&tx_id, &anchor_url);
        }
        Commands::Revoke { address, contract_id, network, secret_key, keypair_file } => {
            let source = resolve_source(secret_key.as_deref(), keypair_file.as_deref());
            revoke(&address, &contract_id, &network, &source);
        }
        Commands::Doctor => {
            println!("Checking environment...");
            println!("  cargo: {}", std::process::Command::new("cargo")
                .arg("--version").output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "not found".into()));
            println!("  stellar: {}", std::process::Command::new("stellar")
                .arg("--version").output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "not found".into()));
        }
    }
}
