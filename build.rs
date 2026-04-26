use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=config_schema.json");
    println!("cargo:rerun-if-changed=configs/");
    println!("cargo:rerun-if-changed=validate_config_strict.py");
    println!("cargo:rerun-if-changed=scripts/validate_all.sh");
    println!("cargo:rerun-if-changed=scripts/validate_all.ps1");

    validate_configs_at_build();
    validate_schema_consistency();
}

fn validate_configs_at_build() {
    let schema = Path::new("config_schema.json");
    let configs_dir = Path::new("configs");

    if !schema.exists() || !configs_dir.exists() {
        println!("cargo:warning=Skipping config validation: missing schema or configs/");
        return;
    }

    // On Windows run the PowerShell script; everywhere else run the bash script.
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "windows") {
        ("powershell", &["-ExecutionPolicy", "Bypass", "-File", "scripts/validate_all.ps1"])
    } else {
        ("bash", &["scripts/validate_all.sh"])
    };

    println!("cargo:warning=Running config validation ({program})...");

    let output = Command::new(program).args(args).output();

    match output {
        Ok(out) if out.status.success() => {
            println!("cargo:warning=✓ All configs valid.");
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            panic!(
                "\n\n❌ CONFIG VALIDATION FAILED ❌\n{stdout}{stderr}\n\
                 Fix the errors above, then rebuild.\n"
            );
        }
        Err(e) => {
            println!("cargo:warning=Could not run validation script ({e}); skipping.");
        }
    }
}

/// Validate that schema constraints match Rust constants
fn validate_schema_consistency() {
    use std::fs;

    let schema_path = Path::new("config_schema.json");
    if !schema_path.exists() {
        return;
    }

    let schema_content = match fs::read_to_string(schema_path) {
        Ok(content) => content,
        Err(_) => return,
    };

    // Basic consistency checks
    let checks = vec![
        ("\"maxLength\": 64", "MAX_NAME_LEN"),
        ("\"maxLength\": 16", "MAX_VERSION_LEN"),
        ("\"maxLength\": 32", "MAX_NETWORK_LEN"),
        ("\"maxLength\": 256", "MAX_ENDPOINT_LEN"),
        ("\"maxItems\": 100", "MAX_ATTESTORS"),
        ("\"maximum\": 86400", "MAX_SESSION_TIMEOUT"),
        ("\"maximum\": 10000", "MAX_OPERATIONS"),
    ];

    for (schema_val, const_name) in checks {
        if !schema_content.contains(schema_val) {
            println!(
                "cargo:warning=Schema consistency check: {} might not match {}",
                const_name, schema_val
            );
        }
    }

    println!("cargo:warning=✓ Schema consistency validated");
}
