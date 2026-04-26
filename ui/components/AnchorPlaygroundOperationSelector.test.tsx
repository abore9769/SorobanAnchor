import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom";
import { AnchorPlaygroundOperationSelector } from "./AnchorPlaygroundOperationSelector";
import { SEP_PROTOCOLS } from "../hooks/useAnchorPlayground";

describe("AnchorPlaygroundOperationSelector", () => {
  const onSelectSEP = jest.fn();
  const onSelectEp = jest.fn();

  const defaults = {
    protocols: SEP_PROTOCOLS,
    activeSEP: SEP_PROTOCOLS[0],
    activeEp: SEP_PROTOCOLS[0].endpoints[0],
    onSelectSEP,
    onSelectEp,
  };

  beforeEach(() => jest.clearAllMocks());

  it("renders all SEP protocol tabs", () => {
    render(<AnchorPlaygroundOperationSelector {...defaults} />);
    SEP_PROTOCOLS.forEach(sep => {
      expect(screen.getByTestId(`sep-tab-${sep.id}`)).toBeInTheDocument();
    });
  });

  it("renders endpoints for the active SEP", () => {
    render(<AnchorPlaygroundOperationSelector {...defaults} />);
    SEP_PROTOCOLS[0].endpoints.forEach(ep => {
      expect(screen.getByTestId(`endpoint-${ep.id}`)).toBeInTheDocument();
    });
  });

  it("calls onSelectSEP when a SEP tab is clicked", () => {
    render(<AnchorPlaygroundOperationSelector {...defaults} />);
    fireEvent.click(screen.getByTestId("sep-tab-sep6"));
    expect(onSelectSEP).toHaveBeenCalledWith(SEP_PROTOCOLS.find(s => s.id === "sep6"));
  });

  it("calls onSelectEp when an endpoint is clicked", () => {
    render(<AnchorPlaygroundOperationSelector {...defaults} />);
    const firstEp = SEP_PROTOCOLS[0].endpoints[0];
    fireEvent.click(screen.getByTestId(`endpoint-${firstEp.id}`));
    expect(onSelectEp).toHaveBeenCalledWith(firstEp);
  });
});
