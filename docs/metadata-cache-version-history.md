# Metadata Cache and Version History Reference

This document describes how the metadata cache and the metadata version history behave as currently implemented and tested. It covers the cache key, the version indicator, and expiry semantics, and explains how consumers distinguish cached current data from historical metadata.

All statements below reflect only exposed fields and public lookup behavior. Where the implementation does not provide a guarantee, this document does not claim one.

## Cache key

The metadata cache is keyed by the anchor identifier. A cached entry is stored and looked up under the anchor it describes, so `cache_metadata` writes an entry for a given anchor and `get_cached_metadata` retrieves the entry for that same anchor. The key does not incorporate a version number; versioning is handled separately by the version history (see below).

## Version indicator

Each cached metadata entry carries a version indicator. The version indicator is a monotonically increasing counter associated with the anchor's metadata:

- `get_anchor_meta_version_count` returns the number of metadata versions recorded for an anchor.
- `get_anchor_metadata_at_version` returns the metadata recorded at a specific version.
- `get_anchor_metadata_history` returns the recorded history of metadata versions for an anchor.

The version indicator identifies which revision of the metadata a value corresponds to. It is the field consumers use to tell cached current data apart from historical metadata: the cached entry reflects the current metadata, while the version history exposes prior revisions retrievable by version.

## Expiry semantics

Cached entries are subject to expiry as implemented in the cache. An entry that has expired is no longer treated as current cached data by `get_cached_metadata`. Expiry governs only whether the cache considers an entry fresh; it does not alter the recorded version history, which remains available through the version history lookups.

## Cache freshness versus source authenticity

A fresh cache entry means the cache considers the entry current according to its own expiry rules. It does not attest to the authenticity, correctness, or provenance of the underlying metadata. Consumers that require assurance about the source of metadata must obtain that assurance through means other than cache freshness.

## Persistence

This document makes no claim about persistence of cached entries or version history across restarts or storage boundaries beyond what the implementation and tests demonstrate. Consult the tests for the exact behavior exercised.

## Tests

The behavior described here is exercised by:

- [`tests/metadata_cache_tests.rs`](../tests/metadata_cache_tests.rs) — cache key, `cache_metadata`, `get_cached_metadata`, and expiry behavior.
- [`tests/metadata_version_history_tests.rs`](../tests/metadata_version_history_tests.rs) — version indicator, `get_anchor_metadata_history`, `get_anchor_metadata_at_version`, and `get_anchor_meta_version_count`.