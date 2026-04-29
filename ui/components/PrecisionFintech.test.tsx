import React from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { axe, toHaveNoViolations } from "jest-axe";
import PrecisionFintech, { PrecisionFintechMetric, PrecisionFintechPosition } from "./PrecisionFintech";

expect.extend(toHaveNoViolations);

describe("PrecisionFintech", () => {
  it("renders primary dashboard content with default data", () => {
    render(<PrecisionFintech />);

    expect(screen.getByLabelText("portfolio-headline-value")).toHaveTextContent("$2,847,391");
    expect(screen.getByText("PORTFOLIO OVERVIEW — FEB 24, 2026")).toBeInTheDocument();
    expect(screen.getByText("Positions")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Deploy Capital/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Buy NVDA" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sell NVDA" })).toBeInTheDocument();
  });

  it("supports tab switching for positions section", () => {
    render(<PrecisionFintech initialTab="overview" />);

    const overviewTab = screen.getByRole("button", { name: "overview" });
    const historyTab = screen.getByRole("button", { name: "history" });

    expect(overviewTab.className).toContain("active");
    expect(historyTab.className).not.toContain("active");

    fireEvent.click(historyTab);

    expect(historyTab.className).toContain("active");
    expect(overviewTab.className).not.toContain("active");
  });

  it("renders empty-state row when no positions are provided", () => {
    const metrics: PrecisionFintechMetric[] = [
      { label: "Portfolio Value", value: "$1,000", change: "+0.1%", up: true },
    ];

    render(
      <PrecisionFintech
        metrics={metrics}
        positions={[]}
        chartData={[1, 2, 3]}
        headlineValue="$1,000"
        headlineLabel="Demo AUM"
      />
    );

    expect(screen.getByLabelText("portfolio-headline-value")).toHaveTextContent("$1,000");
    expect(screen.getByText("Demo AUM")).toBeInTheDocument();
    expect(screen.getByText("NO ACTIVE POSITIONS")).toBeInTheDocument();
  });

  it("passes accessibility checks", async () => {
    const positions: PrecisionFintechPosition[] = [
      {
        ticker: "USDC",
        name: "USD Coin",
        qty: 1000,
        price: 1,
        change: 0,
        value: 1000,
      },
    ];

    const { container } = render(
      <PrecisionFintech
        positions={positions}
        chartData={[10, 20, 30, 40]}
      />
    );

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});
