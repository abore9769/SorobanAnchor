import React from "react";

// ─── Error categories ─────────────────────────────────────────────────────────

export type ErrorCategory =
  | "network"
  | "authentication"
  | "contract"
  | "validation"
  | "unknown";

function categorize(error: Error): ErrorCategory {
  const msg = error.message.toLowerCase();
  if (msg.includes("network") || msg.includes("fetch") || msg.includes("timeout") || msg.includes("connection"))
    return "network";
  if (msg.includes("auth") || msg.includes("jwt") || msg.includes("token") || msg.includes("unauthorized") || msg.includes("sep-10") || msg.includes("sep10"))
    return "authentication";
  if (msg.includes("contract") || msg.includes("soroban") || msg.includes("stellar") || msg.includes("wasm"))
    return "contract";
  if (msg.includes("valid") || msg.includes("schema") || msg.includes("required") || msg.includes("invalid"))
    return "validation";
  return "unknown";
}

const CATEGORY_CONFIG: Record<ErrorCategory, { title: string; message: string; fundsMessage: string | null }> = {
  network: {
    title: "Network Error",
    message: "A network error occurred. Please check your connection and try again.",
    fundsMessage: "Your funds are safe — no transaction was submitted.",
  },
  authentication: {
    title: "Authentication Error",
    message: "Your session has expired or your credentials are invalid. Please sign in again.",
    fundsMessage: "Your funds are safe — no transaction was submitted.",
  },
  contract: {
    title: "Contract Error",
    message: "An error occurred while communicating with the Stellar contract.",
    fundsMessage: null, // may have been submitted; show status link instead
  },
  validation: {
    title: "Validation Error",
    message: "The request data is invalid. Please review your inputs and try again.",
    fundsMessage: "Your funds are safe — no transaction was submitted.",
  },
  unknown: {
    title: "Unexpected Error",
    message: "An unexpected error occurred. Please try again or contact support.",
    fundsMessage: "Your funds are safe — no transaction was submitted.",
  },
};

// ─── Props / State ────────────────────────────────────────────────────────────

export interface AnchorErrorBoundaryProps {
  children: React.ReactNode;
  /** Label shown in the error UI (e.g. "SEP-10 Auth Flow") */
  componentLabel?: string;
  /** Transaction ID to link to the status page when available */
  transactionId?: string;
  /** URL of the transaction status page */
  statusPageUrl?: string;
  /** Endpoint to POST error details to */
  reportingEndpoint?: string;
  /** Fallback UI override */
  fallback?: React.ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
  errorInfo: React.ErrorInfo | null;
  /** Incremented to force re-mount of children on retry */
  retryKey: number;
}

// ─── Component ────────────────────────────────────────────────────────────────

export class AnchorErrorBoundary extends React.Component<AnchorErrorBoundaryProps, State> {
  constructor(props: AnchorErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null, errorInfo: null, retryKey: 0 };
  }

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    this.setState({ errorInfo: info });
    this.reportError(error, info);
  }

  private reportError(error: Error, info: React.ErrorInfo) {
    const { reportingEndpoint, componentLabel } = this.props;
    const payload = {
      component: componentLabel ?? "unknown",
      message: error.message,
      stack: error.stack,
      componentStack: info.componentStack,
      timestamp: new Date().toISOString(),
    };
    console.error("[AnchorErrorBoundary]", payload);
    if (reportingEndpoint) {
      fetch(reportingEndpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      }).catch(() => {/* best-effort */});
    }
  }

  private handleRetry = () => {
    this.setState(s => ({ hasError: false, error: null, errorInfo: null, retryKey: s.retryKey + 1 }));
  };

  render() {
    const { hasError, error, retryKey } = this.state;
    const { children, fallback, transactionId, statusPageUrl } = this.props;

    if (!hasError) {
      return <React.Fragment key={retryKey}>{children}</React.Fragment>;
    }

    if (fallback) return fallback;

    const category = error ? categorize(error) : "unknown";
    const cfg = CATEGORY_CONFIG[category];

    // Extract request ID from error message if present (format: "requestId:xxx")
    const requestIdMatch = error?.message.match(/requestId[:\s]+([a-zA-Z0-9_-]+)/i);
    const requestId = requestIdMatch?.[1] ?? null;

    return (
      <div
        role="alert"
        data-testid="anchor-error-boundary"
        data-error-category={category}
        style={{
          padding: "20px 24px",
          borderRadius: 12,
          border: "1px solid #fecaca",
          background: "#fef2f2",
          fontFamily: "sans-serif",
          color: "#991b1b",
          maxWidth: 560,
        }}
      >
        <strong style={{ fontSize: 16 }}>{cfg.title}</strong>
        <p style={{ margin: "8px 0 0", fontSize: 14, color: "#7f1d1d" }}>{cfg.message}</p>

        {cfg.fundsMessage && (
          <p style={{ margin: "8px 0 0", fontSize: 13, color: "#166534", background: "#f0fdf4", padding: "6px 10px", borderRadius: 6 }}>
            ✅ {cfg.fundsMessage}
          </p>
        )}

        {requestId && (
          <p style={{ margin: "8px 0 0", fontSize: 12, color: "#92400e" }}>
            Request ID: <code style={{ userSelect: "all" }}>{requestId}</code>
          </p>
        )}

        {transactionId && (
          <p style={{ margin: "8px 0 0", fontSize: 12 }}>
            Transaction ID: <code style={{ userSelect: "all" }}>{transactionId}</code>
            {statusPageUrl && (
              <> — <a href={`${statusPageUrl}?id=${transactionId}`} style={{ color: "#1d4ed8" }}>Check status</a></>
            )}
          </p>
        )}

        {error && (
          <details style={{ marginTop: 10 }}>
            <summary style={{ fontSize: 12, cursor: "pointer", color: "#b91c1c" }}>Error details</summary>
            <pre style={{ marginTop: 6, fontSize: 11, color: "#b91c1c", whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
              {error.message}
            </pre>
          </details>
        )}

        <div style={{ marginTop: 14, display: "flex", gap: 10 }}>
          <button
            onClick={this.handleRetry}
            data-testid="retry-button"
            style={{
              padding: "6px 16px",
              borderRadius: 6,
              border: "1px solid #fca5a5",
              background: "#fff",
              color: "#991b1b",
              cursor: "pointer",
              fontSize: 13,
            }}
          >
            Try again
          </button>
          <a
            href="mailto:support@anchorkit.dev"
            style={{ padding: "6px 16px", fontSize: 13, color: "#1d4ed8", lineHeight: "1.8" }}
          >
            Contact support
          </a>
        </div>
      </div>
    );
  }
}
