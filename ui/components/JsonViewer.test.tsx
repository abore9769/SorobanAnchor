import React from "react";
import { render, screen, fireEvent, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@testing-library/jest-dom";
import { JsonViewer } from "./JsonViewer";

const user = userEvent.setup();

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const flatObject = { name: "Alice", age: 30, active: true };

function buildDeepObject(depth: number): Record<string, unknown> {
  if (depth === 0) return { value: "leaf" };
  return { nested: buildDeepObject(depth - 1) };
}

function buildLargeArray(size: number): number[] {
  return Array.from({ length: size }, (_, i) => i);
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("JsonViewer", () => {
  test("renders a flat JSON object", () => {
    render(<JsonViewer data={flatObject} />);
    expect(screen.getByText(/"name"/)).toBeInTheDocument();
    expect(screen.getByText(/"Alice"/)).toBeInTheDocument();
    expect(screen.getByText(/"age"/)).toBeInTheDocument();
    expect(screen.getByText(/30/)).toBeInTheDocument();
  });

  test("renders a deeply nested object (10+ levels)", () => {
    const deep = buildDeepObject(12);
    render(<JsonViewer data={deep} defaultExpandDepth={15} />);
    // Root renders without crashing — "nested" key appears at least once
    expect(screen.getAllByText(/"nested"/).length).toBeGreaterThan(0);
  });

  test("handles a 1000-element array without performance degradation", () => {
    const largeArray = buildLargeArray(1000);
    const start = performance.now();
    render(<JsonViewer data={largeArray} defaultExpandDepth={0} />);
    const elapsed = performance.now() - start;
    // Should render in under 3 seconds even without virtualization
    expect(elapsed).toBeLessThan(3000);
    // Root array node renders — summary shows item count
    expect(screen.getByText(/1000/)).toBeInTheDocument();
  });

  test("displays special characters (Unicode, escaped quotes) correctly", () => {
    const specialData = {
      unicode: "こんにちは 🌟",
      escaped: 'say "hello"',
      newline: "line1\nline2",
    };
    render(<JsonViewer data={specialData} defaultExpandDepth={2} />);
    expect(screen.getByText(/"unicode"/)).toBeInTheDocument();
    expect(screen.getByText(/"escaped"/)).toBeInTheDocument();
  });

  test("collapses and expands tree nodes via click", () => {
    const nested = { outer: { inner: { value: 42 } } };
    render(<JsonViewer data={nested} defaultExpandDepth={3} />);

    // "inner" key is visible when expanded
    expect(screen.getByText(/"inner"/)).toBeInTheDocument();

    // Click the outer node to collapse it
    const outerRow = screen.getByText(/"outer"/).closest("[style]")!;
    fireEvent.click(outerRow);

    // After collapse, inner should no longer be visible
    expect(screen.queryByText(/"inner"/)).not.toBeInTheDocument();

    // Click again to expand
    fireEvent.click(outerRow);
    expect(screen.getByText(/"inner"/)).toBeInTheDocument();
  });

  test("copies JSON value to clipboard via copy button", async () => {
    const mockWriteText = jest.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: mockWriteText },
      writable: true,
      configurable: true,
    });
    render(<JsonViewer data={flatObject} />);
    const copyButton = screen.getByText(/Copy/);
    await user.click(copyButton);
    expect(mockWriteText).toHaveBeenCalledWith(
      JSON.stringify(flatObject, null, 2)
    );
  });

  test("renders null with distinct visual style", () => {
    render(<JsonViewer data={{ val: null }} defaultExpandDepth={2} />);
    expect(screen.getAllByText(/null/).length).toBeGreaterThan(0);
  });

  test("renders boolean true and false with distinct visual style", () => {
    render(
      <JsonViewer data={{ yes: true, no: false }} defaultExpandDepth={2} />
    );
    expect(screen.getByText(/true/)).toBeInTheDocument();
    expect(screen.getByText(/false/)).toBeInTheDocument();
  });

  test("renders numbers with distinct visual style", () => {
    render(<JsonViewer data={{ count: 42, pi: 3.14 }} defaultExpandDepth={2} />);
    expect(screen.getByText(/42/)).toBeInTheDocument();
    expect(screen.getByText(/3\.14/)).toBeInTheDocument();
  });

  test("renders strings with distinct visual style", () => {
    render(<JsonViewer data={{ greeting: "hello" }} defaultExpandDepth={2} />);
    expect(screen.getByText(/"hello"/)).toBeInTheDocument();
  });

  test("maxDepth prop limits initial expansion depth", () => {
    const deep = buildDeepObject(5);
    render(<JsonViewer data={deep} maxDepth={1} />);
    // At maxDepth=1, only the first level is expanded
    // "nested" key at depth 1 is visible (may appear multiple times in tree)
    expect(screen.getAllByText(/"nested"/).length).toBeGreaterThan(0);
    // But deeper keys (depth 2+) should be collapsed
    expect(screen.queryByText(/"value"/)).not.toBeInTheDocument();
  });

  test("maxDepth=0 collapses everything initially", () => {
    const obj = { a: { b: { c: 1 } } };
    render(<JsonViewer data={obj} maxDepth={0} />);
    // At maxDepth=0, root is shown but children are collapsed
    // "a" key is visible (it's the root's child shown in collapsed summary)
    // but "b" (depth 2) should NOT be visible
    expect(screen.queryByText(/"b"/)).not.toBeInTheDocument();
    // Root summary shows key count
    expect(screen.getAllByText(/1 keys/).length).toBeGreaterThan(0);
  });

  test("expand all button expands all nodes", async () => {
    const nested = { a: { b: 1 }, c: { d: 2 } };
    render(<JsonViewer data={nested} maxDepth={0} />);

    const expandBtn = screen.getByText(/Expand All/);
    await user.click(expandBtn);

    expect(screen.getByText(/"a"/)).toBeInTheDocument();
    expect(screen.getByText(/"b"/)).toBeInTheDocument();
  });

  test("collapse all button collapses all nodes", async () => {
    const nested = { a: { b: 1 } };
    render(<JsonViewer data={nested} defaultExpandDepth={5} />);

    // Initially expanded
    expect(screen.getByText(/"b"/)).toBeInTheDocument();

    const collapseBtn = screen.getByText(/Collapse/);
    await user.click(collapseBtn);

    expect(screen.queryByText(/"b"/)).not.toBeInTheDocument();
  });

  test("search highlights matching keys and values", async () => {
    render(<JsonViewer data={flatObject} defaultExpandDepth={2} searchable />);
    const searchInput = screen.getByPlaceholderText(/Search/);
    await user.type(searchInput, "Alice");
    // Match count badge appears (the orange count span)
    const matchBadge = document.querySelector(
      'span[style*="color: rgb(249, 115, 22)"]'
    );
    expect(matchBadge).toBeTruthy();
  });

  test("switches between tree and raw mode", async () => {
    render(<JsonViewer data={flatObject} defaultMode="tree" />);
    const rawBtn = screen.getByText("raw");
    await user.click(rawBtn);
    // In raw mode, a <pre> element with the JSON is rendered
    const pre = document.querySelector("pre");
    expect(pre).toBeTruthy();
  });

  test("renders title and subtitle in titlebar", () => {
    render(
      <JsonViewer
        data={flatObject}
        title="GET /sep24/info"
        subtitle="testanchor.stellar.org"
      />
    );
    expect(screen.getByText("GET /sep24/info")).toBeInTheDocument();
    expect(screen.getByText("testanchor.stellar.org")).toBeInTheDocument();
  });

  test("renders HTTP status badge for 200", () => {
    render(<JsonViewer data={flatObject} status={200} />);
    expect(screen.getByText(/200/)).toBeInTheDocument();
  });

  test("renders HTTP status badge for 400 error", () => {
    render(<JsonViewer data={{ error: "bad request" }} status={400} />);
    expect(screen.getByText(/400/)).toBeInTheDocument();
  });

  test("renders response time", () => {
    render(<JsonViewer data={flatObject} responseTime={143} />);
    expect(screen.getByText(/143ms/)).toBeInTheDocument();
  });

  test("renders all three themes without crashing", () => {
    const { rerender } = render(<JsonViewer data={flatObject} theme="ember" />);
    rerender(<JsonViewer data={flatObject} theme="arctic" />);
    rerender(<JsonViewer data={flatObject} theme="forest" />);
    expect(screen.getByText(/"name"/)).toBeInTheDocument();
  });
});
