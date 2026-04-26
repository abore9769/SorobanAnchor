import { useState, useEffect, useRef } from "react";
import { signTransaction } from "@stellar/freighter-api";
import { AnchorErrorBoundary } from "./AnchorErrorBoundary";

// ─── Types ────────────────────────────────────────────────────────────────────
type Step =
  | "idle"
  | "connect"
  | "challenge"
  | "sign"
  | "token"
  | "authenticated";

interface WalletInfo {
  address: string;
  network: string;
}

// ─── Wallet Helpers ───────────────────────────────────────────────────────────

/** Freighter browser extension API (injected at window.freighter) */
interface FreighterAPI {
  getPublicKey(): Promise<string>;
  signTransaction(xdr: string, opts?: { network?: string }): Promise<string>;
  isConnected(): Promise<boolean>;
}

declare global {
  interface Window {
    freighter?: FreighterAPI;
  }
}

/** Albedo intent API */
interface AlbedoResult {
  pubkey: string;
  signed_envelope_xdr?: string;
}

async function getWalletPublicKey(): Promise<{ address: string; wallet: "freighter" | "albedo" }> {
  // Try Freighter first
  if (window.freighter) {
    const connected = await window.freighter.isConnected();
    if (connected) {
      const address = await window.freighter.getPublicKey();
      return { address, wallet: "freighter" };
    }
  }
  // Fallback to Albedo
  const albedo = await import("@albedo-link/intent");
  const result: AlbedoResult = await albedo.default.publicKey({ require_existing: false });
  return { address: result.pubkey, wallet: "albedo" };
}

async function signWithWallet(xdr: string, network: string): Promise<string> {
  if (window.freighter) {
    return window.freighter.signTransaction(xdr, { network });
  }
  const albedo = await import("@albedo-link/intent");
  const result: AlbedoResult = await albedo.default.tx({ xdr, network: network.toLowerCase(), submit: false });
  return result.signed_envelope_xdr!;
}

const mockXDR = () => {
  const chars =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let xdr = "";
  for (let i = 0; i < 220; i++) {
    if (i > 0 && i % 64 === 0) xdr += "\n";
    xdr += chars[Math.floor(Math.random() * chars.length)];
  }
  return xdr + "==";
};

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// ─── Step Config ──────────────────────────────────────────────────────────────
const STEPS = [
  { id: "connect", label: "Connect Wallet", icon: "◎", num: 1 },
  { id: "challenge", label: "Fetch Challenge", icon: "⟁", num: 2 },
  { id: "sign", label: "Sign Challenge", icon: "✦", num: 3 },
  { id: "token", label: "Auth Token", icon: "◈", num: 4 },
];

// ─── Sub-components ───────────────────────────────────────────────────────────

function GlowRing({
  active,
  done,
  color,
}: {
  active: boolean;
  done: boolean;
  color: string;
}) {
  return (
    <div style={{ position: "relative", width: 48, height: 48, flexShrink: 0 }}>
      {/* Outer pulse */}
      {active && (
        <div
          style={{
            position: "absolute",
            inset: -6,
            borderRadius: "50%",
            border: `1px solid ${color}`,
            animation: "sep10-ping 1.4s ease-out infinite",
            opacity: 0,
          }}
        />
      )}
      {/* Ring */}
      <div
        style={{
          width: 48,
          height: 48,
          borderRadius: "50%",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          border: `1.5px solid ${done ? color : active ? color : "#1e2d45"}`,
          background: done
            ? `${color}20`
            : active
              ? `${color}12`
              : "rgba(0,0,0,0.3)",
          boxShadow:
            active || done
              ? `0 0 20px ${color}40, inset 0 0 12px ${color}10`
              : "none",
          transition: "all 0.4s",
          position: "relative",
          zIndex: 1,
        }}
      >
        {done ? (
          <span
            style={{
              fontSize: 18,
              color,
              filter: `drop-shadow(0 0 6px ${color})`,
            }}
          >
            ✓
          </span>
        ) : (
          <span
            style={{
              fontSize: 16,
              color: active ? color : "#2a3d5a",
              transition: "color 0.3s",
              filter: active ? `drop-shadow(0 0 6px ${color})` : "none",
            }}
          >
            {STEPS.find((s) => s.id === (active ? "active" : ""))?.icon ?? "○"}
          </span>
        )}
      </div>
    </div>
  );
}

function Connector({ done, color }: { done: boolean; color: string }) {
  return (
    <div
      style={{
        flex: 1,
        height: 1,
        position: "relative",
        margin: "0 8px",
        overflow: "hidden",
      }}
    >
      <div style={{ position: "absolute", inset: 0, background: "#1e2d45" }} />
      <div
        style={{
          position: "absolute",
          inset: 0,
          background: color,
          boxShadow: `0 0 8px ${color}`,
          transform: done ? "scaleX(1)" : "scaleX(0)",
          transformOrigin: "left",
          transition: "transform 0.6s cubic-bezier(0.4,0,0.2,1)",
        }}
      />
    </div>
  );
}

function TokenDisplay({ jwt }: { jwt: string }) {
  const [copied, setCopied] = useState(false);
  const parts = jwt.split(".");
  const colors = ["#ff7eb3", "#79d4fd", "#7effc7"];
  const labels = ["HEADER", "PAYLOAD", "SIGNATURE"];

  const copy = () => {
    navigator.clipboard.writeText(jwt);
    setCopied(true);
    setTimeout(() => setCopied(false), 1600);
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      {/* Segmented JWT */}
      <div
        style={{
          fontFamily: "monospace",
          fontSize: 10,
          lineHeight: 1.7,
          padding: "14px 16px",
          borderRadius: 8,
          background: "rgba(0,0,0,0.5)",
          border: "1px solid #1e2d45",
          wordBreak: "break-all",
        }}
      >
        {parts.map((part, i) => (
          <span key={i}>
            <span
              style={{
                color: colors[i],
                textShadow: `0 0 10px ${colors[i]}60`,
              }}
            >
              {part}
            </span>
            {i < 2 && <span style={{ color: "#2a3d5a" }}>.</span>}
          </span>
        ))}
      </div>

      {/* Decoded payload */}
      {(() => {
        try {
          const payload = JSON.parse(atob(parts[1]));
          return (
            <div
              style={{
                fontSize: 10,
                fontFamily: "monospace",
                padding: "12px 14px",
                borderRadius: 8,
                background: "rgba(0,0,0,0.3)",
                border: "1px solid #1e2d45",
                display: "flex",
                flexDirection: "column",
                gap: 5,
              }}
            >
              <div
                style={{
                  color: "#3a5070",
                  marginBottom: 4,
                  letterSpacing: "0.15em",
                  fontSize: 9,
                }}
              >
                DECODED PAYLOAD
              </div>
              {Object.entries(payload).map(([k, v]) => (
                <div key={k} style={{ display: "flex", gap: 10 }}>
                  <span style={{ color: "#ff7eb3", minWidth: 52 }}>{k}</span>
                  <span style={{ color: "#3a5070" }}>:</span>
                  <span
                    style={{
                      color: "#79d4fd",
                      flex: 1,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {k === "iat" || k === "exp"
                      ? new Date((v as number) * 1000).toLocaleString()
                      : String(v)}
                  </span>
                </div>
              ))}
            </div>
          );
        } catch {
          return null;
        }
      })()}

      <button
        onClick={copy}
        style={{
          alignSelf: "flex-end",
          padding: "6px 14px",
          fontSize: 9,
          fontWeight: 700,
          letterSpacing: "0.15em",
          textTransform: "uppercase",
          fontFamily: "monospace",
          borderRadius: 5,
          cursor: "pointer",
          transition: "all 0.2s",
          border: `1px solid ${copied ? "#00e5ff" : "#1e2d45"}`,
          color: copied ? "#00e5ff" : "#3a5070",
          background: copied ? "rgba(0,229,255,0.08)" : "transparent",
          boxShadow: copied ? "0 0 12px rgba(0,229,255,0.2)" : "none",
        }}
      >
        {copied ? "✓ COPIED" : "COPY JWT"}
      </button>
    </div>
  );
}

function AuthStatusBadge({ wallet }: { wallet: WalletInfo }) {
  const [age, setAge] = useState(0);
  useEffect(() => {
    const iv = setInterval(() => setAge((a) => a + 1), 1000);
    return () => clearInterval(iv);
  }, []);

  const expiresIn = 86400 - age;
  const pct = Math.max(0, (expiresIn / 86400) * 100);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      {/* Status banner */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: "14px 16px",
          borderRadius: 10,
          border: "1px solid rgba(0,255,157,0.3)",
          background: "rgba(0,255,157,0.06)",
          boxShadow: "0 0 30px rgba(0,255,157,0.08)",
        }}
      >
        <div
          style={{
            width: 10,
            height: 10,
            borderRadius: "50%",
            background: "#00ff9d",
            boxShadow: "0 0 14px #00ff9d",
            flexShrink: 0,
            animation: "sep10-pulse 2s infinite",
          }}
        />
        <div style={{ flex: 1 }}>
          <div
            style={{
              fontSize: 12,
              fontWeight: 700,
              color: "#00ff9d",
              letterSpacing: "0.1em",
            }}
          >
            AUTHENTICATED
          </div>
          <div style={{ fontSize: 10, color: "#3a5070", marginTop: 2 }}>
            Session active · SEP-10 verified
          </div>
        </div>
        <div style={{ textAlign: "right" }}>
          <div
            style={{ fontSize: 9, color: "#3a5070", letterSpacing: "0.1em" }}
          >
            EXPIRES IN
          </div>
          <div
            style={{
              fontSize: 13,
              fontWeight: 700,
              color: "#00ff9d",
              fontFamily: "monospace",
            }}
          >
            {Math.floor(expiresIn / 3600)}h{" "}
            {Math.floor((expiresIn % 3600) / 60)}m {expiresIn % 60}s
          </div>
        </div>
      </div>

      {/* Token expiry bar */}
      <div>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            fontSize: 9,
            color: "#3a5070",
            marginBottom: 5,
          }}
        >
          <span>TOKEN VALIDITY</span>
          <span>{pct.toFixed(1)}%</span>
        </div>
        <div
          style={{
            height: 4,
            borderRadius: 2,
            background: "#0d1628",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              height: "100%",
              borderRadius: 2,
              width: `${pct}%`,
              background: "linear-gradient(90deg,#00e5ff,#00ff9d)",
              boxShadow: "0 0 8px #00e5ff60",
              transition: "width 1s linear",
            }}
          />
        </div>
      </div>

      {/* Wallet info grid */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
        {[
          {
            label: "ACCOUNT",
            value: `${wallet.address.slice(0, 6)}...${wallet.address.slice(-6)}`,
          },
          { label: "NETWORK", value: wallet.network },
          { label: "PROTOCOL", value: "SEP-10" },
          { label: "AUTH METHOD", value: "ED25519" },
        ].map(({ label, value }) => (
          <div
            key={label}
            style={{
              padding: "10px 12px",
              borderRadius: 7,
              border: "1px solid #131f32",
              background: "rgba(0,0,0,0.25)",
            }}
          >
            <div
              style={{
                fontSize: 8,
                color: "#2a3d5a",
                letterSpacing: "0.15em",
                marginBottom: 4,
              }}
            >
              {label}
            </div>
            <div
              style={{
                fontSize: 11,
                color: "#8aaad4",
                fontFamily: "monospace",
              }}
            >
              {value}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Loading Skeleton ─────────────────────────────────────────────────────────

function LoadingSkeleton() {
  return (
    <div
      data-testid="loading-skeleton"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 10,
        padding: "14px 0",
      }}
    >
      {[80, 60, 90].map((w, i) => (
        <div
          key={i}
          style={{
            height: 12,
            borderRadius: 4,
            width: `${w}%`,
            background: "linear-gradient(90deg,#0d1628 25%,#1e2d45 50%,#0d1628 75%)",
            backgroundSize: "200% 100%",
            animation: "sep10-shimmer 1.4s infinite",
          }}
        />
      ))}
    </div>
  );
}

// ─── Main ─────────────────────────────────────────────────────────────────────
function SEP10AuthFlowInner() {
  const [step, setStep] = useState<Step>("idle");
  const [wallet, setWallet] = useState<WalletInfo | null>(null);
  const [challenge, setChallenge] = useState<string | null>(null);
  const [networkPassphrase, setNetworkPassphrase] = useState<string>("Test SDF Network ; September 2015");
  const [webAuthEndpoint, setWebAuthEndpoint] = useState<string | null>(null);
  const [signedXdr, setSignedXdr] = useState<string | null>(null);
  const [jwt, setJwt] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [errorStep, setErrorStep] = useState<Step | null>(null);
  const [domain, setDomain] = useState("testanchor.stellar.org");
  const [log, setLog] = useState<string[]>([]);
  const logRef = useRef<HTMLDivElement>(null);

  const addLog = (msg: string) =>
    setLog((prev) => [
      ...prev.slice(-40),
      `[${new Date().toLocaleTimeString()}] ${msg}`,
    ]);

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [log]);

  const NEON = "#00e5ff";

  const completedSteps = (() => {
    const map: Record<string, boolean> = {};
    if (wallet) map["connect"] = true;
    if (challenge) map["challenge"] = true;
    if (signedXdr) map["sign"] = true;
    if (jwt) map["token"] = true;
    return map;
  })();

  const activeStep = jwt
    ? "token"
    : signedXdr
      ? "sign"
      : challenge
        ? "challenge"
        : wallet
          ? "connect"
          : "";

  // ── Step handlers ──
  const connectWallet = async () => {
    setLoading(true);
    setError(null);
    setErrorStep(null);
    addLog("Requesting wallet connection...");
    try {
      addLog("Scanning for available wallets (Freighter, Albedo)...");
      const { address, wallet: walletName } = await getWalletPublicKey();
      addLog(`${walletName} wallet found: ${address.slice(0, 8)}...`);
      addLog("Connection approved ✓");
      setWallet({ address, network: "Testnet" });
      setStep("challenge");
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Wallet connection failed";
      setError(msg);
      addLog(`Error: ${msg}`);
    } finally {
      setLoading(false);
    }
  };

  const fetchChallenge = async () => {
    setLoading(true);
    setError(null);
    setErrorStep(null);
    try {
      addLog(`GET https://${domain}/.well-known/stellar.toml`);
      const tomlRes = await fetch(`https://${domain}/.well-known/stellar.toml`);
      if (!tomlRes.ok) throw new Error(`stellar.toml fetch failed: ${tomlRes.status}`);
      const toml = await tomlRes.text();
      const match = toml.match(/WEB_AUTH_ENDPOINT\s*=\s*"([^"]+)"/);
      if (!match) throw new Error("WEB_AUTH_ENDPOINT not found in stellar.toml");
      const endpoint = match[1];
      addLog(`web_auth_endpoint: ${endpoint}`);
      setWebAuthEndpoint(endpoint);

      const url = `${endpoint}?account=${wallet!.address}`;
      addLog(`GET ${url}`);
      const res = await fetch(url);
      if (!res.ok) throw new Error(`Challenge fetch failed: ${res.status}`);
      const data = await res.json();
      if (!data.transaction) throw new Error("No transaction field in challenge response");
      addLog(`Response: 200 OK`);
      addLog(`Challenge XDR received (${data.transaction.length} chars)`);
      setChallenge(data.transaction);
      if (data.network_passphrase) setNetworkPassphrase(data.network_passphrase);
      setStep("sign");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      addLog(`Error: ${msg}`);
      setError(msg);
      setErrorStep("challenge");
    } finally {
      setLoading(false);
    }
  };

  const signChallenge = async () => {
    setLoading(true);
    setError(null);
    setErrorStep(null);
    try {
      addLog("Sending challenge XDR to Freighter for signing...");
      const result = await signTransaction(challenge!, { networkPassphrase });
      if (result.error) throw new Error(String(result.error));
      addLog("User approved signature request");
      addLog("Transaction signed with ED25519 key ✓");
      setSignedXdr(result.signedTxXdr);
      setStep("token");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      addLog(`Error: ${msg}`);
      setError(msg);
      setErrorStep("sign");
    } finally {
      setLoading(false);
    }
  };

  const submitChallenge = async () => {
    setLoading(true);
    setError(null);
    setErrorStep(null);
    try {
      const endpoint = webAuthEndpoint ?? `https://${domain}/auth`;
      addLog(`POST ${endpoint}`);
      addLog("Sending signed XDR...");
      const res = await fetch(endpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ transaction: signedXdr }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? `POST /auth failed: ${res.status}`);
      if (!data.token) throw new Error("No token in auth response");
      addLog("Response: 200 OK");
      addLog("JWT received ✓");
      addLog("Auth session established — expires in 24h");
      setJwt(data.token);
      setStep("authenticated");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      addLog(`Error: ${msg}`);
      setError(msg);
      setErrorStep("token");
    } finally {
      setLoading(false);
    }
  };

  const reset = () => {
    setStep("idle");
    setWallet(null);
    setChallenge(null);
    setNetworkPassphrase("Test SDF Network ; September 2015");
    setWebAuthEndpoint(null);
    setSignedXdr(null);
    setJwt(null);
    setError(null);
    setErrorStep(null);
    addLog("─── Session reset ───");
  };

  const logout = () => {
    setJwt(null);
    setStep("token");
    addLog("─── Token cleared — logged out ───");
  };

  // Auto-refresh token when < 60 seconds remaining
  useEffect(() => {
    if (!jwt) return;
    try {
      const parts = jwt.split(".");
      const payload = JSON.parse(atob(parts[1]));
      if (!payload.exp) return;
      const msUntilRefresh = (payload.exp * 1000) - Date.now() - 60_000;
      if (msUntilRefresh <= 0) {
        logout();
        return;
      }
      const timer = setTimeout(() => {
        addLog("Token expiring soon — refreshing...");
        logout();
      }, msUntilRefresh);
      return () => clearTimeout(timer);
    } catch {
      // non-standard JWT — skip refresh
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jwt]);

  const retryFromStep = () => {
    setError(null);
    if (errorStep === "challenge") { setChallenge(null); setStep("challenge"); }
    else if (errorStep === "sign") { setSignedXdr(null); setStep("sign"); }
    else if (errorStep === "token") { setSignedXdr(null); setStep("token"); }
    else reset();
    setErrorStep(null);
  };

  // ─ Step cards config ─
  const stepCards = [
    {
      id: "connect",
      title: "Connect Wallet",
      icon: "◎",
      subtitle: "Link your Stellar keypair",
      ready: step === "idle" || step === "connect",
      done: !!wallet,
      content: wallet ? (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 10,
              padding: "12px 14px",
              borderRadius: 8,
              background: "rgba(0,229,255,0.06)",
              border: "1px solid rgba(0,229,255,0.2)",
            }}
          >
            <div
              style={{
                width: 36,
                height: 36,
                borderRadius: "50%",
                background: "linear-gradient(135deg,#00e5ff22,#00ff9d22)",
                border: "1px solid #00e5ff40",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                fontSize: 18,
                flexShrink: 0,
              }}
            >
              ◉
            </div>
            <div>
              <div
                style={{
                  fontSize: 10,
                  color: "#3a5070",
                  letterSpacing: "0.12em",
                  marginBottom: 3,
                }}
              >
                CONNECTED ACCOUNT
              </div>
              <div
                style={{
                  fontSize: 11,
                  fontFamily: "monospace",
                  color: "#8aaad4",
                }}
              >
                {wallet.address.slice(0, 12)}...{wallet.address.slice(-8)}
              </div>
            </div>
            <div
              style={{
                marginLeft: "auto",
                fontSize: 9,
                padding: "3px 8px",
                borderRadius: 4,
                background: "rgba(0,255,157,0.1)",
                color: "#00ff9d",
                border: "1px solid rgba(0,255,157,0.3)",
              }}
            >
              {wallet.network}
            </div>
          </div>
        </div>
      ) : (
        <div>
          <p
            style={{
              fontSize: 11,
              color: "#3a5070",
              lineHeight: 1.6,
              marginBottom: 14,
            }}
          >
            Connect your Stellar wallet to begin the SEP-10 authentication
            handshake. Your private key never leaves your wallet.
          </p>
          <button
            onClick={connectWallet}
            disabled={loading}
            style={btnStyle(NEON, loading)}
          >
            {loading ? <Spinner /> : <>{walletIcon} Connect Wallet</>}
          </button>
        </div>
      ),
    },
    {
      id: "challenge",
      title: "Fetch Challenge",
      icon: "⟁",
      subtitle: "Server issues a signed transaction",
      ready: !!wallet,
      done: !!challenge,
      content: challenge ? (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <div
            style={{
              fontSize: 9,
              color: "#3a5070",
              letterSpacing: "0.12em",
              marginBottom: 4,
            }}
          >
            CHALLENGE XDR
          </div>
          <div
            style={{
              fontFamily: "monospace",
              fontSize: 10,
              lineHeight: 1.6,
              padding: "12px 14px",
              borderRadius: 8,
              background: "rgba(0,0,0,0.4)",
              border: "1px solid #131f32",
              wordBreak: "break-all",
              color: "#79d4fd",
              maxHeight: 90,
              overflowY: "auto",
            }}
          >
            {challenge.slice(0, 160)}…
          </div>
          <div style={{ fontSize: 9, color: "#2a3d5a" }}>
            Received from{" "}
            <span style={{ color: "#3a5070" }}>{domain}/auth</span>
          </div>
        </div>
      ) : (
        <div>
          <p
            style={{
              fontSize: 11,
              color: "#3a5070",
              lineHeight: 1.6,
              marginBottom: 14,
            }}
          >
            The anchor generates a unique challenge transaction. This XDR must
            be signed by your wallet to prove key ownership.
          </p>
          {loading && step === "challenge" ? <LoadingSkeleton /> : (
          <button
            onClick={fetchChallenge}
            disabled={loading || !wallet}
            style={btnStyle(NEON, loading || !wallet)}
          >
            {loading ? <Spinner /> : <>{fetchIcon} Fetch Challenge</>}
          </button>
          )}
        </div>
      ),
    },
    {
      id: "sign",
      title: "Sign Challenge",
      icon: "✦",
      subtitle: "Prove keypair ownership",
      ready: !!challenge,
      done: !!signedXdr,
      content: signedXdr ? (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <div
            style={{
              fontSize: 9,
              color: "#3a5070",
              letterSpacing: "0.12em",
              marginBottom: 4,
            }}
          >
            SIGNED XDR
          </div>
          <div
            style={{
              fontFamily: "monospace",
              fontSize: 10,
              lineHeight: 1.6,
              padding: "12px 14px",
              borderRadius: 8,
              background: "rgba(0,0,0,0.4)",
              border: "1px solid #131f32",
              wordBreak: "break-all",
              color: "#7effc7",
              maxHeight: 90,
              overflowY: "auto",
            }}
          >
            {signedXdr.slice(0, 160)}…
          </div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              fontSize: 9,
              color: "#00ff9d",
            }}
          >
            <span>✓</span> ED25519 signature applied
          </div>
        </div>
      ) : (
        <div>
          <p
            style={{
              fontSize: 11,
              color: "#3a5070",
              lineHeight: 1.6,
              marginBottom: 14,
            }}
          >
            Your wallet signs the challenge XDR with your private key. This
            creates cryptographic proof that you control the account — no
            password needed.
          </p>
          <button
            onClick={signChallenge}
            disabled={loading || !challenge}
            style={btnStyle("#7effc7", loading || !challenge)}
          >
            {loading ? <Spinner /> : <>{signIcon} Sign with Wallet</>}
          </button>
        </div>
      ),
    },
    {
      id: "token",
      title: "Auth Token",
      icon: "◈",
      subtitle: "Receive your JWT bearer token",
      ready: !!signedXdr,
      done: !!jwt,
      content: jwt ? (
        <TokenDisplay jwt={jwt} />
      ) : (
        <div>
          <p
            style={{
              fontSize: 11,
              color: "#3a5070",
              lineHeight: 1.6,
              marginBottom: 14,
            }}
          >
            Submit the signed XDR back to the anchor. If valid, a JWT is issued
            — use it as a{" "}
            <code style={{ color: "#79d4fd", fontSize: 10 }}>Bearer</code> token
            in all subsequent SEP requests.
          </p>
          <button
            onClick={submitChallenge}
            disabled={loading || !signedXdr}
            style={btnStyle("#ff7eb3", loading || !signedXdr)}
          >
            {loading ? <Spinner /> : <>{submitIcon} Submit & Get Token</>}
          </button>
        </div>
      ),
    },
  ];

  return (
    <div
      style={{
        fontFamily: "'JetBrains Mono','Fira Code',monospace",
        minHeight: "100vh",
        background: "#050810",
        color: "#c8d8ee",
        position: "relative",
        overflow: "hidden",
      }}
    >
      <style>{`
        @import url('https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;700&display=swap');
        @keyframes sep10-ping { 0%{transform:scale(1);opacity:0.7} 100%{transform:scale(1.9);opacity:0} }
        @keyframes sep10-pulse { 0%,100%{opacity:1} 50%{opacity:0.4} }
        @keyframes sep10-spin { to{transform:rotate(360deg)} }
        @keyframes sep10-slide-in { from{opacity:0;transform:translateY(10px)} to{opacity:1;transform:translateY(0)} }
        @keyframes sep10-float { 0%,100%{transform:translateY(0)} 50%{transform:translateY(-8px)} }
        @keyframes sep10-shimmer { 0%{background-position:200% 0} 100%{background-position:-200% 0} }
        ::-webkit-scrollbar{width:4px;height:4px}
        ::-webkit-scrollbar-track{background:transparent}
        ::-webkit-scrollbar-thumb{background:#1e2d4580;border-radius:2px}
      `}</style>

      {/* Grid */}
      <div
        style={{
          position: "fixed",
          inset: 0,
          pointerEvents: "none",
          backgroundImage:
            "linear-gradient(rgba(0,229,255,0.03) 1px,transparent 1px),linear-gradient(90deg,rgba(0,229,255,0.03) 1px,transparent 1px)",
          backgroundSize: "44px 44px",
        }}
      />

      {/* Glows */}
      <div
        style={{
          position: "fixed",
          top: -200,
          left: "10%",
          width: 600,
          height: 600,
          borderRadius: "50%",
          background: "#00e5ff",
          opacity: 0.04,
          filter: "blur(120px)",
          pointerEvents: "none",
        }}
      />
      <div
        style={{
          position: "fixed",
          bottom: -100,
          right: "5%",
          width: 400,
          height: 400,
          borderRadius: "50%",
          background: "#00ff9d",
          opacity: 0.03,
          filter: "blur(100px)",
          pointerEvents: "none",
        }}
      />

      {/* Corner brackets */}
      {["tl", "tr", "bl", "br"].map((pos) => (
        <div
          key={pos}
          style={{
            position: "fixed",
            width: 24,
            height: 24,
            opacity: 0.3,
            pointerEvents: "none",
            top: pos.includes("t") ? 10 : undefined,
            bottom: pos.includes("b") ? 10 : undefined,
            left: pos.includes("l") ? 10 : undefined,
            right: pos.includes("r") ? 10 : undefined,
            borderTop: pos.includes("t") ? `1.5px solid ${NEON}` : undefined,
            borderBottom: pos.includes("b") ? `1.5px solid ${NEON}` : undefined,
            borderLeft: pos.includes("l") ? `1.5px solid ${NEON}` : undefined,
            borderRight: pos.includes("r") ? `1.5px solid ${NEON}` : undefined,
          }}
        />
      ))}

      <div
        style={{
          maxWidth: 960,
          margin: "0 auto",
          padding: "32px 24px",
          position: "relative",
          zIndex: 1,
        }}
      >
        {/* ── Header ── */}
        <div
          style={{
            display: "flex",
            alignItems: "flex-start",
            justifyContent: "space-between",
            marginBottom: 36,
          }}
        >
          <div>
            <div
              style={{
                fontSize: 10,
                letterSpacing: "0.25em",
                color: "#3a5070",
                textTransform: "uppercase",
                marginBottom: 8,
              }}
            >
              ◈ Stellar · SEP-10
            </div>
            <h1
              style={{
                fontSize: 26,
                fontWeight: 700,
                letterSpacing: "-0.02em",
                color: "#dde6f5",
                margin: 0,
                lineHeight: 1.2,
              }}
            >
              Authentication
              <br />
              <span style={{ color: NEON, textShadow: `0 0 30px ${NEON}60` }}>
                Flow
              </span>
            </h1>
            <p
              style={{
                marginTop: 10,
                fontSize: 11,
                color: "#3a5070",
                lineHeight: 1.6,
                maxWidth: 340,
              }}
            >
              Challenge-response authentication using your Stellar keypair. No
              passwords. Cryptographic proof of account ownership.
            </p>
          </div>

          {/* Domain + reset */}
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              alignItems: "flex-end",
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "7px 12px",
                borderRadius: 7,
                border: "1px solid #131f32",
                background: "rgba(0,0,0,0.35)",
              }}
            >
              <span style={{ fontSize: 10, color: "#2a3d5a" }}>https://</span>
              <input
                value={domain}
                onChange={(e) => setDomain(e.target.value)}
                style={{
                  background: "transparent",
                  border: "none",
                  outline: "none",
                  color: NEON,
                  fontSize: 10,
                  fontFamily: "inherit",
                  width: 200,
                }}
                placeholder="anchor.example.com"
              />
            </div>
            {step !== "idle" && (
              <button
                onClick={reset}
                style={{
                  fontSize: 9,
                  fontWeight: 700,
                  letterSpacing: "0.15em",
                  textTransform: "uppercase",
                  cursor: "pointer",
                  fontFamily: "inherit",
                  padding: "5px 12px",
                  borderRadius: 5,
                  border: "1px solid #1e2d45",
                  background: "transparent",
                  color: "#2a3d5a",
                  transition: "all 0.2s",
                }}
              >
                ↺ RESET
              </button>
            )}
          </div>
        </div>

        {/* ── Progress bar ── */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            marginBottom: 40,
            padding: "0 4px",
          }}
        >
          {STEPS.map((s, i) => {
            const isDone = completedSteps[s.id];
            const isActive =
              (!isDone && activeStep === s.id) ||
              (!activeStep && i === 0 && step === "idle");
            const stepColor =
              s.id === "sign" ? "#7effc7" : s.id === "token" ? "#ff7eb3" : NEON;
            return (
              <div
                key={s.id}
                style={{ display: "flex", alignItems: "center", flex: 1 }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    flexShrink: 0,
                  }}
                >
                  <GlowRing active={isActive} done={isDone} color={stepColor} />
                  <div style={{ display: i < 3 ? "none" : undefined }}>
                    {/* hide labels for middle steps on small screens */}
                  </div>
                </div>
                {i < STEPS.length - 1 && (
                  <Connector
                    done={
                      completedSteps[STEPS[i + 1]?.id] || completedSteps[s.id]
                    }
                    color={stepColor}
                  />
                )}
              </div>
            );
          })}
        </div>

        {/* Step labels row */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(4, 1fr)",
            marginBottom: 32,
            gap: 4,
          }}
        >
          {STEPS.map((s) => {
            const isDone = completedSteps[s.id];
            const stepColor =
              s.id === "sign" ? "#7effc7" : s.id === "token" ? "#ff7eb3" : NEON;
            return (
              <div key={s.id} style={{ textAlign: "center" }}>
                <div
                  style={{
                    fontSize: 11,
                    fontWeight: 700,
                    color: isDone ? stepColor : "#2a3d5a",
                    letterSpacing: "0.05em",
                    transition: "color 0.3s",
                  }}
                >
                  {s.label}
                </div>
                <div
                  style={{
                    fontSize: 9,
                    color: "#1e2d45",
                    marginTop: 2,
                    letterSpacing: "0.1em",
                  }}
                >
                  STEP {s.num}
                </div>
              </div>
            );
          })}
        </div>

        {/* ── Error Banner ── */}
        {error && (
          <div
            role="alert"
            style={{
              marginBottom: 16,
              padding: "14px 16px",
              borderRadius: 10,
              border: "1px solid rgba(255,80,80,0.4)",
              background: "rgba(255,80,80,0.07)",
              display: "flex",
              alignItems: "flex-start",
              gap: 12,
              animation: "sep10-slide-in 0.3s ease",
            }}
          >
            <span style={{ fontSize: 16, flexShrink: 0, color: "#ff5050" }}>⚠</span>
            <div style={{ flex: 1, fontSize: 11, color: "#ff9090", lineHeight: 1.5 }}>
              {error}
            </div>
            <button
              onClick={retryFromStep}
              style={{
                flexShrink: 0,
                padding: "5px 12px",
                fontSize: 9,
                fontWeight: 700,
                letterSpacing: "0.15em",
                textTransform: "uppercase",
                fontFamily: "inherit",
                cursor: "pointer",
                borderRadius: 5,
                border: "1px solid rgba(255,80,80,0.5)",
                background: "rgba(255,80,80,0.12)",
                color: "#ff7070",
                transition: "all 0.2s",
              }}
            >
              ↺ Try Again
            </button>
          </div>
        )}

        {/* ── Step Cards Grid ── */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(2, 1fr)",
            gap: 16,
            marginBottom: 24,
          }}
        >
          {stepCards.map((card) => {
            const isActive = !card.done && card.ready && !jwt;
            const stepColor =
              card.id === "sign"
                ? "#7effc7"
                : card.id === "token"
                  ? "#ff7eb3"
                  : NEON;
            return (
              <div
                key={card.id}
                style={{
                  borderRadius: 12,
                  padding: "20px 20px",
                  border: `1px solid ${card.done ? `${stepColor}35` : isActive ? `${stepColor}25` : "#0f1a28"}`,
                  background: card.done
                    ? `${stepColor}06`
                    : isActive
                      ? "rgba(0,0,0,0.45)"
                      : "rgba(0,0,0,0.2)",
                  boxShadow: card.done
                    ? `0 0 40px ${stepColor}08`
                    : isActive
                      ? `0 0 30px ${stepColor}05`
                      : "none",
                  transition: "all 0.4s",
                  animation: isActive ? "sep10-slide-in 0.35s ease" : undefined,
                  opacity: !card.ready && !card.done ? 0.35 : 1,
                  position: "relative",
                  overflow: "hidden",
                }}
              >
                {/* Active shimmer */}
                {isActive && (
                  <div
                    style={{
                      position: "absolute",
                      top: 0,
                      left: "-100%",
                      width: "60%",
                      height: "100%",
                      background: `linear-gradient(90deg, transparent, ${stepColor}06, transparent)`,
                      animation: "sep10-float 3s ease-in-out infinite",
                      pointerEvents: "none",
                    }}
                  />
                )}

                {/* Card header */}
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    marginBottom: 14,
                  }}
                >
                  <div
                    style={{
                      width: 32,
                      height: 32,
                      borderRadius: 8,
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      fontSize: 15,
                      border: `1px solid ${card.done ? `${stepColor}50` : isActive ? `${stepColor}35` : "#131f32"}`,
                      background: card.done
                        ? `${stepColor}15`
                        : isActive
                          ? `${stepColor}08`
                          : "transparent",
                      color: card.done
                        ? stepColor
                        : isActive
                          ? stepColor
                          : "#1e2d45",
                      boxShadow:
                        card.done || isActive
                          ? `0 0 14px ${stepColor}30`
                          : "none",
                      transition: "all 0.4s",
                    }}
                  >
                    {card.done ? "✓" : card.icon}
                  </div>
                  <div>
                    <div
                      style={{
                        fontSize: 12,
                        fontWeight: 700,
                        color: card.done
                          ? stepColor
                          : isActive
                            ? "#c8d8ee"
                            : "#2a3d5a",
                        letterSpacing: "0.04em",
                        transition: "color 0.3s",
                      }}
                    >
                      {card.title}
                    </div>
                    <div
                      style={{
                        fontSize: 9,
                        color: "#2a3d5a",
                        marginTop: 1,
                        letterSpacing: "0.1em",
                      }}
                    >
                      {card.subtitle}
                    </div>
                  </div>
                  {card.done && (
                    <div
                      style={{
                        marginLeft: "auto",
                        fontSize: 9,
                        padding: "2px 8px",
                        borderRadius: 10,
                        background: `${stepColor}15`,
                        color: stepColor,
                        border: `1px solid ${stepColor}30`,
                      }}
                    >
                      COMPLETE
                    </div>
                  )}
                </div>

                {/* Card content */}
                <div>{card.content}</div>
              </div>
            );
          })}
        </div>

        {/* ── Auth Status (full width, post-auth) ── */}
        {step === "authenticated" && wallet && (
          <div
            style={{
              borderRadius: 12,
              padding: "24px 24px",
              border: "1px solid rgba(0,255,157,0.2)",
              background: "rgba(0,255,157,0.04)",
              boxShadow: "0 0 60px rgba(0,255,157,0.06)",
              animation: "sep10-slide-in 0.4s ease",
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                marginBottom: 18,
              }}
            >
              <div
                style={{
                  width: 32,
                  height: 32,
                  borderRadius: 8,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontSize: 16,
                  border: "1px solid rgba(0,255,157,0.4)",
                  background: "rgba(0,255,157,0.1)",
                  color: "#00ff9d",
                  boxShadow: "0 0 16px rgba(0,255,157,0.25)",
                }}
              >
                ◉
              </div>
              <div>
                <div
                  style={{
                    fontSize: 12,
                    fontWeight: 700,
                    color: "#00ff9d",
                    letterSpacing: "0.06em",
                  }}
                >
                  Auth Status
                </div>
                <div
                  style={{
                    fontSize: 9,
                    color: "#2a3d5a",
                    marginTop: 1,
                    letterSpacing: "0.1em",
                  }}
                >
                  Live session monitor
                </div>
              </div>
            </div>
            <AuthStatusBadge wallet={wallet} />
            <button
              data-testid="logout-btn"
              onClick={logout}
              style={{
                marginTop: 12,
                padding: "7px 16px",
                fontSize: 9,
                fontWeight: 700,
                letterSpacing: "0.15em",
                textTransform: "uppercase",
                cursor: "pointer",
                fontFamily: "inherit",
                borderRadius: 5,
                border: "1px solid rgba(255,80,80,0.4)",
                background: "transparent",
                color: "#ff7070",
                transition: "all 0.2s",
              }}
            >
              ✕ LOGOUT
            </button>
          </div>
        )}

        {/* ── Activity Log ── */}
        {log.length > 0 && (
          <div
            style={{
              marginTop: 20,
              borderRadius: 10,
              overflow: "hidden",
              border: "1px solid #0f1a28",
            }}
          >
            <div
              style={{
                padding: "8px 14px",
                background: "rgba(0,0,0,0.6)",
                borderBottom: "1px solid #0f1a28",
                display: "flex",
                alignItems: "center",
                gap: 8,
              }}
            >
              <div
                style={{
                  width: 7,
                  height: 7,
                  borderRadius: "50%",
                  background: NEON,
                  animation: "sep10-pulse 2s infinite",
                  boxShadow: `0 0 8px ${NEON}`,
                }}
              />
              <span
                style={{
                  fontSize: 9,
                  fontWeight: 700,
                  letterSpacing: "0.2em",
                  color: "#2a3d5a",
                  textTransform: "uppercase",
                }}
              >
                Activity Log
              </span>
            </div>
            <div
              ref={logRef}
              style={{
                padding: "12px 14px",
                background: "rgba(0,0,0,0.45)",
                maxHeight: 130,
                overflowY: "auto",
                display: "flex",
                flexDirection: "column",
                gap: 3,
              }}
            >
              {log.map((entry, i) => (
                <div
                  key={i}
                  style={{
                    fontSize: 10,
                    fontFamily: "monospace",
                    color: i === log.length - 1 ? "#8aaad4" : "#2a3d5a",
                    transition: "color 0.3s",
                  }}
                >
                  {entry}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Bottom HUD */}
        <div
          style={{
            marginTop: 32,
            display: "flex",
            justifyContent: "center",
            gap: 6,
          }}
        >
          {STEPS.map((s) => (
            <div
              key={s.id}
              style={{
                width: completedSteps[s.id] ? 20 : 6,
                height: 3,
                borderRadius: 2,
                background: completedSteps[s.id] ? NEON : "#131f32",
                boxShadow: completedSteps[s.id] ? `0 0 8px ${NEON}` : "none",
                transition: "all 0.4s",
              }}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

// ─── Inline helpers ────────────────────────────────────────────────────────────
function btnStyle(color: string, disabled: boolean): React.CSSProperties {
  return {
    display: "inline-flex",
    alignItems: "center",
    gap: 8,
    padding: "10px 18px",
    borderRadius: 7,
    fontSize: 10,
    fontWeight: 700,
    letterSpacing: "0.15em",
    textTransform: "uppercase",
    fontFamily: "inherit",
    cursor: disabled ? "not-allowed" : "pointer",
    transition: "all 0.2s",
    border: `1px solid ${disabled ? "#131f32" : color}`,
    background: disabled ? "transparent" : `${color}14`,
    color: disabled ? "#1e2d45" : color,
    boxShadow: disabled ? "none" : `0 0 18px ${color}28`,
  };
}

const Spinner = () => (
  <svg
    style={{
      width: 13,
      height: 13,
      animation: "sep10-spin 0.7s linear infinite",
    }}
    fill="none"
    viewBox="0 0 24 24"
  >
    <circle
      style={{ opacity: 0.2 }}
      cx="12"
      cy="12"
      r="10"
      stroke="currentColor"
      strokeWidth="3"
    />
    <path
      style={{ opacity: 0.9 }}
      fill="currentColor"
      d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
    />
  </svg>
);

const walletIcon = (
  <svg
    style={{ width: 13, height: 13 }}
    fill="none"
    stroke="currentColor"
    viewBox="0 0 24 24"
  >
    <path
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
      d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z"
    />
  </svg>
);
const fetchIcon = (
  <svg
    style={{ width: 13, height: 13 }}
    fill="none"
    stroke="currentColor"
    viewBox="0 0 24 24"
  >
    <path
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
      d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
    />
  </svg>
);
const signIcon = (
  <svg
    style={{ width: 13, height: 13 }}
    fill="none"
    stroke="currentColor"
    viewBox="0 0 24 24"
  >
    <path
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
      d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z"
    />
  </svg>
);
const submitIcon = (
  <svg
    style={{ width: 13, height: 13 }}
    fill="none"
    stroke="currentColor"
    viewBox="0 0 24 24"
  >
    <path
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
      d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"
    />
  </svg>
);

// ─── Wrapped export with AnchorErrorBoundary ─────────────────────────────────
export default function SEP10AuthFlow() {
  return (
    <AnchorErrorBoundary componentLabel="SEP-10 Auth Flow">
      <SEP10AuthFlowInner />
    </AnchorErrorBoundary>
  );
}
