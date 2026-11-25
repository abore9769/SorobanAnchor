# Artifact Provenance Inspection

This document explains how to inspect artifact provenance metadata for
SorobanAnchor release builds, helping you establish which build inputs
correspond to a given release artifact.

---

## Table of Contents

1. [What is artifact provenance](#what-is-artifact-provenance)
2. [Provenance record structure](#provenance-record-structure)
3. [Inspecting a provenance record](#inspecting-a-provenance-record)
4. [Interpreting fields](#interpreting-fields)
5. [Scope and limitations](#scope-and-limitations)
6. [References](#references)

---

## What is artifact provenance

Artifact provenance is a record of the build inputs, environment, and commands
used to produce a release artifact. It answers:

- Which source revision was built?
- What build command was used?
- What environment variables or toolchain versions were active?
- When was the artifact produced?

Provenance records help users confirm that a published binary or WASM contract
matches the expected source code and build process.

**What provenance does:**

- Traces an artifact to a specific commit and build configuration.
- Provides audit data for compliance and security review.
- Helps reproduce a build by documenting inputs.

**What provenance does not do:**

- Provenance metadata is **self-attested** by the build system. It is not
  independently verified unless combined with an external attestation service
  (e.g., SLSA provenance, Sigstore). SorobanAnchor does not currently produce
  external attestations.
- Provenance does not prove that the source code is safe or correct.
- Provenance does not replace signature verification. Always verify detached
  signatures (see [release-signing.md](release-signing.md)) to authenticate
  the publisher.

---

## Provenance record structure

A provenance record for a single artifact contains the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `provenance_id` | `u64` | Monotonically increasing record identifier |
| `artifact_name` | `String` | Name of the artifact (e.g., `anchorkit-0.2.0-wasm32.wasm`) |
| `content_hash` | `String` | Hex-encoded SHA-256 hash of the artifact file |
| `source_revision` | `Option<String>` | Git commit SHA or other VCS revision identifier |
| `source_repository` | `Option<String>` | Source repository URI (e.g., `https://github.com/org/repo`) |
| `build_command` | `Option<String>` | Exact build command used (e.g., `cargo build --release --target wasm32-unknown-unknown`) |
| `build_env` | `Vec<(String, String)>` | Key/value pairs describing the build environment (e.g., `rustc` version, OS) |
| `recorded_at` | `u64` | Unix timestamp when the record was created |
| `signature` | `Option<String>` | Optional signature over the content hash for non-repudiation |

A record is considered **complete** when `source_revision`, `source_repository`,
and `build_command` are all populated.

---

## Inspecting a provenance record

### Example: Synthetic provenance record

Below is a synthetic example of a provenance record for `anchorkit-0.2.0.wasm`:

```json
{
  "provenance_id": 42,
  "artifact_name": "anchorkit-0.2.0.wasm",
  "content_hash": "a3f8c12d7e4b56789abcdef0123456789abcdef0123456789abcdef012345678",
  "source_revision": "e8f7c3a1b2d4567890abcdef1234567890abcdef",
  "source_repository": "https://github.com/abore9769/SorobanAnchor",
  "build_command": "cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm",
  "build_env": [
    ["rustc", "1.80.0 (051478957 2024-07-21)"],
    ["CARGO_CFG_TARGET_ARCH", "wasm32"],
    ["SOURCE_DATE_EPOCH", "1717200000"],
    ["CI", "true"]
  ],
  "recorded_at": 1725000000,
  "signature": null
}
```

### Reading the record

1. **Artifact identity:** The `artifact_name` and `content_hash` together
   uniquely identify the file. You can verify the hash matches by running:

   ```bash
   sha256sum anchorkit-0.2.0.wasm
   ```

2. **Source traceability:** The `source_revision` and `source_repository`
   fields tell you which commit produced this artifact. You can inspect the
   source code at that revision:

   ```bash
   git clone https://github.com/abore9769/SorobanAnchor
   cd SorobanAnchor
   git checkout e8f7c3a1b2d4567890abcdef1234567890abcdef
   ```

3. **Build reproducibility:** The `build_command` and `build_env` fields
   document how to reproduce the build. You can attempt to reproduce it:

   ```bash
   export SOURCE_DATE_EPOCH=1717200000
   cargo build --release --target wasm32-unknown-unknown \
       --no-default-features --features wasm
   sha256sum target/wasm32-unknown-unknown/release/anchorkit.wasm
   ```

   For full reproducibility guidance, see
   [REPRODUCIBLE_BUILDS.md](REPRODUCIBLE_BUILDS.md).

4. **Timestamp:** The `recorded_at` field (Unix timestamp) indicates when the
   provenance record was created. Convert to human-readable format:

   ```bash
   date -d @1725000000
   # Fri Aug 30 06:40:00 UTC 2024
   ```

---

## Interpreting fields

### `source_revision`

- **What it means:** The exact commit that was checked out before the build.
- **How to use it:** Compare this value to the tag or branch you expect. If a
  release is tagged `v0.2.0`, the `source_revision` should match the commit
  at that tag.
- **What it does not mean:** The revision field does not prove that the commit
  is safe, authorized, or reviewed. Always inspect the commit history and code
  changes independently.

### `build_command`

- **What it means:** The exact command line used to invoke the build.
- **How to use it:** You can copy this command to attempt a reproduction. Note
  that environment variables (recorded in `build_env`) may also affect the
  output.
- **What it does not mean:** The command does not prove that the resulting
  artifact is safe. A malicious build command could inject backdoors. Always
  review the build scripts and toolchain before trusting a build.

### `build_env`

- **What it means:** Key/value pairs describing the build environment. Common
  entries include:
  - `rustc` — Rust compiler version
  - `CARGO_CFG_TARGET_ARCH` — Target architecture
  - `SOURCE_DATE_EPOCH` — Timestamp override for reproducible builds
  - `CI` — Whether the build ran in a CI environment
- **How to use it:** Match the environment to your reproduction attempt. For
  example, if the record shows `rustc 1.80.0`, install that version:

  ```bash
  rustup install 1.80.0
  rustup default 1.80.0
  ```

### `signature`

- **What it means:** An optional signature over the `content_hash` for
  non-repudiation. If present, it ties the provenance record to a signing
  identity.
- **How to use it:** Verify the signature using the publisher's public key
  (same process as verifying release signatures). If `signature` is `null`,
  the provenance record is unauthenticated.

---

## Scope and limitations

### Provenance is self-attested

Provenance records are generated by the build system itself. They are only as
trustworthy as:

1. The build environment (local machine, CI runner).
2. The access controls on that environment.
3. The integrity of the build scripts that generate provenance.

An attacker with access to the build environment could forge provenance
records. Provenance is **not** an independent attestation.

### No external attestation

SorobanAnchor does not currently integrate with external attestation systems
like SLSA provenance or Sigstore. Provenance records are stored alongside
artifacts but are not cryptographically bound to a third-party transparency
log.

If you require independently verifiable provenance, consider integrating:

- **SLSA provenance:** Cryptographically signed build metadata with external
  verification.
- **Sigstore:** Keyless signing with transparency log backing.

### Provenance does not replace signature verification

Always verify detached signatures (`.sig` or `.minisig` files) before trusting
an artifact. Signatures authenticate the **publisher** identity; provenance
documents the **build process**.

Both are complementary:

| Verification method | What it proves |
|---------------------|----------------|
| Signature verification | The artifact was published by a trusted identity |
| Provenance inspection | The artifact was built from a known source and configuration |

Use both together for maximum assurance.

---

## References

- [release-signing.md](release-signing.md) — How to verify signatures
- [REPRODUCIBLE_BUILDS.md](REPRODUCIBLE_BUILDS.md) — How to reproduce builds
- [src/artifact_provenance.rs](../src/artifact_provenance.rs) — Provenance
  record implementation
- [Governance and Security](governance-and-security.md) — Overall security model
- [Audit log and provenance field sensitivity](audit-log-sensitive-fields.md) —
  What must stay out of `build_command`, `build_env`, and `source_repository`

