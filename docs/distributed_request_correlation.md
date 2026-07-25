# Distributed Request Correlation Specification

Architecture specification for propagating `X-Correlation-ID` headers and trace contexts across distributed SorobanAnchor services.

---

## 1. Trace Context Schema

```http
X-Correlation-ID: 9ceb789e-ade0-4823-a996-943475647081
X-Parent-Span-ID: 7b9e8402-4823-4996
```

---

## References

- Issue reference: Fixes #684
