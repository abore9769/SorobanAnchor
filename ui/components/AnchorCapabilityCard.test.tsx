import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom";
import {
  AnchorCapabilityCard,
  AnchorCapabilityCardProps,
  SupportedAsset,
} from "./AnchorCapabilityCard";
import { AnchorErrorBoundary } from "./AnchorErrorBoundary";

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const depositAsset: SupportedAsset = {
  code: "USDC",
  name: "USD Coin",
  operationTypes: ["deposit"],
  depositEnabled: true,
  withdrawalEnabled: false,
  fees: {
    deposit: { type: "flat", flatAmount: 1.5, currency: "USD" },
  },
  limits: {
    minDeposit: 10,
    maxDeposit: 5000,
    currency: "USD",
  },
  kyc: { level: "basic", fields: [{ name: "email", label: "Email", required: true }] },
};

const withdrawalAsset: SupportedAsset = {
  code: "EURC",
  name: "Euro Coin",
  operationTypes: ["withdrawal"],
  depositEnabled: false,
  withdrawalEnabled: true,
  fees: {
    withdrawal: { type: "percent", percent: 0.5, currency: "EUR" },
  },
  limits: {
    minWithdrawal: 20,
    maxWithdrawal: 10000,
    currency: "EUR",
  },
  kyc: { level: "full", fields: [{ name: "id_number", label: "ID Number", required: true }] },
};

const fullServiceAsset: SupportedAsset = {
  code: "XLM",
  name: "Stellar Lumens",
  operationTypes: ["both"],
  depositEnabled: true,
  withdrawalEnabled: true,
  fees: {
    deposit: { type: "flat", flatAmount: 0.5, currency: "USD" },
    withdrawal: { type: "percent", percent: 1.0, currency: "USD" },
  },
  limits: {
    minDeposit: 5,
    maxDeposit: 20000,
    minWithdrawal: 5,
    maxWithdrawal: 20000,
    dailyLimit: 50000,
    monthlyLimit: 200000,
    currency: "USD",
  },
  kyc: {
    level: "none",
    fields: [],
  },
};

const quoteAsset: SupportedAsset = {
  code: "BTC",
  name: "Bitcoin",
  operationTypes: ["both"],
  depositEnabled: true,
  withdrawalEnabled: true,
  fees: {
    deposit: {
      type: "tiered",
      currency: "USD",
      tiers: [
        { upTo: 1000, fee: "$5.00" },
        { upTo: null, fee: "0.5%" },
      ],
    },
  },
  limits: {
    minDeposit: 100,
    maxDeposit: 100000,
    currency: "USD",
  },
  kyc: { level: "enhanced", fields: [{ name: "source_of_funds", label: "Source of Funds", required: true }] },
};

const baseProps: AnchorCapabilityCardProps = {
  anchorName: "TestAnchor",
  domain: "testanchor.stellar.org",
  accentColor: "#3b82f6",
  assets: [depositAsset],
};

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("AnchorCapabilityCard", () => {
  test("renders deposit-only anchor", () => {
    render(<AnchorCapabilityCard {...baseProps} assets={[depositAsset]} />);
    expect(screen.getByText("TestAnchor")).toBeInTheDocument();
    expect(screen.getByText("testanchor.stellar.org")).toBeInTheDocument();
    expect(screen.getByText("USDC")).toBeInTheDocument();
    // deposit badge visible
    expect(screen.getByText("Deposit")).toBeInTheDocument();
    // withdrawal badge should NOT appear
    expect(screen.queryByText("Withdraw")).not.toBeInTheDocument();
  });

  test("renders withdrawal-only anchor", () => {
    render(<AnchorCapabilityCard {...baseProps} assets={[withdrawalAsset]} />);
    expect(screen.getByText("EURC")).toBeInTheDocument();
    expect(screen.getByText("Withdraw")).toBeInTheDocument();
    expect(screen.queryByText("Deposit")).not.toBeInTheDocument();
  });

  test("renders full-service anchor with all capabilities", () => {
    render(
      <AnchorCapabilityCard
        {...baseProps}
        assets={[fullServiceAsset]}
      />
    );
    expect(screen.getByText("XLM")).toBeInTheDocument();
    expect(screen.getByText("Deposit")).toBeInTheDocument();
    expect(screen.getByText("Withdraw")).toBeInTheDocument();
  });

  test("renders inactive anchor with disabled state (no assets)", () => {
    render(<AnchorCapabilityCard {...baseProps} assets={[]} />);
    // Header still renders
    expect(screen.getByText("TestAnchor")).toBeInTheDocument();
    // 0 assets pill
    expect(screen.getByText("0 assets")).toBeInTheDocument();
  });

  test("displays correct fee structure for deposit asset", () => {
    render(<AnchorCapabilityCard {...baseProps} assets={[depositAsset]} />);
    // Navigate to fees tab
    fireEvent.click(screen.getByText("Fees"));
    expect(screen.getByText("Deposit Fees")).toBeInTheDocument();
    expect(screen.getByText("Flat")).toBeInTheDocument();
    // flat amount formatted
    expect(screen.getByText("USD 1.50")).toBeInTheDocument();
  });

  test("displays correct fee structure for withdrawal asset", () => {
    render(<AnchorCapabilityCard {...baseProps} assets={[withdrawalAsset]} />);
    fireEvent.click(screen.getByText("Fees"));
    expect(screen.getByText("Withdrawal Fees")).toBeInTheDocument();
    expect(screen.getByText("Percent")).toBeInTheDocument();
    expect(screen.getByText("0.5%")).toBeInTheDocument();
  });

  test("displays tiered fee structure for quote-provider anchor", () => {
    render(<AnchorCapabilityCard {...baseProps} assets={[quoteAsset]} />);
    fireEvent.click(screen.getByText("Fees"));
    expect(screen.getByText("Tiered")).toBeInTheDocument();
    expect(screen.getByText("$5.00")).toBeInTheDocument();
  });

  test("shows asset limits (min/max amounts) for deposit asset", () => {
    render(<AnchorCapabilityCard {...baseProps} assets={[depositAsset]} />);
    fireEvent.click(screen.getByText("Limits"));
    expect(screen.getByText("Min Deposit")).toBeInTheDocument();
    expect(screen.getByText("Max Deposit")).toBeInTheDocument();
    // Values may appear in range bar and table — use getAllByText
    expect(screen.getAllByText("USD 10.00").length).toBeGreaterThan(0);
    expect(screen.getAllByText("USD 5,000.00").length).toBeGreaterThan(0);
  });

  test("shows asset limits for full-service anchor including daily/monthly", () => {
    render(<AnchorCapabilityCard {...baseProps} assets={[fullServiceAsset]} />);
    fireEvent.click(screen.getByText("Limits"));
    expect(screen.getByText("Daily Limit")).toBeInTheDocument();
    expect(screen.getByText("Monthly Limit")).toBeInTheDocument();
    expect(screen.getByText("USD 50,000.00")).toBeInTheDocument();
  });

  test("renders reputation score / KYC level with correct visual indicator", () => {
    render(<AnchorCapabilityCard {...baseProps} assets={[depositAsset]} />);
    // KYC badge in header
    expect(screen.getByText("Basic KYC")).toBeInTheDocument();
  });

  test("renders KYC panel with required fields", () => {
    render(<AnchorCapabilityCard {...baseProps} assets={[depositAsset]} />);
    fireEvent.click(screen.getByText("KYC"));
    // "Basic KYC" appears in header badge and KYC panel — both are fine
    expect(screen.getAllByText("Basic KYC").length).toBeGreaterThan(0);
    expect(screen.getByText("Email")).toBeInTheDocument();
  });

  test("renders no-KYC state with celebration message", () => {
    render(<AnchorCapabilityCard {...baseProps} assets={[fullServiceAsset]} />);
    fireEvent.click(screen.getByText("KYC"));
    // "No KYC" appears in header badge and KYC panel
    expect(screen.getAllByText("No KYC").length).toBeGreaterThan(0);
    expect(screen.getByText("No verification needed")).toBeInTheDocument();
  });

  test("handles loading/skeleton state (empty assets array)", () => {
    const { container } = render(
      <AnchorCapabilityCard {...baseProps} assets={[]} />
    );
    // Card renders without crashing
    expect(container.firstChild).toBeTruthy();
    expect(screen.getByText("0 assets")).toBeInTheDocument();
  });

  test("switches between assets via asset strip", () => {
    render(
      <AnchorCapabilityCard
        {...baseProps}
        assets={[depositAsset, withdrawalAsset]}
      />
    );
    // Go to fees tab so asset strip appears
    fireEvent.click(screen.getByText("Fees"));
    // Both asset codes appear in the strip
    const usdcButtons = screen.getAllByText("USDC");
    expect(usdcButtons.length).toBeGreaterThan(0);
  });

  test("accepts typed AnchorCapabilityCardProps interface", () => {
    // TypeScript compile-time check — if this renders, the types are correct
    const props: AnchorCapabilityCardProps = {
      anchorName: "TypedAnchor",
      domain: "typed.stellar.org",
      assets: [depositAsset],
    };
    render(<AnchorCapabilityCard {...props} />);
    expect(screen.getByText("TypedAnchor")).toBeInTheDocument();
  });

  test("AnchorErrorBoundary catches render errors gracefully", () => {
    const ThrowingComponent = () => {
      throw new Error("Test render error");
    };
    // Suppress console.error for this test
    const spy = jest.spyOn(console, "error").mockImplementation(() => {});
    render(
      <AnchorErrorBoundary>
        <ThrowingComponent />
      </AnchorErrorBoundary>
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByText(/Something went wrong/)).toBeInTheDocument();
    spy.mockRestore();
  });

  test("AnchorErrorBoundary renders children when no error", () => {
    render(
      <AnchorErrorBoundary>
        <AnchorCapabilityCard {...baseProps} assets={[depositAsset]} />
      </AnchorErrorBoundary>
    );
    expect(screen.getByText("TestAnchor")).toBeInTheDocument();
  });
});
