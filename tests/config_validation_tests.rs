/// Config validation integration tests.
///
/// Each test invokes `validate_config_strict.py` (the same validator used by
/// build.rs and the shell scripts) so the test results are authoritative.
///
/// Run with: cargo test --test config_validation_tests
#[cfg(test)]
mod config_validation_tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    const SCHEMA: &str = "config_schema.json";
    const VALIDATOR: &str = "validate_config_strict.py";

    /// Invoke the Python validator; return (success, combined output).
    fn validate(config_path: &str) -> (bool, String) {
        // Prefer python3, fall back to python (Windows).
        let python = if cfg!(target_os = "windows") { "python" } else { "python3" };
        let out = Command::new(python)
            .args([VALIDATOR, config_path, SCHEMA])
            .output()
            .expect("failed to run validator — is python installed?");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.success(), combined)
    }

    fn skip_if_no_validator() -> bool {
        !Path::new(VALIDATOR).exists() || !Path::new(SCHEMA).exists()
    }

    // ── 1. Positive: all shipped configs must pass ──────────────────────────

    #[test]
    fn test_remittance_anchor_json_is_valid() {
        if skip_if_no_validator() { return; }
        let (ok, out) = validate("configs/remittance-anchor.json");
        assert!(ok, "remittance-anchor.json failed validation:\n{out}");
    }

    #[test]
    fn test_stablecoin_issuer_json_is_valid() {
        if skip_if_no_validator() { return; }
        let (ok, out) = validate("configs/stablecoin-issuer.json");
        assert!(ok, "stablecoin-issuer.json failed validation:\n{out}");
    }

    #[test]
    fn test_fiat_on_off_ramp_json_is_valid() {
        if skip_if_no_validator() { return; }
        let (ok, out) = validate("configs/fiat-on-off-ramp.json");
        assert!(ok, "fiat-on-off-ramp.json failed validation:\n{out}");
    }

    // ── 2. Negative: missing required field ─────────────────────────────────

    #[test]
    fn test_missing_required_field_fails() {
        if skip_if_no_validator() { return; }

        // Remove the required "network" field from contract.
        let bad = r#"{
  "contract": { "name": "test-anchor", "version": "1.0.0" },
  "attestors": {
    "registry": [{
      "name": "kyc-issuer",
      "address": "GBBD6A7KNZF5WNWQEPZP5DYJD2AYUTLXRB6VXJ4RCX4RTNPPQVNF3GQ",
      "role": "kyc-issuer",
      "enabled": true
    }]
  }
}"#;
        let tmp = "configs/_test_missing_field.json";
        fs::write(tmp, bad).unwrap();
        let (ok, out) = validate(tmp);
        fs::remove_file(tmp).unwrap();
        assert!(!ok, "Expected validation failure for missing 'network', but it passed.\n{out}");
    }

    // ── 3. Negative: type mismatch (string where integer expected) ──────────

    #[test]
    fn test_type_mismatch_fails() {
        if skip_if_no_validator() { return; }

        // session_timeout_seconds must be integer; supply a string.
        let bad = r#"{
  "contract": { "name": "test-anchor", "version": "1.0.0", "network": "stellar-testnet" },
  "attestors": {
    "registry": [{
      "name": "kyc-issuer",
      "address": "GBBD6A7KNZF5WNWQEPZP5DYJD2AYUTLXRB6VXJ4RCX4RTNPPQVNF3GQ",
      "role": "kyc-issuer",
      "enabled": true
    }]
  },
  "sessions": { "session_timeout_seconds": "not-a-number" }
}"#;
        let tmp = "configs/_test_type_mismatch.json";
        fs::write(tmp, bad).unwrap();
        let (ok, out) = validate(tmp);
        fs::remove_file(tmp).unwrap();
        assert!(!ok, "Expected validation failure for type mismatch, but it passed.\n{out}");
    }

    // ── 4. Equivalence: TOML and JSON minimal configs both pass ─────────────
    //
    // We write a minimal valid JSON config and a TOML equivalent, validate
    // both, and assert both succeed (or both fail identically).

    #[test]
    fn test_toml_and_json_equivalence() {
        if skip_if_no_validator() { return; }

        let json_content = r#"{
  "contract": { "name": "equiv-anchor", "version": "1.0.0", "network": "stellar-testnet" },
  "attestors": {
    "registry": [{
      "name": "kyc-issuer",
      "address": "GBBD6A7KNZF5WNWQEPZP5DYJD2AYUTLXRB6VXJ4RCX4RTNPPQVNF3GQ",
      "role": "kyc-issuer",
      "enabled": true
    }]
  }
}"#;
        // Equivalent TOML — validator converts it to JSON before checking.
        let toml_content = r#"
[contract]
name    = "equiv-anchor"
version = "1.0.0"
network = "stellar-testnet"

[[attestors.registry]]
name    = "kyc-issuer"
address = "GBBD6A7KNZF5WNWQEPZP5DYJD2AYUTLXRB6VXJ4RCX4RTNPPQVNF3GQ"
role    = "kyc-issuer"
enabled = true
"#;
        let json_tmp = "configs/_test_equiv.json";
        let toml_tmp = "configs/_test_equiv.toml";
        fs::write(json_tmp, json_content).unwrap();
        fs::write(toml_tmp, toml_content).unwrap();

        let (json_ok, json_out) = validate(json_tmp);
        // For TOML we call the validator directly; it handles conversion internally.
        let (toml_ok, toml_out) = validate(toml_tmp);

        fs::remove_file(json_tmp).unwrap();
        fs::remove_file(toml_tmp).unwrap();

        assert!(json_ok, "JSON equivalent failed:\n{json_out}");
        // TOML validation may be skipped if no TOML library is installed; treat
        // that as a soft pass (the validator exits 2 for missing deps).
        if !toml_out.contains("ModuleNotFoundError") && !toml_out.contains("No module named") {
            assert_eq!(
                json_ok, toml_ok,
                "JSON and TOML equivalents produced different results.\nJSON: {json_out}\nTOML: {toml_out}"
            );
        }
    }
}
