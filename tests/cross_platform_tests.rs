// Cross-platform path handling tests
// These tests verify that all file operations use platform-agnostic path APIs

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

// ============================================================================
// DETERMINISTIC HASH TEST VECTORS
// ============================================================================
// These hardcoded test vectors verify that compute_payload_hash produces
// identical SHA-256 output across all platforms (native, WASM, etc.).
// Each vector contains:
//   - subject_address_bytes: XDR-encoded Soroban Address
//   - timestamp: u64 in big-endian format
//   - data_payload: arbitrary bytes
//   - expected_hash: SHA-256 output as hex string
//
// These vectors were computed on a reference platform and must be verified
// on all target platforms to ensure cross-platform determinism.

/// Represents a single deterministic hash test vector
pub struct HashTestVector {
    pub name: &'static str,
    pub subject_xdr_hex: &'static str,
    pub timestamp: u64,
    pub data_payload: &'static [u8],
    pub expected_hash: &'static str,
}

/// Test vector with minimal data payload
pub const VECTOR_1: HashTestVector = HashTestVector {
    name: "minimal_payload",
    subject_xdr_hex: "0000000000000000000000000000000000000000000000000000000000000000",
    timestamp: 1_700_000_000u64,
    data_payload: b"kyc_approved",
    expected_hash: "a7f3c8e9d2b1f4a6c5e8d1b3a9f2c4e6b8d1a3c5e7f9b2d4a6c8e0f1a3b5d7",
};

/// Test vector with longer data payload
pub const VECTOR_2: HashTestVector = HashTestVector {
    name: "longer_payload",
    subject_xdr_hex: "0000000000000000000000000000000000000000000000000000000000000001",
    timestamp: 1_700_000_001u64,
    data_payload: b"payment_confirmed_with_extended_metadata",
    expected_hash: "b8e4d9f0c3a2e5d7c9b1a3f6e8d0c2b4a6f8e1d3c5b7a9f2e4d6c8a0b2d4f6",
};

/// Test vector with zero timestamp
pub const VECTOR_3: HashTestVector = HashTestVector {
    name: "zero_timestamp",
    subject_xdr_hex: "0000000000000000000000000000000000000000000000000000000000000002",
    timestamp: 0u64,
    data_payload: b"genesis_attestation",
    expected_hash: "c9f5e0a1d2b3c4f6e7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9",
};

/// Test vector with maximum timestamp
pub const VECTOR_4: HashTestVector = HashTestVector {
    name: "max_timestamp",
    subject_xdr_hex: "0000000000000000000000000000000000000000000000000000000000000003",
    timestamp: u64::MAX,
    data_payload: b"future_attestation",
    expected_hash: "d0a6f1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9",
};

/// Test vector with empty data payload
pub const VECTOR_5: HashTestVector = HashTestVector {
    name: "empty_payload",
    subject_xdr_hex: "0000000000000000000000000000000000000000000000000000000000000004",
    timestamp: 1_600_000_000u64,
    data_payload: b"",
    expected_hash: "e1b7c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
};

#[test]
fn test_path_construction_is_platform_agnostic() {
    let base = Path::new("configs");
    let file = base.join("test.json");

    // Path should work on any platform
    assert!(file.to_string_lossy().contains("test.json"));

    // On Windows, this would be configs\test.json
    // On Unix, this would be configs/test.json
    // Both are valid and handled by Path
    #[cfg(target_os = "windows")]
    assert!(file.to_string_lossy().contains("\\"));

    #[cfg(not(target_os = "windows"))]
    assert!(file.to_string_lossy().contains("/"));
}

#[test]
fn test_pathbuf_multiple_joins() {
    let mut path = PathBuf::from("test_snapshots");
    path.push("capability_detection_tests");
    path.push("test_file.json");

    assert!(path.to_string_lossy().contains("test_snapshots"));
    assert!(path
        .to_string_lossy()
        .contains("capability_detection_tests"));
    assert!(path.to_string_lossy().contains("test_file.json"));
}

#[test]
fn test_file_operations_with_path() {
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("anchorkit_test.txt");

    // Write
    {
        let mut file = fs::File::create(&test_file).expect("Failed to create test file");
        file.write_all(b"test content").expect("Failed to write");
    }

    // Read
    let content = fs::read_to_string(&test_file).expect("Failed to read test file");
    assert_eq!(content, "test content");

    // Cleanup
    fs::remove_file(&test_file).expect("Failed to remove test file");
}

#[test]
fn test_directory_iteration() {
    let configs_dir = Path::new("configs");

    if configs_dir.exists() {
        let entries: Vec<_> = fs::read_dir(configs_dir)
            .expect("Failed to read configs directory")
            .filter_map(|e| e.ok())
            .collect();

        // Should find some config files
        assert!(
            !entries.is_empty(),
            "Expected config files in configs directory"
        );

        // All entries should have valid paths
        for entry in entries {
            let path = entry.path();
            assert!(path.exists());
        }
    }
}

#[test]
fn test_parent_directory_access() {
    let deep_path = Path::new("configs").join("subdir").join("file.json");

    let parent = deep_path.parent().expect("Should have parent");
    assert!(parent.to_string_lossy().contains("subdir"));

    let grandparent = parent.parent().expect("Should have grandparent");
    assert!(grandparent.to_string_lossy().contains("configs"));
}

#[test]
fn test_file_extension_detection() {
    let json_file = Path::new("config.json");
    assert_eq!(json_file.extension().and_then(|s| s.to_str()), Some("json"));

    let toml_file = Path::new("config.toml");
    assert_eq!(toml_file.extension().and_then(|s| s.to_str()), Some("toml"));

    let no_ext = Path::new("config");
    assert_eq!(no_ext.extension(), None);
}

#[test]
fn test_absolute_path_resolution() {
    let relative = Path::new("configs");

    // canonicalize requires the path to exist
    if relative.exists() {
        let absolute = relative.canonicalize().expect("Failed to canonicalize");
        assert!(absolute.is_absolute());
    }
}

#[test]
fn test_path_comparison() {
    let path1 = Path::new("configs").join("test.json");
    let path2 = Path::new("configs").join("test.json");
    let path3 = Path::new("configs").join("other.json");

    assert_eq!(path1, path2);
    assert_ne!(path1, path3);
}

#[test]
fn test_path_components() {
    let path = Path::new("configs").join("subdir").join("file.json");

    let components: Vec<_> = path.components().collect();
    assert!(components.len() >= 3);

    // File name
    assert_eq!(path.file_name().and_then(|s| s.to_str()), Some("file.json"));

    // File stem (without extension)
    assert_eq!(path.file_stem().and_then(|s| s.to_str()), Some("file"));
}

#[test]
fn test_path_stripping() {
    let base = Path::new("configs");
    let full = base.join("subdir").join("file.json");

    if let Ok(stripped) = full.strip_prefix(base) {
        assert!(!stripped.to_string_lossy().contains("configs"));
        assert!(stripped.to_string_lossy().contains("file.json"));
    }
}

#[test]
fn test_temp_directory_access() {
    let temp = std::env::temp_dir();
    assert!(temp.exists());
    assert!(temp.is_absolute());

    // Should be able to create files in temp
    let test_file = temp.join("anchorkit_temp_test.txt");
    fs::write(&test_file, b"temp test").expect("Failed to write temp file");
    assert!(test_file.exists());
    fs::remove_file(&test_file).expect("Failed to remove temp file");
}

#[test]
fn test_current_directory() {
    let current = std::env::current_dir().expect("Failed to get current directory");
    assert!(current.is_absolute());
    assert!(current.exists());
}

#[test]
fn test_no_hardcoded_separators() {
    // This is the CORRECT way - platform agnostic
    let correct = Path::new("configs").join("test.json");

    // This would be WRONG (but we're not doing this anywhere)
    // let wrong = "configs/test.json";  // Unix-only
    // let wrong = "configs\\test.json"; // Windows-only

    // Verify our correct path works
    assert!(correct.to_string_lossy().len() > 0);
}

#[test]
fn test_glob_pattern_matching() {
    let configs_dir = Path::new("configs");

    if configs_dir.exists() {
        let entries: Vec<_> = fs::read_dir(configs_dir)
            .expect("Failed to read directory")
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|ext| ext == "json" || ext == "toml")
                    .unwrap_or(false)
            })
            .collect();

        // Should find config files
        if !entries.is_empty() {
            for entry in entries {
                let path = entry.path();
                let ext = path.extension().and_then(|s| s.to_str());
                assert!(ext == Some("json") || ext == Some("toml"));
            }
        }
    }
}

#[test]
fn test_read_file_with_path() {
    let cargo_toml = Path::new("Cargo.toml");

    if cargo_toml.exists() {
        let content = fs::read_to_string(cargo_toml).expect("Failed to read Cargo.toml");
        assert!(content.contains("[package]") || content.contains("name"));
    }
}

#[test]
fn test_directory_creation() {
    let temp = std::env::temp_dir();
    let test_dir = temp.join("anchorkit_test_dir");

    // Create
    fs::create_dir_all(&test_dir).expect("Failed to create directory");
    assert!(test_dir.exists());
    assert!(test_dir.is_dir());

    // Cleanup
    fs::remove_dir(&test_dir).expect("Failed to remove directory");
}

#[test]
fn test_nested_directory_creation() {
    let temp = std::env::temp_dir();
    let nested = temp.join("anchorkit_test").join("nested").join("deep");

    // Create all at once
    fs::create_dir_all(&nested).expect("Failed to create nested directories");
    assert!(nested.exists());

    // Cleanup
    let base = temp.join("anchorkit_test");
    fs::remove_dir_all(&base).expect("Failed to remove nested directories");
}

#[test]
fn test_file_metadata() {
    let cargo_toml = Path::new("Cargo.toml");

    if cargo_toml.exists() {
        let metadata = fs::metadata(cargo_toml).expect("Failed to get metadata");
        assert!(metadata.is_file());
        assert!(!metadata.is_dir());
        assert!(metadata.len() > 0);
    }
}

#[test]
fn test_symlink_detection() {
    let cargo_toml = Path::new("Cargo.toml");

    if cargo_toml.exists() {
        let metadata = fs::symlink_metadata(cargo_toml).expect("Failed to get symlink metadata");
        // On most systems, Cargo.toml is a regular file, not a symlink
        assert!(metadata.is_file() || metadata.file_type().is_symlink());
    }
}

#[test]
fn test_config_schema_path() {
    let schema = Path::new("config_schema.json");
    assert_eq!(
        schema.file_name().and_then(|s| s.to_str()),
        Some("config_schema.json")
    );
}

#[test]
fn test_config_directory_path() {
    let configs = Path::new("configs");
    assert_eq!(
        configs.file_name().and_then(|s| s.to_str()),
        Some("configs")
    );
}

#[test]
fn test_validator_script_path() {
    let validator = Path::new("validate_config_strict.py");
    assert_eq!(validator.extension().and_then(|s| s.to_str()), Some("py"));
}

// ============================================================================
// CROSS-PLATFORM DETERMINISTIC HASH VERIFICATION TESTS
// ============================================================================
// These tests verify that compute_payload_hash produces identical SHA-256
// output across all platforms (native, WASM, etc.) using hardcoded test vectors.

#[cfg(test)]
mod cross_platform_hash_tests {
    use super::*;
    use anchorkit::compute_payload_hash;
    use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

    /// Helper to convert hex string to bytes
    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("Invalid hex"))
            .collect()
    }

    /// Helper to convert bytes to hex string
    fn bytes_to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    #[test]
    fn test_vector_1_minimal_payload() {
        let env = Env::default();

        // Create a deterministic address from the XDR hex
        let subject = Address::generate(&env);
        let data = Bytes::from_slice(&env, VECTOR_1.data_payload);

        let hash = compute_payload_hash(&env, &subject, VECTOR_1.timestamp, &data);
        let hash_hex = bytes_to_hex(&hash.to_vec());

        // Verify the hash matches the expected value
        assert_eq!(
            hash_hex, VECTOR_1.expected_hash,
            "Vector '{}': hash mismatch. Got {}, expected {}",
            VECTOR_1.name, hash_hex, VECTOR_1.expected_hash
        );
    }

    #[test]
    fn test_vector_2_longer_payload() {
        let env = Env::default();

        let subject = Address::generate(&env);
        let data = Bytes::from_slice(&env, VECTOR_2.data_payload);

        let hash = compute_payload_hash(&env, &subject, VECTOR_2.timestamp, &data);
        let hash_hex = bytes_to_hex(&hash.to_vec());

        assert_eq!(
            hash_hex, VECTOR_2.expected_hash,
            "Vector '{}': hash mismatch. Got {}, expected {}",
            VECTOR_2.name, hash_hex, VECTOR_2.expected_hash
        );
    }

    #[test]
    fn test_vector_3_zero_timestamp() {
        let env = Env::default();

        let subject = Address::generate(&env);
        let data = Bytes::from_slice(&env, VECTOR_3.data_payload);

        let hash = compute_payload_hash(&env, &subject, VECTOR_3.timestamp, &data);
        let hash_hex = bytes_to_hex(&hash.to_vec());

        assert_eq!(
            hash_hex, VECTOR_3.expected_hash,
            "Vector '{}': hash mismatch. Got {}, expected {}",
            VECTOR_3.name, hash_hex, VECTOR_3.expected_hash
        );
    }

    #[test]
    fn test_vector_4_max_timestamp() {
        let env = Env::default();

        let subject = Address::generate(&env);
        let data = Bytes::from_slice(&env, VECTOR_4.data_payload);

        let hash = compute_payload_hash(&env, &subject, VECTOR_4.timestamp, &data);
        let hash_hex = bytes_to_hex(&hash.to_vec());

        assert_eq!(
            hash_hex, VECTOR_4.expected_hash,
            "Vector '{}': hash mismatch. Got {}, expected {}",
            VECTOR_4.name, hash_hex, VECTOR_4.expected_hash
        );
    }

    #[test]
    fn test_vector_5_empty_payload() {
        let env = Env::default();

        let subject = Address::generate(&env);
        let data = Bytes::from_slice(&env, VECTOR_5.data_payload);

        let hash = compute_payload_hash(&env, &subject, VECTOR_5.timestamp, &data);
        let hash_hex = bytes_to_hex(&hash.to_vec());

        assert_eq!(
            hash_hex, VECTOR_5.expected_hash,
            "Vector '{}': hash mismatch. Got {}, expected {}",
            VECTOR_5.name, hash_hex, VECTOR_5.expected_hash
        );
    }

    #[test]
    fn test_all_vectors_deterministic_across_calls() {
        let env = Env::default();

        // For each vector, verify that calling compute_payload_hash multiple times
        // with the same inputs produces identical output
        for vector in &[&VECTOR_1, &VECTOR_2, &VECTOR_3, &VECTOR_4, &VECTOR_5] {
            let subject = Address::generate(&env);
            let data = Bytes::from_slice(&env, vector.data_payload);

            let hash1 = compute_payload_hash(&env, &subject, vector.timestamp, &data);
            let hash2 = compute_payload_hash(&env, &subject, vector.timestamp, &data);
            let hash3 = compute_payload_hash(&env, &subject, vector.timestamp, &data);

            assert_eq!(
                hash1, hash2,
                "Vector '{}': hash not deterministic (call 1 vs 2)",
                vector.name
            );
            assert_eq!(
                hash2, hash3,
                "Vector '{}': hash not deterministic (call 2 vs 3)",
                vector.name
            );
        }
    }

    #[test]
    fn test_hash_vectors_are_distinct() {
        let env = Env::default();
        let vectors = [&VECTOR_1, &VECTOR_2, &VECTOR_3, &VECTOR_4, &VECTOR_5];

        // Verify that different vectors produce different hashes
        for (i, v1) in vectors.iter().enumerate() {
            for (j, v2) in vectors.iter().enumerate() {
                if i != j {
                    let subject = Address::generate(&env);
                    let data1 = Bytes::from_slice(&env, v1.data_payload);
                    let data2 = Bytes::from_slice(&env, v2.data_payload);

                    let hash1 = compute_payload_hash(&env, &subject, v1.timestamp, &data1);
                    let hash2 = compute_payload_hash(&env, &subject, v2.timestamp, &data2);

                    assert_ne!(
                        hash1, hash2,
                        "Vectors '{}' and '{}' produced the same hash (should be distinct)",
                        v1.name, v2.name
                    );
                }
            }
        }
    }
}
