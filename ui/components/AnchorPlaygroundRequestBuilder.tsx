import React from "react";
import type { Endpoint } from "../hooks/useAnchorPlayground";

interface Props {
  endpoint: Endpoint;
  params: Record<string, string>;
  jwt: string;
  url: string;
  loading: boolean;
  onParamChange: (key: string, value: string) => void;
  onJwtChange: (v: string) => void;
  onSend: () => void;
}

export function AnchorPlaygroundRequestBuilder({
  endpoint, params, jwt, url, loading, onParamChange, onJwtChange, onSend,
}: Props) {
  return (
    <div data-testid="request-builder" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      {/* URL preview */}
      <div
        data-testid="url-preview"
        style={{
          fontFamily: "monospace",
          fontSize: 10,
          padding: "8px 12px",
          borderRadius: 6,
          background: "rgba(0,0,0,0.4)",
          border: "1px solid #1e2d45",
          color: "#79d4fd",
          wordBreak: "break-all",
        }}
      >
        <span style={{ color: "#3a5070", marginRight: 6 }}>{endpoint.method}</span>
        {url}
      </div>

      {/* Params */}
      {endpoint.params.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {endpoint.params.map(p => (
            <div key={p.key} style={{ display: "flex", flexDirection: "column", gap: 3 }}>
              <label style={{ fontSize: 9, color: "#3a5070", letterSpacing: "0.1em" }}>
                {p.label.toUpperCase()}{p.required && <span style={{ color: "#ff5050" }}> *</span>}
              </label>
              <input
                data-testid={`param-${p.key}`}
                value={params[p.key] ?? ""}
                onChange={e => onParamChange(p.key, e.target.value)}
                placeholder={p.placeholder}
                style={{
                  background: "rgba(0,0,0,0.3)",
                  border: "1px solid #1e2d45",
                  borderRadius: 5,
                  padding: "7px 10px",
                  color: "#c8d8ee",
                  fontSize: 11,
                  fontFamily: "monospace",
                  outline: "none",
                }}
              />
            </div>
          ))}
        </div>
      )}

      {/* JWT */}
      <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
        <label style={{ fontSize: 9, color: "#3a5070", letterSpacing: "0.1em" }}>
          AUTHORIZATION (JWT)
        </label>
        <input
          data-testid="jwt-input"
          value={jwt}
          onChange={e => onJwtChange(e.target.value)}
          placeholder="Bearer token (optional)"
          style={{
            background: "rgba(0,0,0,0.3)",
            border: "1px solid #1e2d45",
            borderRadius: 5,
            padding: "7px 10px",
            color: "#c8d8ee",
            fontSize: 11,
            fontFamily: "monospace",
            outline: "none",
          }}
        />
      </div>

      {/* Send button */}
      <button
        data-testid="send-button"
        onClick={onSend}
        disabled={loading}
        style={{
          padding: "10px 0",
          borderRadius: 6,
          fontSize: 11,
          fontWeight: 700,
          letterSpacing: "0.12em",
          cursor: loading ? "not-allowed" : "pointer",
          fontFamily: "inherit",
          border: "1px solid #00e5ff",
          background: loading ? "transparent" : "rgba(0,229,255,0.1)",
          color: loading ? "#1e2d45" : "#00e5ff",
          transition: "all 0.2s",
        }}
      >
        {loading ? "SENDING…" : "⚡ SEND REQUEST"}
      </button>
    </div>
  );
}
