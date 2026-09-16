/** USD price per whole token for each mint. Mints Jupiter does not know are absent from the map. */
export interface Pricer {
  prices(mints: string[]): Promise<Map<string, number>>;
}

const MAX_IDS = 50; // Jupiter /price/v3 cap

interface JupiterPricerOptions {
  host: string;
  apiKey?: string;
  fetchFn?: typeof fetch;
}

/**
 * Jupiter price API v3. This is the only pricing source today; a future engine (Rust service,
 * on-chain quotes, TWAP) plugs in by implementing `Pricer` and nothing else changes. Uncached, so
 * every price of one valuation comes from the same response.
 */
export class JupiterPricer implements Pricer {
  private readonly host: string;
  private readonly apiKey?: string;
  private readonly fetchFn: typeof fetch;

  constructor(opts: JupiterPricerOptions) {
    this.host = opts.host.replace(/\/$/, "");
    this.apiKey = opts.apiKey;
    this.fetchFn = opts.fetchFn ?? fetch;
  }

  async prices(mints: string[]): Promise<Map<string, number>> {
    const out = new Map<string, number>();
    const unique = [...new Set(mints)];
    for (let i = 0; i < unique.length; i += MAX_IDS) {
      const chunk = unique.slice(i, i + MAX_IDS);
      let body: Record<string, { usdPrice: number } | null>;
      try {
        const res = await this.fetchFn(`${this.host}/price/v3?ids=${chunk.join(",")}`, {
          headers: { "Content-Type": "application/json", ...(this.apiKey ? { "x-api-key": this.apiKey } : {}) },
        });
        if (!res.ok) throw new Error(`Jupiter price ${res.status}`);
        body = (await res.json()) as Record<string, { usdPrice: number } | null>;
      } catch {
        continue; // leave the chunk unpriced; the valuation decides whether that is fatal
      }
      for (const mint of chunk) {
        const price = body[mint]?.usdPrice;
        if (typeof price === "number" && Number.isFinite(price) && price > 0) out.set(mint, price);
      }
    }
    return out;
  }
}
