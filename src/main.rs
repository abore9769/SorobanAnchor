use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "anchorkit", about = "SorobanAnchor CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deploy contract to a network
    Deploy {
        #[arg(long, default_value = "testnet")]
        network: String,
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

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Deploy { network } => {
            println!("Deploying to {network}...");
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
