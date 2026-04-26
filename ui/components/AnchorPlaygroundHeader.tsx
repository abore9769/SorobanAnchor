import React from "react";

interface Props {
  domain: string;
  onDomainChange: (v: string) => void;
  dark: boolean;
  onToggleDark: () => void;
}

export function AnchorPlaygroundHeader({ domain, onDomainChange, dark, onToggleDark }: Props) {
  return (
    <div data-testid="playground-header" style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 20 }}>
      <div>
        <div style={{ fontSize: 10, letterSpacing: "0.2em", color: "#3a5070", marginBottom: 4 }}>
          ◈ ANCHOR PLAYGROUND
        </div>
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 700, color: dark ? "#dde6f5" : "#1a2a3a" }}>
          SEP Explorer
        </h1>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ fontSize: 10, color: "#3a5070" }}>https://</span>
        <input
          data-testid="domain-input"
          value={domain}
          onChange={e => onDomainChange(e.target.value)}
          style={{
            background: "transparent",
            border: "1px solid #1e2d45",
            borderRadius: 5,
            padding: "5px 10px",
            color: dark ? "#00e5ff" : "#0050bb",
            fontSize: 11,
            fontFamily: "inherit",
            outline: "none",
            width: 200,
          }}
          placeholder="anchor.example.com"
        />
        <button
          data-testid="theme-toggle"
          onClick={onToggleDark}
          style={{
            background: "transparent",
            border: "1px solid #1e2d45",
            borderRadius: 5,
            padding: "5px 8px",
            cursor: "pointer",
            color: dark ? "#3a5070" : "#555",
            fontSize: 12,
          }}
          aria-label={dark ? "Switch to light mode" : "Switch to dark mode"}
        >
          {dark ? "☀" : "☾"}
        </button>
      </div>
    </div>
  );
}
