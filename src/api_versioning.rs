//! Backend API versioning policy and compatibility layer (issue #307).
//!
//! # Versioning policy
//! - Routes are prefixed with `/v<N>/` (e.g. `/v1/`, `/v2/`).
//! - Additive changes (new optional fields, new endpoints) are non-breaking and
//!   do not require a version bump.
//! - Breaking changes (removed fields, changed types, removed endpoints) require
//!   a new major version.
//! - Deprecated versions emit a `Deprecation` header and a `Sunset` date.
//! - A version is supported for at least 6 months after its successor ships.
//!
//! # Supported versions
//! | Version | Status     | Sunset date  |
//! |---------|------------|--------------|
//! | v1      | deprecated | 2026-12-01   |
//! | v2      | current    | —            |
//!
//! This module provides:
//! - `ApiVersion` enum with stable discriminants.
//! - `VersionPolicy` that resolves a version from a path prefix and attaches
//!   deprecation metadata to responses.
//! - `ResponseEnvelope` — the canonical response shape shared across versions.
//! - Schema snapshot helpers used by CI to detect breaking changes.

extern crate alloc;
use alloc::{
    string::{String, ToString},
    vec::Vec,
};

// ---------------------------------------------------------------------------
// Supported versions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiVersion {
    V1 = 1,
    V2 = 2,
}

impl ApiVersion {
    pub fn from_path_prefix(path: &str) -> Option<Self> {
        if path.starts_with("/v1/") || path == "/v1" {
            Some(Self::V1)
        } else if path.starts_with("/v2/") || path == "/v2" {
            Some(Self::V2)
        } else {
            None
        }
    }

    pub fn is_deprecated(self) -> bool {
        matches!(self, Self::V1)
    }

    /// RFC 7231 date string after which the version will be removed.
    pub fn sunset_date(self) -> Option<&'static str> {
        match self {
            Self::V1 => Some("2026-12-01"),
            Self::V2 => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
        }
    }
}

// ---------------------------------------------------------------------------
// Deprecation metadata attached to responses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct DeprecationMeta {
    pub version: String,
    pub sunset: String,
    pub migration_notice: String,
}

// ---------------------------------------------------------------------------
// Canonical response envelope
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ResponseEnvelope<T> {
    pub version: String,
    pub data: T,
    pub deprecation: Option<DeprecationMeta>,
}

// ---------------------------------------------------------------------------
// Version policy
// ---------------------------------------------------------------------------

pub struct VersionPolicy {
    pub current: ApiVersion,
    pub supported: Vec<ApiVersion>,
}

impl Default for VersionPolicy {
    fn default() -> Self {
        Self {
            current: ApiVersion::V2,
            supported: alloc::vec![ApiVersion::V1, ApiVersion::V2],
        }
    }
}

impl VersionPolicy {
    /// Resolve the API version from a request path.
    /// Returns `None` if the path does not match any supported version.
    pub fn resolve(&self, path: &str) -> Option<ApiVersion> {
        let v = ApiVersion::from_path_prefix(path)?;
        if self.supported.contains(&v) { Some(v) } else { None }
    }

    /// Wrap `data` in a `ResponseEnvelope`, attaching deprecation metadata when
    /// the resolved version is deprecated.
    pub fn wrap<T>(&self, version: ApiVersion, data: T) -> ResponseEnvelope<T> {
        let deprecation = if version.is_deprecated() {
            Some(DeprecationMeta {
                version: version.as_str().into(),
                sunset: version.sunset_date().unwrap_or("").into(),
                migration_notice: alloc::format!(
                    "This endpoint version ({}) is deprecated. Migrate to /{}/.",
                    version.as_str(),
                    self.current.as_str(),
                ),
            })
        } else {
            None
        };
        ResponseEnvelope { version: version.as_str().into(), data, deprecation }
    }
}

// ---------------------------------------------------------------------------
// Schema snapshot helpers (used by CI compatibility checks)
// ---------------------------------------------------------------------------

/// A minimal field descriptor for snapshot-based schema diffing.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDescriptor {
    pub name: String,
    pub required: bool,
}

/// Snapshot of a response schema at a given version.
#[derive(Debug, Clone)]
pub struct SchemaSnapshot {
    pub version: ApiVersion,
    pub endpoint: String,
    pub fields: Vec<FieldDescriptor>,
}

impl SchemaSnapshot {
    /// Returns field names present in `baseline` but missing from `current`.
    /// Any such removal is a breaking change.
    pub fn breaking_removals<'a>(baseline: &'a Self, current: &'a Self) -> Vec<&'a str> {
        baseline
            .fields
            .iter()
            .filter(|f| f.required)
            .filter(|f| !current.fields.iter().any(|cf| cf.name == f.name))
            .map(|f| f.name.as_str())
            .collect()
    }
}
