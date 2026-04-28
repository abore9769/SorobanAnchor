import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import { AnchorErrorBoundary } from "./AnchorErrorBoundary";

// Helper: a component that throws on first render then succeeds
function ThrowOnce({ message }: { message: string }) {
  const ref = React.useRef(false);
  if (!ref.current) {
    ref.current = true;
    throw new Error(message);
  }
  return <div>recovered</div>;
}

// Helper: always throws
function AlwaysThrow({ message }: { message: string }) {
  throw new Error(message);
}

// Suppress React's console.error noise in tests
beforeEach(() => {
  jest.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(() => {
  (console.error as jest.Mock).mockRestore();
});

describe("AnchorErrorBoundary", () => {
  it("renders children when no error", () => {
    render(
      <AnchorErrorBoundary>
        <div>hello</div>
      </AnchorErrorBoundary>
    );
    expect(screen.getByText("hello")).toBeInTheDocument();
  });

  it("shows network error category", () => {
    render(
      <AnchorErrorBoundary>
        <AlwaysThrow message="network timeout" />
      </AnchorErrorBoundary>
    );
    expect(screen.getByTestId("anchor-error-boundary")).toHaveAttribute("data-error-category", "network");
    expect(screen.getByText("Network Error")).toBeInTheDocument();
    expect(screen.getByText(/funds are safe/i)).toBeInTheDocument();
  });

  it("shows authentication error category", () => {
    render(
      <AnchorErrorBoundary>
        <AlwaysThrow message="JWT token expired" />
      </AnchorErrorBoundary>
    );
    expect(screen.getByTestId("anchor-error-boundary")).toHaveAttribute("data-error-category", "authentication");
    expect(screen.getByText("Authentication Error")).toBeInTheDocument();
  });

  it("shows contract error category", () => {
    render(
      <AnchorErrorBoundary>
        <AlwaysThrow message="soroban contract invocation failed" />
      </AnchorErrorBoundary>
    );
    expect(screen.getByTestId("anchor-error-boundary")).toHaveAttribute("data-error-category", "contract");
    expect(screen.getByText("Contract Error")).toBeInTheDocument();
  });

  it("shows validation error category", () => {
    render(
      <AnchorErrorBoundary>
        <AlwaysThrow message="invalid schema: missing required field" />
      </AnchorErrorBoundary>
    );
    expect(screen.getByTestId("anchor-error-boundary")).toHaveAttribute("data-error-category", "validation");
    expect(screen.getByText("Validation Error")).toBeInTheDocument();
  });

  it("shows unknown error category for unrecognised errors", () => {
    render(
      <AnchorErrorBoundary>
        <AlwaysThrow message="something completely unexpected" />
      </AnchorErrorBoundary>
    );
    expect(screen.getByTestId("anchor-error-boundary")).toHaveAttribute("data-error-category", "unknown");
    expect(screen.getByText("Unexpected Error")).toBeInTheDocument();
  });

  it("retry button re-mounts children", () => {
    render(
      <AnchorErrorBoundary>
        <ThrowOnce message="transient error" />
      </AnchorErrorBoundary>
    );
    expect(screen.getByText("Unexpected Error")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("retry-button"));
    expect(screen.getByText("recovered")).toBeInTheDocument();
  });

  it("displays request ID when present in error message", () => {
    render(
      <AnchorErrorBoundary>
        <AlwaysThrow message="fetch failed requestId: req-abc-123" />
      </AnchorErrorBoundary>
    );
    expect(screen.getByText("req-abc-123")).toBeInTheDocument();
  });

  it("shows transaction status link when transactionId and statusPageUrl provided", () => {
    render(
      <AnchorErrorBoundary transactionId="txn-001" statusPageUrl="https://anchor.example.com/status">
        <AlwaysThrow message="contract error" />
      </AnchorErrorBoundary>
    );
    expect(screen.getByText("txn-001")).toBeInTheDocument();
    const link = screen.getByRole("link", { name: /check status/i });
    expect(link).toHaveAttribute("href", "https://anchor.example.com/status?id=txn-001");
  });

  it("logs error to reporting endpoint", () => {
    const fetchSpy = jest.spyOn(global, "fetch").mockResolvedValue(new Response());
    render(
      <AnchorErrorBoundary reportingEndpoint="https://errors.example.com/report">
        <AlwaysThrow message="network failure" />
      </AnchorErrorBoundary>
    );
    expect(fetchSpy).toHaveBeenCalledWith(
      "https://errors.example.com/report",
      expect.objectContaining({ method: "POST" })
    );
    fetchSpy.mockRestore();
  });

  it("renders custom fallback when provided", () => {
    render(
      <AnchorErrorBoundary fallback={<div>custom fallback</div>}>
        <AlwaysThrow message="error" />
      </AnchorErrorBoundary>
    );
    expect(screen.getByText("custom fallback")).toBeInTheDocument();
  });
});
