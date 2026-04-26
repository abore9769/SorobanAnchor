import React from "react";

interface Props {
  children: React.ReactNode;
  fallback?: React.ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class AnchorErrorBoundary extends React.Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("[AnchorErrorBoundary]", error, info);
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback;
      return (
        <div
          role="alert"
          style={{
            padding: "20px 24px",
            borderRadius: 12,
            border: "1px solid #fecaca",
            background: "#fef2f2",
            fontFamily: "sans-serif",
            color: "#991b1b",
          }}
        >
          <strong>Something went wrong rendering this component.</strong>
          {this.state.error && (
            <pre style={{ marginTop: 8, fontSize: 12, color: "#b91c1c" }}>
              {this.state.error.message}
            </pre>
          )}
        </div>
      );
    }
    return this.props.children;
  }
}
