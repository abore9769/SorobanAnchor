import React from "react";
import type { SEPProtocol, Endpoint } from "../hooks/useAnchorPlayground";

const SEP_HEX: Record<string, string> = {
  cyan: "#00e5ff", blue: "#4d8dff", green: "#00ff9d",
  rose: "#ff3670", violet: "#c44dff", orange: "#ff8c00", teal: "#00ffcc",
};

const METHOD_COLOR: Record<string, string> = {
  GET: "#00ff9d", POST: "#ffcb6b", PUT: "#c44dff",
};

interface Props {
  protocols: SEPProtocol[];
  activeSEP: SEPProtocol;
  activeEp: Endpoint;
  onSelectSEP: (sep: SEPProtocol) => void;
  onSelectEp: (ep: Endpoint) => void;
}

export function AnchorPlaygroundOperationSelector({
  protocols, activeSEP, activeEp, onSelectSEP, onSelectEp,
}: Props) {
  return (
    <div data-testid="operation-selector" style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {/* SEP tabs */}
      <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
        {protocols.map(sep => {
          const color = SEP_HEX[sep.color] ?? "#00e5ff";
          const active = sep.id === activeSEP.id;
          return (
            <button
              key={sep.id}
              data-testid={`sep-tab-${sep.id}`}
              onClick={() => onSelectSEP(sep)}
              style={{
                padding: "5px 12px",
                borderRadius: 5,
                fontSize: 10,
                fontWeight: 700,
                letterSpacing: "0.1em",
                cursor: "pointer",
                fontFamily: "inherit",
                border: `1px solid ${active ? color : "#1e2d45"}`,
                background: active ? `${color}18` : "transparent",
                color: active ? color : "#3a5070",
                transition: "all 0.2s",
              }}
            >
              {sep.name}
            </button>
          );
        })}
      </div>

      {/* Endpoint list */}
      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        {activeSEP.endpoints.map(ep => {
          const active = ep.id === activeEp.id;
          const mc = METHOD_COLOR[ep.method] ?? "#aaa";
          return (
            <button
              key={ep.id}
              data-testid={`endpoint-${ep.id}`}
              onClick={() => onSelectEp(ep)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "8px 12px",
                borderRadius: 6,
                fontSize: 11,
                cursor: "pointer",
                fontFamily: "inherit",
                textAlign: "left",
                border: `1px solid ${active ? "#1e2d45" : "transparent"}`,
                background: active ? "rgba(0,0,0,0.3)" : "transparent",
                color: active ? "#c8d8ee" : "#3a5070",
                transition: "all 0.15s",
              }}
            >
              <span style={{ fontSize: 9, fontWeight: 700, color: mc, minWidth: 32 }}>
                {ep.method}
              </span>
              <span style={{ fontFamily: "monospace", fontSize: 10 }}>{ep.path}</span>
              <span style={{ fontSize: 9, color: "#2a3d5a", marginLeft: "auto" }}>{ep.description}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
