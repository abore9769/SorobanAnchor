import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom";
import { AnchorPlaygroundHeader } from "./AnchorPlaygroundHeader";

describe("AnchorPlaygroundHeader", () => {
  const defaults = {
    domain: "testanchor.stellar.org",
    onDomainChange: jest.fn(),
    dark: true,
    onToggleDark: jest.fn(),
  };

  it("renders the header with domain input", () => {
    render(<AnchorPlaygroundHeader {...defaults} />);
    expect(screen.getByTestId("playground-header")).toBeInTheDocument();
    expect(screen.getByTestId("domain-input")).toHaveValue("testanchor.stellar.org");
  });

  it("calls onDomainChange when domain input changes", () => {
    render(<AnchorPlaygroundHeader {...defaults} />);
    fireEvent.change(screen.getByTestId("domain-input"), { target: { value: "custom.anchor.com" } });
    expect(defaults.onDomainChange).toHaveBeenCalledWith("custom.anchor.com");
  });

  it("calls onToggleDark when theme button is clicked", () => {
    render(<AnchorPlaygroundHeader {...defaults} />);
    fireEvent.click(screen.getByTestId("theme-toggle"));
    expect(defaults.onToggleDark).toHaveBeenCalled();
  });

  it("shows sun icon in dark mode", () => {
    render(<AnchorPlaygroundHeader {...defaults} dark={true} />);
    expect(screen.getByTestId("theme-toggle")).toHaveTextContent("☀");
  });

  it("shows moon icon in light mode", () => {
    render(<AnchorPlaygroundHeader {...defaults} dark={false} />);
    expect(screen.getByTestId("theme-toggle")).toHaveTextContent("☾");
  });
});
