import React from "react";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import "@testing-library/jest-dom";
import AnchorPlayground from "./AnchorPlayground";

const MOCK_RESPONSE = { assets: [{ code: "USDC" }] };

global.fetch = jest.fn() as jest.Mock;

beforeEach(() => {
  jest.clearAllMocks();
  (global.fetch as jest.Mock).mockResolvedValue({
    ok: true,
    status: 200,
    headers: { get: () => "application/json" },
    json: () => Promise.resolve(MOCK_RESPONSE),
    text: () => Promise.resolve(""),
  });
  Object.defineProperty(global, "performance", {
    value: { now: jest.fn(() => 0) },
    writable: true,
  });
  Object.defineProperty(global, "crypto", {
    value: { randomUUID: () => "test-uuid-1234" },
    writable: true,
  });
});

describe("AnchorPlayground", () => {
  it("renders without errors", () => {
    render(<AnchorPlayground />);
    expect(screen.getByTestId("anchor-playground")).toBeInTheDocument();
    expect(screen.getByTestId("playground-header")).toBeInTheDocument();
    expect(screen.getByTestId("operation-selector")).toBeInTheDocument();
    expect(screen.getByTestId("request-builder")).toBeInTheDocument();
    expect(screen.getByTestId("response-viewer")).toBeInTheDocument();
  });

  it("operation selection updates the request builder URL", () => {
    render(<AnchorPlayground />);

    // Switch to SEP-6
    fireEvent.click(screen.getByTestId("sep-tab-sep6"));

    // Select the deposit endpoint
    fireEvent.click(screen.getByTestId("endpoint-sep6-deposit"));

    const urlPreview = screen.getByTestId("url-preview");
    expect(urlPreview).toHaveTextContent("/transfer/deposit");
    expect(urlPreview).toHaveTextContent("GET");
  });

  it("submitting a request calls fetch with the correct endpoint URL", async () => {
    render(<AnchorPlayground />);

    // Default is SEP-1 stellar.toml GET
    await act(async () => {
      fireEvent.click(screen.getByTestId("send-button"));
    });

    await waitFor(() => {
      expect(global.fetch).toHaveBeenCalledWith(
        expect.stringContaining("/.well-known/stellar.toml"),
        expect.objectContaining({ method: "GET" })
      );
    });
  });

  it("response is displayed in the viewer after a successful request", async () => {
    render(<AnchorPlayground />);

    await act(async () => {
      fireEvent.click(screen.getByTestId("send-button"));
    });

    await waitFor(() => {
      expect(screen.getByTestId("response-status")).toBeInTheDocument();
      expect(screen.getByTestId("response-body")).toHaveTextContent("USDC");
    });
  });

  it("history is updated after each request", async () => {
    render(<AnchorPlayground />);

    await act(async () => {
      fireEvent.click(screen.getByTestId("send-button"));
    });

    await waitFor(() => {
      expect(screen.getByTestId("response-status")).toBeInTheDocument();
    });

    // Switch to history tab
    fireEvent.click(screen.getByTestId("tab-history"));

    expect(screen.getByTestId("history-list")).toBeInTheDocument();
    // One entry should be present
    const entries = screen.getAllByTestId(/^history-entry-/);
    expect(entries).toHaveLength(1);
  });
});
