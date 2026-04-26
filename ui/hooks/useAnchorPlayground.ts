import { useState, useCallback } from "react";
import { useCopyToClipboard } from "../hooks/useCopyToClipboard";

// ─── Types ────────────────────────────────────────────────────────────────────
export type HttpMethod = "GET" | "POST" | "PUT";

export interface Param {
  key: string;
  label: string;
  placeholder: string;
  required?: boolean;
}

export interface Endpoint {
  id: string;
  method: HttpMethod;
  path: string;
  description: string;
  params: Param[];
}

export interface SEPProtocol {
  id: string;
  name: string;
  tag: string;
  color: string;
  description: string;
  endpoints: Endpoint[];
}

export interface HistoryEntry {
  id: string;
  time: string;
  sep: string;
  endpoint: string;
  method: HttpMethod;
  status: number | null;
  success: boolean;
}

export interface ResponseState {
  data: unknown;
  status: number;
  time: number;
}

// ─── SEP Data ─────────────────────────────────────────────────────────────────
export const SEP_PROTOCOLS: SEPProtocol[] = [
  {
    id: "sep1",
    name: "SEP-1",
    tag: "Stellar.toml",
    color: "cyan",
    description: "Stellar Info File – exposes anchor metadata & capabilities",
    endpoints: [
      { id: "stellar-toml", method: "GET", path: "/.well-known/stellar.toml", description: "Fetch Stellar TOML config", params: [] },
    ],
  },
  {
    id: "sep6",
    name: "SEP-6",
    tag: "Deposit & Withdraw",
    color: "blue",
    description: "Programmatic deposit & withdrawal via query params",
    endpoints: [
      { id: "sep6-info", method: "GET", path: "/transfer/info", description: "Retrieve supported assets", params: [] },
      {
        id: "sep6-deposit", method: "GET", path: "/transfer/deposit", description: "Initiate a deposit",
        params: [
          { key: "asset_code", label: "Asset Code", placeholder: "USDC", required: true },
          { key: "account", label: "Stellar Account", placeholder: "G...", required: true },
          { key: "amount", label: "Amount", placeholder: "100" },
        ],
      },
      {
        id: "sep6-withdraw", method: "GET", path: "/transfer/withdraw", description: "Initiate a withdrawal",
        params: [
          { key: "asset_code", label: "Asset Code", placeholder: "USDC", required: true },
          { key: "type", label: "Type", placeholder: "bank_account", required: true },
          { key: "dest", label: "Destination", placeholder: "Bank account number" },
          { key: "amount", label: "Amount", placeholder: "100" },
        ],
      },
    ],
  },
  {
    id: "sep10",
    name: "SEP-10",
    tag: "Auth",
    color: "green",
    description: "Challenge-response authentication for Stellar accounts",
    endpoints: [
      {
        id: "sep10-challenge", method: "GET", path: "/auth", description: "Get a challenge transaction",
        params: [
          { key: "account", label: "Stellar Account", placeholder: "G...", required: true },
          { key: "memo", label: "Memo ID", placeholder: "optional integer memo" },
        ],
      },
      {
        id: "sep10-token", method: "POST", path: "/auth", description: "Submit signed challenge → JWT",
        params: [{ key: "transaction", label: "Signed XDR", placeholder: "base64-encoded XDR", required: true }],
      },
    ],
  },
  {
    id: "sep24",
    name: "SEP-24",
    tag: "Interactive",
    color: "violet",
    description: "Hosted deposit & withdrawal with interactive UI flow",
    endpoints: [
      { id: "sep24-info", method: "GET", path: "/sep24/info", description: "Get assets & config", params: [] },
      {
        id: "sep24-deposit", method: "POST", path: "/sep24/transactions/deposit/interactive", description: "Interactive deposit",
        params: [
          { key: "asset_code", label: "Asset Code", placeholder: "USDC", required: true },
          { key: "account", label: "Stellar Account", placeholder: "G...", required: true },
        ],
      },
    ],
  },
];

// ─── Hook ─────────────────────────────────────────────────────────────────────
export function useAnchorPlayground() {
  const [domain, setDomain] = useState("testanchor.stellar.org");
  const [activeSEP, setActiveSEP] = useState<SEPProtocol>(SEP_PROTOCOLS[0]);
  const [activeEp, setActiveEp] = useState<Endpoint>(SEP_PROTOCOLS[0].endpoints[0]);
  const [params, setParams] = useState<Record<string, string>>({});
  const [jwt, setJwt] = useState("");
  const [response, setResponse] = useState<ResponseState | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const { copy, isCopied: copied } = useCopyToClipboard();

  const selectSEP = useCallback((sep: SEPProtocol) => {
    setActiveSEP(sep);
    setActiveEp(sep.endpoints[0]);
    setParams({});
    setResponse(null);
    setError(null);
  }, []);

  const selectEp = useCallback((ep: Endpoint) => {
    setActiveEp(ep);
    setParams({});
    setResponse(null);
    setError(null);
  }, []);

  const setParam = useCallback((key: string, value: string) => {
    setParams(prev => ({ ...prev, [key]: value }));
  }, []);

  const buildUrl = useCallback(() => {
    const base = `https://${domain.replace(/^https?:\/\//, "")}${activeEp.path}`;
    if (activeEp.method === "GET") {
      const qs = new URLSearchParams(
        Object.fromEntries(Object.entries(params).filter(([, v]) => v))
      ).toString();
      return qs ? `${base}?${qs}` : base;
    }
    return base;
  }, [domain, activeEp, params]);

  const sendRequest = useCallback(async () => {
    setLoading(true);
    setError(null);
    setResponse(null);
    const url = buildUrl();
    const t0 = performance.now();
    const headers: Record<string, string> = { Accept: "application/json" };
    if (jwt) headers["Authorization"] = `Bearer ${jwt}`;
    let body: string | undefined;
    if (activeEp.method !== "GET") {
      headers["Content-Type"] = "application/json";
      body = JSON.stringify(Object.fromEntries(Object.entries(params).filter(([, v]) => v)));
    }
    try {
      const res = await fetch(url, { method: activeEp.method, headers, body });
      const elapsed = Math.round(performance.now() - t0);
      const ct = res.headers.get("content-type") ?? "";
      const data = ct.includes("json") ? await res.json() : { _raw: await res.text() };
      setResponse({ data, status: res.status, time: elapsed });
      setHistory(h => [{
        id: crypto.randomUUID(),
        time: new Date().toLocaleTimeString(),
        sep: activeSEP.name,
        endpoint: activeEp.path,
        method: activeEp.method,
        status: res.status,
        success: res.ok,
      }, ...h.slice(0, 19)]);
    } catch (err) {
      const elapsed = Math.round(performance.now() - t0);
      const msg = err instanceof Error ? err.message : "Unknown error";
      setError(msg);
      setResponse({ data: { error: msg }, status: 0, time: elapsed });
      setHistory(h => [{
        id: crypto.randomUUID(),
        time: new Date().toLocaleTimeString(),
        sep: activeSEP.name,
        endpoint: activeEp.path,
        method: activeEp.method,
        status: null,
        success: false,
      }, ...h.slice(0, 19)]);
    } finally {
      setLoading(false);
    }
  }, [buildUrl, jwt, activeEp, activeSEP, params]);

  const copyResponse = useCallback(() => {
    if (response) copy(JSON.stringify(response.data, null, 2));
  }, [response, copy]);

  return {
    domain, setDomain,
    activeSEP, selectSEP,
    activeEp, selectEp,
    params, setParam,
    jwt, setJwt,
    response, loading, error,
    history,
    buildUrl,
    sendRequest,
    copyResponse, copied,
  };
}
