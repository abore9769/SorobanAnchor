import { renderHook, act } from "@testing-library/react";
import { useAnchorPlayground, SEP_PROTOCOLS } from "./useAnchorPlayground";

const MOCK_JSON = { assets: [{ code: "USDC" }] };

global.fetch = jest.fn() as jest.Mock;

beforeEach(() => {
  jest.clearAllMocks();
  (global.fetch as jest.Mock).mockResolvedValue({
    ok: true,
    status: 200,
    headers: { get: () => "application/json" },
    json: () => Promise.resolve(MOCK_JSON),
    text: () => Promise.resolve(""),
  });
  Object.defineProperty(global, "performance", {
    value: { now: jest.fn(() => 0) },
    writable: true,
  });
  Object.defineProperty(global, "crypto", {
    value: { randomUUID: () => "uuid-1" },
    writable: true,
  });
});

describe("useAnchorPlayground", () => {
  it("initialises with SEP-1 and its first endpoint", () => {
    const { result } = renderHook(() => useAnchorPlayground());
    expect(result.current.activeSEP.id).toBe("sep1");
    expect(result.current.activeEp.id).toBe("stellar-toml");
    expect(result.current.params).toEqual({});
    expect(result.current.history).toHaveLength(0);
  });

  it("selectSEP switches protocol and resets state", () => {
    const { result } = renderHook(() => useAnchorPlayground());
    act(() => {
      result.current.selectSEP(SEP_PROTOCOLS.find(s => s.id === "sep6")!);
    });
    expect(result.current.activeSEP.id).toBe("sep6");
    expect(result.current.activeEp.id).toBe("sep6-info");
    expect(result.current.params).toEqual({});
    expect(result.current.response).toBeNull();
  });

  it("selectEp switches endpoint and resets params", () => {
    const { result } = renderHook(() => useAnchorPlayground());
    const sep6 = SEP_PROTOCOLS.find(s => s.id === "sep6")!;
    act(() => { result.current.selectSEP(sep6); });
    act(() => { result.current.selectEp(sep6.endpoints[1]); }); // sep6-deposit
    expect(result.current.activeEp.id).toBe("sep6-deposit");
    expect(result.current.params).toEqual({});
  });

  it("setParam updates params", () => {
    const { result } = renderHook(() => useAnchorPlayground());
    act(() => { result.current.setParam("asset_code", "USDC"); });
    expect(result.current.params).toEqual({ asset_code: "USDC" });
  });

  it("buildUrl includes query params for GET endpoints", () => {
    const { result } = renderHook(() => useAnchorPlayground());
    const sep6 = SEP_PROTOCOLS.find(s => s.id === "sep6")!;
    act(() => {
      result.current.selectSEP(sep6);
      result.current.selectEp(sep6.endpoints[1]); // sep6-deposit
      result.current.setParam("asset_code", "USDC");
    });
    expect(result.current.buildUrl()).toContain("asset_code=USDC");
  });

  it("sendRequest calls fetch and populates response", async () => {
    const { result } = renderHook(() => useAnchorPlayground());
    await act(async () => { await result.current.sendRequest(); });
    expect(global.fetch).toHaveBeenCalledWith(
      expect.stringContaining("/.well-known/stellar.toml"),
      expect.objectContaining({ method: "GET" })
    );
    expect(result.current.response).not.toBeNull();
    expect(result.current.response?.status).toBe(200);
    expect(result.current.history).toHaveLength(1);
    expect(result.current.history[0].success).toBe(true);
  });

  it("sendRequest records failed request in history", async () => {
    (global.fetch as jest.Mock).mockRejectedValueOnce(new Error("Network failure"));
    const { result } = renderHook(() => useAnchorPlayground());
    await act(async () => { await result.current.sendRequest(); });
    expect(result.current.error).toBe("Network failure");
    expect(result.current.history).toHaveLength(1);
    expect(result.current.history[0].success).toBe(false);
  });

  it("sendRequest sends Authorization header when jwt is set", async () => {
    const { result } = renderHook(() => useAnchorPlayground());
    act(() => { result.current.setJwt("my.jwt.token"); });
    await act(async () => { await result.current.sendRequest(); });
    expect(global.fetch).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ headers: expect.objectContaining({ Authorization: "Bearer my.jwt.token" }) })
    );
  });
});
