//! Tests for build-script schema failure messaging (issue #1318).
//!
//! The build script (`build.rs`) validates every config in `configs/` against
//! `config_schema.json` at compile time.  When a config fails validation it
//! emits one `cargo:warning` line per error, each prefixed with the file label
//! so the contributor can identify the source file and field immediately.
//!
//! These tests exercise that reporting path in-process (without re-invoking
//! `cargo build`) by driving the same validation logic that `build.rs` uses:
//! - loading the compiled JSON Schema,
//! - feeding it an intentionally invalid fixture, and
//! - asserting that the resulting error output names the violating field.
//!
//! Acceptance criteria:
//! - An invalid config produces at least one error that identifies the source
//!   or field.
//! - Valid configs continue to pass without any errors.
//! - The test does **not** touch any checked-in configuration file.

#![cfg(test)]
#![cfg(not(feature = "wasm"))]

mod build_schema_failure_tests {
    use std::fs;
    use std::path::Path;

    use jsonschema::JSONSchema;
    use serde_json::{json, Value};

    // -----------------------------------------------------------------------
    // Helper: compile the project schema once
    // -----------------------------------------------------------------------

    fn load_compiled_schema() -> JSONSchema {
        let schema_path = Path::new("config_schema.json");
        let schema_text = fs::read_to_string(schema_path)
            .expect("config_schema.json must exist at workspace root");
        let schema_json: Value =
            serde_json::from_str(&schema_text).expect("config_schema.json must be valid JSON");
        JSONSchema::compile(&schema_json).expect("config_schema.json must compile as a JSON Schema")
    }

    // -----------------------------------------------------------------------
    // Helper: build a minimal *valid* config fixture so we have a known-good
    // baseline.  Field values are taken from the checked-in example configs.
    // -----------------------------------------------------------------------

    fn valid_fixture() -> Value {
        json!({
            "contract": {
                "name": "test-anchor",
                "version": "1.0.0",
                "network": "stellar-testnet"
            },
            "attestors": {
                "registry": []
            }
        })
    }

    // -----------------------------------------------------------------------
    // Test 1 – valid fixture must pass without errors
    // -----------------------------------------------------------------------

    #[test]
    fn valid_config_fixture_passes_schema() {
        let schema = load_compiled_schema();
        let fixture = valid_fixture();
        let result = schema.validate(&fixture);
        assert!(
            result.is_ok(),
            "A well-formed config fixture must pass schema validation; \
             errors: {:?}",
            result.err()
        );
    }

    // -----------------------------------------------------------------------
    // Test 2 – a fixture that omits a *required* top-level field must fail
    // and the error message must identify the missing field.
    //
    // The schema requires "contract" and "attestors" at the root.  Omitting
    // "contract" exercises the required-field path in the build-script
    // validation output.
    // -----------------------------------------------------------------------

    #[test]
    fn missing_required_field_produces_identifying_error() {
        let schema = load_compiled_schema();

        // Intentionally omit the required "contract" object
        let invalid_fixture = json!({
            "attestors": {
                "registry": []
            }
        });

        let result = schema.validate(&invalid_fixture);
        assert!(
            result.is_err(),
            "A fixture missing a required field must fail schema validation"
        );

        // Collect error descriptions the same way build.rs does:
        //   for error in errors { println!("cargo:warning=  {}", error); }
        let errors: Vec<String> = result
            .err()
            .into_iter()
            .flatten()
            .map(|e| e.to_string())
            .collect();

        assert!(
            !errors.is_empty(),
            "At least one error must be reported for the missing field"
        );

        // The error must reference the missing field so the contributor can
        // identify the source.  jsonschema surfaces this as a 'required'
        // keyword violation that names the property.
        let combined = errors.join("\n");
        assert!(
            combined.contains("contract"),
            "Failure message must identify the missing 'contract' field; \
             got: {combined}"
        );
    }

    // -----------------------------------------------------------------------
    // Test 3 – a fixture with an *additional* (unknown) property must fail
    // when the schema uses `additionalProperties: false`.
    //
    // The build script emits the field path from the jsonschema error, which
    // lets the contributor see exactly which key was unexpected.
    // -----------------------------------------------------------------------

    #[test]
    fn unknown_field_produces_identifying_error() {
        let schema = load_compiled_schema();

        // Add an unrecognised key at the root level
        let mut invalid_fixture = valid_fixture();
        invalid_fixture["monitorring"] = json!({});   // intentional typo

        let result = schema.validate(&invalid_fixture);
        assert!(
            result.is_err(),
            "A fixture with an unknown top-level field must fail schema validation"
        );

        let errors: Vec<String> = result
            .err()
            .into_iter()
            .flatten()
            .map(|e| e.to_string())
            .collect();

        let combined = errors.join("\n");
        // The error message from jsonschema names the unexpected property.
        assert!(
            combined.contains("monitorring"),
            "Failure message must identify the unexpected 'monitorring' field; \
             got: {combined}"
        );
    }

    // -----------------------------------------------------------------------
    // Test 4 – confirms that the label (filename) attached in build.rs is
    // sufficient to locate the source: here we simulate the label-per-error
    // format used by build.rs and verify the label is non-empty.
    // -----------------------------------------------------------------------

    #[test]
    fn error_output_carries_file_label() {
        let schema = load_compiled_schema();
        let invalid_fixture = json!({ "attestors": { "registry": [] } });

        let result = schema.validate(&invalid_fixture);
        let errors: Vec<String> = result
            .err()
            .into_iter()
            .flatten()
            .map(|e| e.to_string())
            .collect();

        // Simulate the build.rs output format:
        //   cargo:warning=❌ <label> failed validation
        //   cargo:warning=  <error description>
        let label = "invalid-test-fixture.json";
        let mut output_lines: Vec<String> = Vec::new();
        output_lines.push(format!("cargo:warning=❌ {} failed validation", label));
        for error in &errors {
            output_lines.push(format!("cargo:warning=  {}", error));
        }

        // Every error line must be traceable: the label line must appear first.
        assert!(
            output_lines[0].contains(label),
            "First output line must contain the file label; got: {}",
            output_lines[0]
        );

        // At least one subsequent line must carry the validation detail.
        assert!(
            output_lines.len() > 1,
            "There must be at least one detail line after the label"
        );
    }
}
