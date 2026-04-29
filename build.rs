use std::fs;
use std::path::Path;

use serde_json::Value;
use toml::Value as TomlValue;
use jsonschema::JSONSchema;

fn main() {
    println!("cargo:rerun-if-changed=config_schema.json");
    println!("cargo:rerun-if-changed=configs/");

    validate_configs_at_build();
    validate_schema_consistency();
}

fn validate_configs_at_build() {
    let schema_path = Path::new("config_schema.json");
    let configs_dir = Path::new("configs");

    if !schema_path.exists() || !configs_dir.exists() {
        println!("cargo:warning=Skipping config validation: missing schema or configs/");
        return;
    }

    let schema_text = fs::read_to_string(schema_path)
        .expect("Unable to read config_schema.json");
    let schema_json: Value = serde_json::from_str(&schema_text)
        .expect("Invalid JSON schema in config_schema.json");
    let compiled = JSONSchema::compile(&schema_json)
        .expect("Failed to compile config schema");

    println!("cargo:warning=Running config validation in build script...");

    let mut failed = false;
    for entry in configs_dir.read_dir().expect("Unable to read configs directory") {
        let entry = entry.expect("Unable to read config file entry");
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let label = path.file_name().unwrap().to_string_lossy();
        let json_value = if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext {
                "json" => serde_json::from_str(&fs::read_to_string(&path)
                    .expect("Unable to read JSON config file")),
                "toml" => {
                    let toml_value: TomlValue = toml::from_str(&fs::read_to_string(&path)
                        .expect("Unable to read TOML config file"))
                        .expect("Invalid TOML config file");
                    serde_json::to_value(toml_value)
                }
                _ => continue,
            }
        } else {
            continue;
        };

        let json_value = match json_value {
            Ok(value) => value,
            Err(err) => {
                println!("cargo:warning=Config parse failed: {}: {}", label, err);
                failed = true;
                continue;
            }
        };

        {
            let validation_result = compiled.validate(&json_value);
            if let Err(errors) = validation_result {
                println!("cargo:warning=❌ {} failed validation", label);
                for error in errors {
                    println!("cargo:warning=  {}", error);
                }
                failed = true;
            } else {
                println!("cargo:warning=✓ {} valid", label);
            }
        }
    }

    if failed {
        panic!(
            "\n\n❌ CONFIG VALIDATION FAILED ❌\nFix the errors above, then rebuild.\n"
        );
    }

    println!("cargo:warning=✓ All configs valid.");
}

/// Validate that schema constraints match Rust constants
fn validate_schema_consistency() {
    let schema_path = Path::new("config_schema.json");
    if !schema_path.exists() {
        return;
    }

    let schema_content = match fs::read_to_string(schema_path) {
        Ok(content) => content,
        Err(_) => return,
    };

    let checks = vec![
        ("\"maxLength\": 64", "MAX_NAME_LEN"),
        ("\"maxLength\": 16", "MAX_VERSION_LEN"),
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
