# ADR 0001: Adoption of Architecture Decision Records (ADRs)

- **Status:** Accepted
- **Date:** 2026-07-25
- **Authors:** @joan-bisbal

---

## Context

As the SorobanAnchor protocol grows in complexity across SEP-6, SEP-10, and SEP-24 modules, design decisions must be documented in a structured, version-controlled format alongside the codebase.

---

## Decision

We adopt the Architecture Decision Record (ADR) format stored in `docs/adr/`. Each ADR will use the following structure:
1. Title and Number
2. Status (Proposed, Accepted, Deprecated, Superseded)
3. Context and Problem Statement
4. Decision Outcome
5. Consequences (Positive and Negative)

---

## Consequences

- **Positive:** Standardized architectural documentation across all anchor sub-systems.
- **Positive:** Improved onboarding clarity for open-source contributors.
- **Negative:** Minor overhead in submitting ADR PRs for structural changes.

---

## References

- Issue reference: Fixes #694
