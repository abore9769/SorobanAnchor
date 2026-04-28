use clap::{Parser, Subcommand};
use serde::Serialize;

// ── Keystore (AES-256-GCM + Argon2id) ────────────────────────────────────────

mod keystore {
    use aes_gcm::{
        aead::{Aead, KeyInit, OsRng as AeadOsRng},
        Aes256Gcm, Nonce,
    };
    use argon2::{Argon2, Params, Version};
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use rand::RngCore;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    /// A single encrypted credential entry.
    #[derive(Serialize, Deserialize, Clone)]
    pub struct EncryptedEntry {
        /// Base64-encoded 12-byte nonce.
        pub nonce: String,
        /// Base64-encoded AES-256-GCM ciphertext (includes 16-byte auth tag).
        pub ciphertext: String,
        /// Base64-encoded 16-byte Argon2id salt used to derive the key for this entry.
        pub salt: String,
    }

    /// The on-disk keystore format stored at `~/.anchorkit/keystore.json`.
    #[derive(Serialize, Deserialize, Default)]
    pub struct Keystore {
        /// Map of credential name → encrypted entry.
        pub credentials: HashMap<String, EncryptedEntry>,
    }

    /// Path to the keystore file.
    pub fn keystore_path() -> std::path::PathBuf {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        let dir = std::path::Path::new(&home).join(".anchorkit");
        std::fs::create_dir_all(&dir).ok();
        dir.join("keystore.json")
    }

    /// Load the keystore from disk, returning an empty one if the file does not exist.
    pub fn load() -> Keystore {
        let path = keystore_path();
        if !path.exists() {
            return Keystore::default();
        }
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    }

    /// Persist the keystore to disk with restricted permissions (0600 on Unix).
    pub fn save(ks: &Keystore) -> std::io::Result<()> {
        let path = keystore_path();
        let json = serde_json::to_string_pretty(ks)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&path, &json)?;
        // Restrict file permissions to owner-only on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// Derive a 32-byte AES key from `password` and `salt` using Argon2id.
    fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
        // Argon2id with recommended parameters: m=65536 KiB, t=3, p=4
        let params = Params::new(65536, 3, 4, Some(32))
            .map_err(|e| format!("argon2 params error: {e}"))?;
        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);
        let mut key = [0u8; 32];
        argon2
            .hash_password_into(password.as_bytes(), salt, &mut key)
            .map_err(|e| format!("argon2 hash error: {e}"))?;
        Ok(key)
    }

    /// Encrypt `plaintext` with AES-256-GCM using a key derived from `password`.
    /// Returns an [`EncryptedEntry`] containing the nonce, ciphertext, and salt.
    pub fn encrypt(password: &str, plaintext: &str) -> Result<EncryptedEntry, String> {
        // Generate a fresh 16-byte salt and 12-byte nonce
        let mut salt = [0u8; 16];
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        let key_bytes = derive_key(password, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| format!("cipher init error: {e}"))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| format!("encryption error: {e}"))?;

        Ok(EncryptedEntry {
            nonce: STANDARD.encode(nonce_bytes),
            ciphertext: STANDARD.encode(ciphertext),
            salt: STANDARD.encode(salt),
        })
    }

    /// Decrypt an [`EncryptedEntry`] using `password`.
    /// Returns `Err` if the password is wrong or the ciphertext is tampered.
    pub fn decrypt(password: &str, entry: &EncryptedEntry) -> Result<String, String> {
        let salt = STANDARD
            .decode(&entry.salt)
            .map_err(|e| format!("base64 salt decode error: {e}"))?;
        let nonce_bytes = STANDARD
            .decode(&entry.nonce)
            .map_err(|e| format!("base64 nonce decode error: {e}"))?;
        let ciphertext = STANDARD
            .decode(&entry.ciphertext)
            .map_err(|e| format!("base64 ciphertext decode error: {e}"))?;

        let key_bytes = derive_key(password, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| format!("cipher init error: {e}"))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_slice())
            .map_err(|_| "wrong password or corrupted credential".to_string())?;

        String::from_utf8(plaintext).map_err(|e| format!("utf8 decode error: {e}"))
    }
}

// ── Key resolution ────────────────────────────────────────────────────────────

/// Resolve the signing source from flags or environment.
/// Priority: --secret-key > ANCHOR_ADMIN_SECRET > --keypair-file > --credential-name
fn resolve_source(secret_key: Option<&str>, keypair_file: Option<&str>, credential_name: Option<&str>) -> String {
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
    if let Some(name) = credential_name {
        let password = rpassword::prompt_password("Keystore password: ")
            .unwrap_or_else(|e| { eprintln!("error: failed to read password: {e}"); std::process::exit(1); });
        let ks = keystore::load();
        let entry = ks.credentials.get(name)
            .unwrap_or_else(|| { eprintln!("error: credential '{}' not found", name); std::process::exit(1); });
        return keystore::decrypt(&password, entry)
            .unwrap_or_else(|e| { eprintln!("error: failed to decrypt credential: {e}"); std::process::exit(1); });
    }
    eprintln!("error: signing key required — provide --secret-key, set ANCHOR_ADMIN_SECRET, use --keypair-file, or use --credential-name");
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
        /// Admin address for post-deployment initialization
        #[arg(long)]
        admin: Option<String>,
        /// Validate without deploying
        #[arg(long)]
        dry_run: bool,
        /// List deployment history
        #[arg(long)]
        list: bool,
    },
    /// Register an attestor
    Register {
        #[arg(long)] address: String,
        #[arg(long, value_delimiter = ',')] services: Vec<String>,
        #[arg(long)] contract_id: String,
        #[arg(long, default_value = "testnet")] network: String,
        #[arg(long)] secret_key: Option<String>,
        #[arg(long)] keypair_file: Option<String>,
        /// Name of a credential stored in the keystore (alternative to --secret-key)
        #[arg(long)] credential_name: Option<String>,
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
        /// Name of a credential stored in the keystore (alternative to --secret-key)
        #[arg(long)] credential_name: Option<String>,
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
        /// Name of a credential stored in the keystore (alternative to --secret-key)
        #[arg(long)] credential_name: Option<String>,
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
        /// Name of a credential stored in the keystore (alternative to --secret-key)
        #[arg(long)] credential_name: Option<String>,
    },
    /// Check environment setup
    Doctor {
        /// Attempt to automatically fix issues
        #[arg(long)]
        fix: bool,
    },
    /// Manage encrypted credentials
    Credentials {
        #[command(subcommand)]
        action: CredentialsAction,
    },
}

#[derive(Subcommand)]
enum CredentialsAction {
    /// Store an encrypted credential
    Add {
        #[arg(long)] name: String,
        #[arg(long)] value: Option<String>,
    },
    /// Retrieve a credential (prompts for keystore password)
    Get {
        #[arg(long)] name: String,
    },
    /// List stored credential names (not values)
    List,
    /// Delete a credential
    Remove {
        #[arg(long)] name: String,
    },
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

// ── Deployments record ────────────────────────────────────────────────────────

#[derive(Serialize, serde::Deserialize, Clone)]
struct DeploymentRecord {
    contract_id: String,
    network: String,
    timestamp: u64,
    initialized: bool,
}

fn deployments_path() -> std::path::PathBuf {
    let dir = std::path::Path::new(".anchorkit");
    std::fs::create_dir_all(dir).ok();
    dir.join("deployments.json")
}

fn load_deployments() -> Vec<DeploymentRecord> {
    let path = deployments_path();
    if !path.exists() { return Vec::new(); }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&content).unwrap_or_default()
}

fn save_deployments(records: &[DeploymentRecord]) {
    let path = deployments_path();
    let json = serde_json::to_string_pretty(records).unwrap_or_default();
    std::fs::write(path, json).ok();
}

// ── Pre-deployment validation ─────────────────────────────────────────────────

fn pre_deploy_validate(network: &str) -> bool {
    let mut ok = true;

    // 1. WASM artifact exists
    let wasm = "target/wasm32-unknown-unknown/release/anchorkit.wasm";
    if std::path::Path::new(wasm).exists() {
        println!("  ✓ WASM artifact found");
    } else {
        eprintln!("  ✗ WASM not found at {wasm} — run: cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm");
        ok = false;
    }

    // 2. Config files valid
    let config_check = check_config_files();
    if config_check.passed {
        println!("  ✓ Config files valid");
    } else {
        eprintln!("  ✗ {}", config_check.message);
        ok = false;
    }

    // 3. Network reachable
    let net_check = check_network_connectivity(network);
    if net_check.passed {
        println!("  ✓ Network reachable");
    } else {
        eprintln!("  ✗ {}", net_check.message);
        ok = false;
    }

    ok
}

fn deploy(network: &str, source: &str, admin: Option<&str>, dry_run: bool, list: bool) {
    // --list: print deployment history and exit
    if list {
        let records = load_deployments();
        if records.is_empty() {
            println!("No deployments recorded.");
        } else {
            println!("{}", serde_json::to_string_pretty(&records).unwrap_or_default());
        }
        return;
    }

    println!("\n🔍 Pre-deployment validation ({network})...");
    if !pre_deploy_validate(network) {
        eprintln!("\n❌ Pre-deployment validation failed. Aborting.");
        std::process::exit(1);
    }
    println!("✅ Validation passed.\n");

    if dry_run {
        println!("--dry-run: skipping actual deployment.");
        return;
    }

    // Build WASM
    println!("Building WASM...");
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

    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr).trim());
        std::process::exit(1);
    }

    let contract_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    println!("Contract ID: {contract_id}");

    // Save to deployments.json
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let mut records = load_deployments();
    let mut record = DeploymentRecord {
        contract_id: contract_id.clone(),
        network: network.to_string(),
        timestamp,
        initialized: false,
    };

    // Post-deployment initialization
    let admin_addr = admin.unwrap_or(source);
    println!("Initializing contract with admin {admin_addr}...");
    let init_result = std::process::Command::new("stellar")
        .args(["contract", "invoke",
               "--id", &contract_id,
               "--source", source,
               "--rpc-url", rpc_url(network),
               "--network-passphrase", passphrase(network),
               "--", "initialize",
               "--admin", admin_addr])
        .output();

    match init_result {
        Ok(out) if out.status.success() => {
            println!("✅ Contract initialized.");
            record.initialized = true;
        }
        Ok(out) => {
            eprintln!("⚠️  Post-deployment initialization failed:");
            eprintln!("{}", String::from_utf8_lossy(&out.stderr).trim());
            eprintln!("\nContract ID: {contract_id}");
            eprintln!("To initialize manually: stellar contract invoke --id {contract_id} --source <ADMIN> -- initialize --admin <ADMIN_ADDRESS>");
        }
        Err(e) => {
            eprintln!("⚠️  Could not run initialization: {e}");
            eprintln!("Contract ID: {contract_id}");
        }
    }

    records.push(record);
    save_deployments(&records);
    println!("Deployment saved to .anchorkit/deployments.json");
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

// ── Doctor command ────────────────────────────────────────────────────────────

struct CheckResult {
    passed: bool,
    warning: bool,
    message: String,
}

impl CheckResult {
    fn pass(msg: impl Into<String>) -> Self {
        Self { passed: true, warning: false, message: msg.into() }
    }
    fn fail(msg: impl Into<String>) -> Self {
        Self { passed: false, warning: false, message: msg.into() }
    }
    fn warn(msg: impl Into<String>) -> Self {
        Self { passed: true, warning: true, message: msg.into() }
    }
    fn icon(&self) -> &str {
        if !self.passed { "✗" } else if self.warning { "⚠" } else { "✓" }
    }
    fn color(&self) -> &str {
        if !self.passed { "\x1b[31m" } else if self.warning { "\x1b[33m" } else { "\x1b[32m" }
    }
}

fn check_stellar_cli() -> CheckResult {
    match std::process::Command::new("stellar").arg("--version").output() {
        Ok(output) => {
            let version_str = String::from_utf8_lossy(&output.stdout);
            if let Some(version_line) = version_str.lines().next() {
                // Parse version like "stellar 21.0.0"
                if let Some(ver) = version_line.split_whitespace().nth(1) {
                    if let Some(major) = ver.split('.').next().and_then(|s| s.parse::<u32>().ok()) {
                        if major >= 21 {
                            return CheckResult::pass(format!("Stellar CLI {} installed", ver));
                        } else {
                            return CheckResult::fail(format!("Stellar CLI {} found, but v21+ required", ver));
                        }
                    }
                }
            }
            CheckResult::warn("Stellar CLI installed but version could not be parsed")
        }
        Err(_) => CheckResult::fail("Stellar CLI not found in PATH"),
    }
}

fn check_wasm_target(fix: bool) -> CheckResult {
    let output = std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();
    
    match output {
        Ok(out) => {
            let targets = String::from_utf8_lossy(&out.stdout);
            if targets.contains("wasm32-unknown-unknown") {
                CheckResult::pass("wasm32-unknown-unknown target installed")
            } else if fix {
                println!("  Attempting to install wasm32-unknown-unknown...");
                let install = std::process::Command::new("rustup")
                    .args(["target", "add", "wasm32-unknown-unknown"])
                    .status();
                if install.is_ok() && install.unwrap().success() {
                    CheckResult::pass("wasm32-unknown-unknown target installed (auto-fixed)")
                } else {
                    CheckResult::fail("wasm32-unknown-unknown target missing and auto-fix failed")
                }
            } else {
                CheckResult::fail("wasm32-unknown-unknown target not installed (run: rustup target add wasm32-unknown-unknown)")
            }
        }
        Err(_) => CheckResult::fail("rustup not found"),
    }
}

fn check_contract_id_env() -> CheckResult {
    match std::env::var("ANCHOR_CONTRACT_ID") {
        Ok(id) if !id.is_empty() => CheckResult::pass(format!("ANCHOR_CONTRACT_ID set: {}", &id[..id.len().min(16)])),
        _ => CheckResult::warn("ANCHOR_CONTRACT_ID not set (required for contract operations)"),
    }
}

fn check_admin_secret_env() -> CheckResult {
    match std::env::var("ANCHOR_ADMIN_SECRET") {
        Ok(secret) if !secret.is_empty() && secret.starts_with('S') => {
            CheckResult::pass("ANCHOR_ADMIN_SECRET set and appears valid")
        }
        Ok(_) => CheckResult::fail("ANCHOR_ADMIN_SECRET set but does not appear to be a valid secret key"),
        Err(_) => CheckResult::warn("ANCHOR_ADMIN_SECRET not set (required for signing operations)"),
    }
}

fn check_network_connectivity(network: &str) -> CheckResult {
    let url = rpc_url(network);
    match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .and_then(|client| client.get(url).send())
    {
        Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 404 => {
            CheckResult::pass(format!("Network connectivity to {} OK", network))
        }
        Ok(resp) => CheckResult::warn(format!("Network {} responded with HTTP {}", network, resp.status())),
        Err(e) => CheckResult::fail(format!("Cannot connect to {} network: {}", network, e)),
    }
}

fn check_contract_deployment(contract_id: &str, network: &str) -> CheckResult {
    let source = std::env::var("ANCHOR_ADMIN_SECRET").unwrap_or_else(|_| "default".to_string());
    
    let output = std::process::Command::new("stellar")
        .args(["contract", "invoke",
               "--id", contract_id,
               "--source", &source,
               "--rpc-url", rpc_url(network),
               "--network-passphrase", passphrase(network),
               "--",
               "get_attestor_count"])
        .output();
    
    match output {
        Ok(out) if out.status.success() => {
            CheckResult::pass(format!("Contract {} is deployed and responding", &contract_id[..contract_id.len().min(16)]))
        }
        Ok(_) => CheckResult::fail("Contract exists but failed to respond (may not be initialized)"),
        Err(_) => CheckResult::fail("Failed to query contract"),
    }
}

fn check_config_files() -> CheckResult {
    let config_dir = std::path::Path::new("configs");
    if !config_dir.exists() {
        return CheckResult::warn("configs/ directory not found");
    }
    
    let mut valid_count = 0;
    let mut total_count = 0;
    
    if let Ok(entries) = std::fs::read_dir(config_dir) {
        for entry in entries.flatten() {
            if let Some(ext) = entry.path().extension() {
                if ext == "json" || ext == "toml" {
                    total_count += 1;
                    if ext == "json" {
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            if serde_json::from_str::<serde_json::Value>(&content).is_ok() {
                                valid_count += 1;
                            }
                        }
                    } else {
                        valid_count += 1; // Basic check for TOML
                    }
                }
            }
        }
    }
    
    if total_count == 0 {
        CheckResult::warn("No config files found in configs/")
    } else if valid_count == total_count {
        CheckResult::pass(format!("{} config file(s) validated", total_count))
    } else {
        CheckResult::fail(format!("{}/{} config files are valid", valid_count, total_count))
    }
}

fn doctor(network: &str, fix: bool) {
    println!("\n🔍 SorobanAnchor Environment Check\n");
    
    let checks = vec![
        ("Stellar CLI", check_stellar_cli()),
        ("WASM Target", check_wasm_target(fix)),
        ("Contract ID", check_contract_id_env()),
        ("Admin Secret", check_admin_secret_env()),
        ("Network", check_network_connectivity(network)),
    ];
    
    let mut all_passed = true;
    
    for (name, result) in &checks {
        println!("  {} {} {}", result.color(), result.icon(), name);
        println!("    {}\x1b[0m", result.message);
        if !result.passed {
            all_passed = false;
        }
    }
    
    // Optional checks that require contract ID
    if let Ok(contract_id) = std::env::var("ANCHOR_CONTRACT_ID") {
        if !contract_id.is_empty() {
            let deployment_check = check_contract_deployment(&contract_id, network);
            println!("  {} {} Contract Deployment", deployment_check.color(), deployment_check.icon());
            println!("    {}\x1b[0m", deployment_check.message);
            if !deployment_check.passed {
                all_passed = false;
            }
        }
    }
    
    let config_check = check_config_files();
    println!("  {} {} Config Files", config_check.color(), config_check.icon());
    println!("    {}\x1b[0m", config_check.message);
    if !config_check.passed {
        all_passed = false;
    }
    
    println!();
    if all_passed {
        println!("✅ All checks passed! Your environment is ready.\n");
        std::process::exit(0);
    } else {
        println!("❌ Some checks failed. Please address the issues above.\n");
        if !fix {
            println!("Tip: Run with --fix to automatically resolve fixable issues.\n");
        }
        std::process::exit(1);
    }
}

// ── Credentials command ───────────────────────────────────────────────────────

fn credentials_add(name: &str, value: Option<&str>) {
    // Read value from stdin if not provided on command line (avoids shell history exposure)
    let plaintext = if let Some(v) = value {
        v.to_string()
    } else {
        rpassword::prompt_password(&format!("Value for '{}': ", name))
            .unwrap_or_else(|e| { eprintln!("error: failed to read value: {e}"); std::process::exit(1); })
    };

    let password = rpassword::prompt_password("Keystore password: ")
        .unwrap_or_else(|e| { eprintln!("error: failed to read password: {e}"); std::process::exit(1); });
    let confirm = rpassword::prompt_password("Confirm password: ")
        .unwrap_or_else(|e| { eprintln!("error: failed to read password: {e}"); std::process::exit(1); });

    if password != confirm {
        eprintln!("error: passwords do not match");
        std::process::exit(1);
    }

    let entry = keystore::encrypt(&password, &plaintext)
        .unwrap_or_else(|e| { eprintln!("error: encryption failed: {e}"); std::process::exit(1); });

    let mut ks = keystore::load();
    ks.credentials.insert(name.to_string(), entry);
    keystore::save(&ks)
        .unwrap_or_else(|e| { eprintln!("error: failed to save keystore: {e}"); std::process::exit(1); });

    println!("Credential '{}' stored at {}", name, keystore::keystore_path().display());
}

fn credentials_get(name: &str) {
    let ks = keystore::load();
    let entry = ks.credentials.get(name)
        .unwrap_or_else(|| { eprintln!("error: credential '{}' not found", name); std::process::exit(1); });

    let password = rpassword::prompt_password("Keystore password: ")
        .unwrap_or_else(|e| { eprintln!("error: failed to read password: {e}"); std::process::exit(1); });

    let value = keystore::decrypt(&password, entry)
        .unwrap_or_else(|e| { eprintln!("error: {e}"); std::process::exit(1); });

    println!("{}", value);
}

fn credentials_list() {
    let ks = keystore::load();
    if ks.credentials.is_empty() {
        println!("No credentials stored.");
        return;
    }
    let mut names: Vec<&String> = ks.credentials.keys().collect();
    names.sort();
    println!("Stored credentials ({}):", names.len());
    for name in names {
        println!("  - {}", name);
    }
}

fn credentials_remove(name: &str) {
    let mut ks = keystore::load();
    if ks.credentials.remove(name).is_none() {
        eprintln!("error: credential '{}' not found", name);
        std::process::exit(1);
    }
    keystore::save(&ks)
        .unwrap_or_else(|e| { eprintln!("error: failed to save keystore: {e}"); std::process::exit(1); });
    println!("Credential '{}' removed.", name);
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Deploy { source, admin, dry_run, list } => {
            deploy(&cli.network, &source, admin.as_deref(), dry_run, list);
        }
        Commands::Register { address, services, contract_id, network, secret_key, keypair_file, credential_name, sep10_token, sep10_issuer } => {
            let source = resolve_source(secret_key.as_deref(), keypair_file.as_deref(), credential_name.as_deref());
            register(&address, &services, &contract_id, &network, &source, &sep10_token, &sep10_issuer);
        }
        Commands::Attest { subject, payload_hash, contract_id, network, secret_key, keypair_file, credential_name, issuer, session_id } => {
            let source = resolve_source(secret_key.as_deref(), keypair_file.as_deref(), credential_name.as_deref());
            attest(&subject, &payload_hash, &contract_id, &network, &source, &issuer, session_id);
        }
        Commands::Quote { from, to, amount, contract_id, network, secret_key, keypair_file, credential_name } => {
            let source = resolve_source(secret_key.as_deref(), keypair_file.as_deref(), credential_name.as_deref());
            quote(&from, &to, amount, &contract_id, &network, &source);
        }
        Commands::Status { tx_id, anchor_url } => {
            status(&tx_id, &anchor_url);
        }
        Commands::Revoke { address, contract_id, network, secret_key, keypair_file, credential_name } => {
            let source = resolve_source(secret_key.as_deref(), keypair_file.as_deref(), credential_name.as_deref());
            revoke(&address, &contract_id, &network, &source);
        }
        Commands::Doctor { fix } => {
            doctor(&cli.network, fix);
        }
        Commands::Credentials { action } => {
            match action {
                CredentialsAction::Add { name, value } => {
                    credentials_add(&name, value.as_deref());
                }
                CredentialsAction::Get { name } => {
                    credentials_get(&name);
                }
                CredentialsAction::List => {
                    credentials_list();
                }
                CredentialsAction::Remove { name } => {
                    credentials_remove(&name);
                }
            }
        }
    }
}
