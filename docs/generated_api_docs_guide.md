# Generated API Documentation Pipeline Guide

Guide for automated cargo rustdoc generation and OpenAPI endpoint documentation deployment for the SorobanAnchor protocol.

---

## 1. Automated Rustdoc Generation

```bash
cargo doc --no-deps --workspace --all-features
```

Output is published automatically to GitHub Pages at `https://abore9769.github.io/SorobanAnchor/docs/`.

---

## 2. API Endpoint Snapshot Verification

- OpenAPI 3.0 specs generated for SEP-6 and SEP-24 endpoints.
- CI pipeline validates that PRs do not break backward compatibility of response schemas.

---

## References

- Issue reference: Fixes #693
