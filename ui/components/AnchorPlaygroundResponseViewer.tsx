import React from "react";
import type { ResponseState, HistoryEntry } from "../hooks/useAnchorPlayground";

interface Props {
  response: ResponseState | null;
  error: string | null;
  history: HistoryEntry[];
  copied: boolean;
  onCopy: () => void;
}

export function AnchorPlaygroundResponseViewer({ response, error, history, copied, onCopy }: Props) {
  const [tab, setTab] = React.useState<"response" | "history">("response");

  return (
    <div data-testid="response-viewer" style={{ display: "flex", flexDirection: "column", gap: 0 }}>
      {/* Tabs */}
      <div style={{ display: "flex", borderBottom: "1px solid #1e2d45", marginBottom: 12 }}>
        {(["response", "history"] as const).map(t => (
          <button
            key={t}
            data-testid={`tab-${t}`}
            onClick={() => setTab(t)}
            style={{
              padding: "7px 16px",
              fontSize: 9,
              fontWeight: 700,
              letterSpacing: "0.15em",
              textTransform: "uppercase",
              cursor: "pointer",
              fontFamily: "inherit",
              background: "transparent",
              border: "none",
              borderBottom: tab === t ? "2px solid #00e5ff" : "2px solid transparent",
              color: tab === t ? "#00e5ff" : "#3a5070",
              transition: "all 0.15s",
            }}
          >
            {t}
            {t === "history" && history.length > 0 && (
              <span style={{ marginLeft: 5, fontSize: 8, color: "#3a5070" }}>({history.length})</span>
            )}
          </button>
        ))}
      </div>

      {tab === "response" && (
        <div>
          {/* Status bar */}
          {response && (
            <div data-testid="response-status" style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 8 }}>
              <span style={{
                fontSize: 10, fontWeight: 700, fontFamily: "monospace",
                color: response.status >= 200 && response.status < 300 ? "#00ff9d" : "#ff5050",
              }}>
                {response.status || "ERR"}
              </span>
              <span style={{ fontSize: 9, color: "#3a5070" }}>{response.time}ms</span>
              <button
                data-testid="copy-response"
                onClick={onCopy}
                style={{
                  marginLeft: "auto", padding: "3px 10px", fontSize: 9, fontWeight: 700,
                  letterSpacing: "0.1em", cursor: "pointer", fontFamily: "inherit",
                  border: `1px solid ${copied ? "#00e5ff" : "#1e2d45"}`,
                  background: "transparent", color: copied ? "#00e5ff" : "#3a5070",
                  borderRadius: 4, transition: "all 0.15s",
                }}
              >
                {copied ? "✓ COPIED" : "COPY"}
              </button>
            </div>
          )}

          {/* Error */}
          {error && (
            <div data-testid="response-error" role="alert" style={{
              padding: "10px 12px", borderRadius: 6, fontSize: 11,
              border: "1px solid rgba(255,80,80,0.4)", background: "rgba(255,80,80,0.07)",
              color: "#ff9090", marginBottom: 8,
            }}>
              {error}
            </div>
          )}

          {/* Body */}
          {response ? (
            <pre
              data-testid="response-body"
              style={{
                margin: 0, padding: "12px 14px", borderRadius: 6, fontSize: 10,
                fontFamily: "monospace", lineHeight: 1.7, overflowX: "auto",
                background: "rgba(0,0,0,0.4)", border: "1px solid #1e2d45",
                color: "#c8d8ee", maxHeight: 320, overflowY: "auto",
              }}
            >
              {JSON.stringify(response.data, null, 2)}
            </pre>
          ) : (
            <div data-testid="response-empty" style={{ fontSize: 11, color: "#2a3d5a", padding: "20px 0", textAlign: "center" }}>
              Send a request to see the response
            </div>
          )}
        </div>
      )}

      {tab === "history" && (
        <div data-testid="history-list" style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          {history.length === 0 ? (
            <div style={{ fontSize: 11, color: "#2a3d5a", padding: "20px 0", textAlign: "center" }}>
              No requests yet
            </div>
          ) : (
            history.map(entry => (
              <div
                key={entry.id}
                data-testid={`history-entry-${entry.id}`}
                style={{
                  display: "flex", alignItems: "center", gap: 8, padding: "8px 10px",
                  borderRadius: 5, border: "1px solid #0f1a28", background: "rgba(0,0,0,0.2)",
                  fontSize: 10,
                }}
              >
                <span style={{ color: entry.success ? "#00ff9d" : "#ff5050", fontSize: 9, fontWeight: 700 }}>
                  {entry.status ?? "ERR"}
                </span>
                <span style={{ color: "#3a5070", fontSize: 9 }}>{entry.method}</span>
                <span style={{ fontFamily: "monospace", color: "#8aaad4", flex: 1 }}>{entry.endpoint}</span>
                <span style={{ color: "#2a3d5a", fontSize: 9 }}>{entry.sep}</span>
                <span style={{ color: "#2a3d5a", fontSize: 9 }}>{entry.time}</span>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
