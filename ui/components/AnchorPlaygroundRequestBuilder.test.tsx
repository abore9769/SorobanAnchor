import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom";
import { AnchorPlaygroundRequestBuilder } from "./AnchorPlaygroundRequestBuilder";
import { SEP_PROTOCOLS } from "../hooks/useAnchorPlayground";

const sep6Deposit = SEP_PROTOCOLS.find(s => s.id === "sep6")!.endpoints.find(e => e.id === "sep6-deposit")!;
const sep1Toml = SEP_PROTOCOLS[0].endpoints[0];

describe("AnchorPlaygroundRequestBuilder", () => {
  const onParamChange = jest.fn();
  const onJwtChange = jest.fn();
  const onSend = jest.fn();

  const defaults = {
    endpoint: sep1Toml,
    params: {},
    jwt: "",
    url: "https://testanchor.stellar.org/.well-known/stellar.toml",
    loading: false,
    onParamChange,
    onJwtChange,
    onSend,
  };

  beforeEach(() => jest.clearAllMocks());

  it("renders the URL preview", () => {
    render(<AnchorPlaygroundRequestBuilder {...defaults} />);
    expect(screen.getByTestId("url-preview")).toHaveTextContent("/.well-known/stellar.toml");
  });

  it("renders param inputs for endpoints with params", () => {
    render(<AnchorPlaygroundRequestBuilder {...defaults} endpoint={sep6Deposit} />);
    expect(screen.getByTestId("param-asset_code")).toBeInTheDocument();
    expect(screen.getByTestId("param-account")).toBeInTheDocument();
  });

  it("calls onParamChange when a param input changes", () => {
    render(<AnchorPlaygroundRequestBuilder {...defaults} endpoint={sep6Deposit} />);
    fireEvent.change(screen.getByTestId("param-asset_code"), { target: { value: "USDC" } });
    expect(onParamChange).toHaveBeenCalledWith("asset_code", "USDC");
  });

  it("calls onJwtChange when JWT input changes", () => {
    render(<AnchorPlaygroundRequestBuilder {...defaults} />);
    fireEvent.change(screen.getByTestId("jwt-input"), { target: { value: "my.jwt.token" } });
    expect(onJwtChange).toHaveBeenCalledWith("my.jwt.token");
  });

  it("calls onSend when send button is clicked", () => {
    render(<AnchorPlaygroundRequestBuilder {...defaults} />);
    fireEvent.click(screen.getByTestId("send-button"));
    expect(onSend).toHaveBeenCalled();
  });

  it("disables send button while loading", () => {
    render(<AnchorPlaygroundRequestBuilder {...defaults} loading={true} />);
    expect(screen.getByTestId("send-button")).toBeDisabled();
    expect(screen.getByTestId("send-button")).toHaveTextContent("SENDING…");
  });
});
