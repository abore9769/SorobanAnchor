import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom";
import { AnchorPlaygroundResponseViewer } from "./AnchorPlaygroundResponseViewer";
import type { HistoryEntry, ResponseState } from "../hooks/useAnchorPlayground";

const mockResponse: ResponseState = { data: { token: "abc" }, status: 200, time: 42 };
const mockHistory: HistoryEntry[] = [
  { id: "h1", time: "12:00:00", sep: "SEP-1", endpoint: "/.well-known/stellar.toml", method: "GET", status: 200, success: true },
];

describe("AnchorPlaygroundResponseViewer", () => {
  const onCopy = jest.fn();
  const defaults = { response: null, error: null, history: [], copied: false, onCopy };

  beforeEach(() => jest.clearAllMocks());

  it("shows empty state when no response", () => {
    render(<AnchorPlaygroundResponseViewer {...defaults} />);
    expect(screen.getByTestId("response-empty")).toBeInTheDocument();
  });

  it("shows response body when response is present", () => {
    render(<AnchorPlaygroundResponseViewer {...defaults} response={mockResponse} />);
    expect(screen.getByTestId("response-body")).toHaveTextContent("abc");
    expect(screen.getByTestId("response-status")).toHaveTextContent("200");
    expect(screen.getByTestId("response-status")).toHaveTextContent("42ms");
  });

  it("shows error alert when error is set", () => {
    render(<AnchorPlaygroundResponseViewer {...defaults} response={mockResponse} error="Network error" />);
    expect(screen.getByTestId("response-error")).toHaveTextContent("Network error");
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("calls onCopy when copy button is clicked", () => {
    render(<AnchorPlaygroundResponseViewer {...defaults} response={mockResponse} />);
    fireEvent.click(screen.getByTestId("copy-response"));
    expect(onCopy).toHaveBeenCalled();
  });

  it("shows COPIED text when copied is true", () => {
    render(<AnchorPlaygroundResponseViewer {...defaults} response={mockResponse} copied={true} />);
    expect(screen.getByTestId("copy-response")).toHaveTextContent("✓ COPIED");
  });

  it("shows history entries when history tab is selected", () => {
    render(<AnchorPlaygroundResponseViewer {...defaults} history={mockHistory} />);
    fireEvent.click(screen.getByTestId("tab-history"));
    expect(screen.getByTestId("history-list")).toBeInTheDocument();
    expect(screen.getByTestId("history-entry-h1")).toBeInTheDocument();
  });

  it("shows empty history message when no history", () => {
    render(<AnchorPlaygroundResponseViewer {...defaults} />);
    fireEvent.click(screen.getByTestId("tab-history"));
    expect(screen.getByText("No requests yet")).toBeInTheDocument();
  });
});
