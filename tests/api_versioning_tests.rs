//! Tests for the API versioning policy and backward-compatibility checks.
//! Closes #307.

#[cfg(test)]
mod api_versioning_tests {
    use anchorkit::api_versioning::{
        ApiVersion, FieldDescriptor, SchemaSnapshot, VersionPolicy,
    };

    fn policy() -> VersionPolicy {
        VersionPolicy::default()
    }

    // ── Path resolution ──────────────────────────────────────────────────────

    #[test]
    fn v1_path_resolves_to_v1() {
        assert_eq!(policy().resolve("/v1/products"), Some(ApiVersion::V1));
    }

    #[test]
    fn v2_path_resolves_to_v2() {
        assert_eq!(policy().resolve("/v2/products"), Some(ApiVersion::V2));
    }

    #[test]
    fn unknown_version_returns_none() {
        assert_eq!(policy().resolve("/v99/products"), None);
        assert_eq!(policy().resolve("/products"), None);
    }

    // ── Deprecation metadata ─────────────────────────────────────────────────

    #[test]
    fn v1_is_deprecated() {
        assert!(ApiVersion::V1.is_deprecated());
    }

    #[test]
    fn v2_is_not_deprecated() {
        assert!(!ApiVersion::V2.is_deprecated());
    }

    #[test]
    fn deprecated_version_emits_deprecation_envelope() {
        let p = policy();
        let env = p.wrap(ApiVersion::V1, "payload");
        let dep = env.deprecation.expect("v1 must carry deprecation metadata");
        assert_eq!(dep.version, "v1");
        assert_eq!(dep.sunset, "2026-12-01");
        assert!(dep.migration_notice.contains("v2"));
    }

    #[test]
    fn current_version_has_no_deprecation_metadata() {
        let p = policy();
        let env = p.wrap(ApiVersion::V2, "payload");
        assert!(env.deprecation.is_none());
    }

    // ── Old-version behaviour ────────────────────────────────────────────────

    #[test]
    fn v1_response_envelope_carries_version_string() {
        let p = policy();
        let env = p.wrap(ApiVersion::V1, 42u32);
        assert_eq!(env.version, "v1");
        assert_eq!(env.data, 42u32);
    }

    // ── Schema snapshot / breaking-change detection ──────────────────────────

    fn field(name: &str, required: bool) -> FieldDescriptor {
        FieldDescriptor { name: name.into(), required }
    }

    fn snapshot(version: ApiVersion, fields: Vec<FieldDescriptor>) -> SchemaSnapshot {
        SchemaSnapshot { version, endpoint: "/products".into(), fields }
    }

    #[test]
    fn no_breaking_changes_when_schemas_identical() {
        let baseline = snapshot(ApiVersion::V2, vec![field("id", true), field("name", true)]);
        let current = snapshot(ApiVersion::V2, vec![field("id", true), field("name", true)]);
        assert!(SchemaSnapshot::breaking_removals(&baseline, &current).is_empty());
    }

    #[test]
    fn additive_field_is_not_breaking() {
        let baseline = snapshot(ApiVersion::V2, vec![field("id", true)]);
        let current = snapshot(ApiVersion::V2, vec![field("id", true), field("extra", false)]);
        assert!(SchemaSnapshot::breaking_removals(&baseline, &current).is_empty());
    }

    #[test]
    fn removed_required_field_is_breaking() {
        let baseline = snapshot(ApiVersion::V2, vec![field("id", true), field("name", true)]);
        let current = snapshot(ApiVersion::V2, vec![field("id", true)]); // "name" removed
        let removals = SchemaSnapshot::breaking_removals(&baseline, &current);
        assert_eq!(removals, vec!["name"]);
    }

    #[test]
    fn removed_optional_field_is_not_breaking() {
        let baseline = snapshot(ApiVersion::V2, vec![field("id", true), field("hint", false)]);
        let current = snapshot(ApiVersion::V2, vec![field("id", true)]);
        assert!(SchemaSnapshot::breaking_removals(&baseline, &current).is_empty());
    }

    // ── Sunset date ──────────────────────────────────────────────────────────

    #[test]
    fn v1_has_sunset_date() {
        assert_eq!(ApiVersion::V1.sunset_date(), Some("2026-12-01"));
    }

    #[test]
    fn v2_has_no_sunset_date() {
        assert_eq!(ApiVersion::V2.sunset_date(), None);
    }
}
