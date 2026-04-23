use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "anchorkit", about = "SorobanAnchor CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deploy contract to a network (testnet/mainnet/futurenet)
    Deploy {
        #[arg(long, default_value = "testnet")]
        network: String,
        /// Source account key (secret key or identity name)
        #[arg(long, default_value = "default")]
        source: String,
    },
    /// Register an attestor
    Register {
        #[arg(long)]
        address: String,
        #[arg(long)]
        services: String,
    },
    /// Submit an attestation
    Attest {
        #[arg(long)]
        subject: String,
        #[arg(long)]
        payload_hash: String,
    },
    /// Check environment setup
    Doctor,
}

fn deploy(network: &str, source: &str) {
    let rpc_url = match network {
        "mainnet" => "https://horizon.stellar.org",
        "futurenet" => "https://rpc-futurenet.stellar.org",
        _ => "https://soroban-testnet.stellar.org",
    };
    let network_passphrase = match network {
        "mainnet" => "Public Global Stellar Network ; September 2015",
        "futurenet" => "Test SDF Future Network ; October 2022",
        _ => "Test SDF Network ; September 2015",
    };

    println!("Building WASM for {network}...");
    let build = std::process::Command::new("cargo")
        .args(["build", "--release", "--target", "wasm32-unknown-unknown",
               "--no-default-features", "--features", "wasm"])
        .status()
        .expect("failed to run cargo build");
    if !build.success() {
        eprintln!("WASM build failed");
        std::process::exit(1);
    }

    let wasm = "target/wasm32-unknown-unknown/release/anchorkit.wasm";
    println!("Deploying {wasm} to {network}...");
    let output = std::process::Command::new("stellar")
        .args([
            "contract", "deploy",
            "--wasm", wasm,
            "--source", source,
            "--rpc-url", rpc_url,
            "--network-passphrase", network_passphrase,
        ])
        .output()
        .expect("failed to run stellar contract deploy — is the Stellar CLI installed?");

    if output.status.success() {
        let contract_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        println!("Contract ID: {contract_id}");
    } else {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr).trim());
        std::process::exit(1);
    }
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Deploy { network, source } => {
            deploy(&network, &source);
        }
        Commands::Register { address, services } => {
            println!("Registering attestor {address} with services: {services}");
        }
        Commands::Attest { subject, payload_hash } => {
            println!("Attesting subject {subject} with payload hash {payload_hash}");
        }
        Commands::Doctor => {
            println!("Checking environment...");
            println!("  cargo: {}", std::process::Command::new("cargo")
                .arg("--version")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "not found".into()));
        }
    }
}
