import { describe, expect, it, vi } from "vitest";
import { JupiterPricer } from "../src/valuation/pricer";

const A = "A".repeat(32), B = "B".repeat(32);

function fakeFetch(body: Record<string, { usdPrice: number } | null>, ok = true) {
  return vi.fn(async () => ({ ok, status: ok ? 200 : 500, json: async () => body })) as unknown as typeof fetch;
}

describe("JupiterPricer", () => {
  it("fetches prices in one call and caches them", async () => {
    const fetchFn = fakeFetch({ [A]: { usdPrice: 1.5 }, [B]: null });
    let t = 0;
    const p = new JupiterPricer({ host: "https://j", fetchFn, now: () => t, ttlMs: 1000 });
    const first = await p.prices([A, B, A]);
    expect(first.get(A)).toBe(1.5);
    expect(first.has(B)).toBe(false);
    expect(fetchFn).toHaveBeenCalledTimes(1);
    expect((fetchFn as any).mock.calls[0][0]).toBe(`https://j/price/v3?ids=${A},${B}`);
    await p.prices([A, B]);
    expect(fetchFn).toHaveBeenCalledTimes(1); // cached, including the null
    t = 2000;
    await p.prices([A]);
    expect(fetchFn).toHaveBeenCalledTimes(2); // expired
  });

  it("chunks requests by 50", async () => {
    const mints = Array.from({ length: 120 }, (_, i) => String(i).padStart(32, "m"));
    const fetchFn = fakeFetch({});
    await new JupiterPricer({ host: "https://j", fetchFn }).prices(mints);
    expect(fetchFn).toHaveBeenCalledTimes(3);
  });

  it("sends the api key header and leaves mints unpriced on failure", async () => {
    const fetchFn = fakeFetch({}, false);
    const out = await new JupiterPricer({ host: "https://j", apiKey: "k", fetchFn }).prices([A]);
    expect(out.size).toBe(0);
    expect((fetchFn as any).mock.calls[0][1].headers["x-api-key"]).toBe("k");
  });
});
