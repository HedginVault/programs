/** USD price per whole token for each mint. Mints Jupiter does not know are absent from the map. */
export interface Pricer {
  prices(mints: string[]): Promise<Map<string, number>>;
}

const MAX_IDS = 50; // Jupiter /price/v3 cap

interface JupiterPricerOptions {
  host: string;
  apiKey?: string;
  ttlMs?: number;
  fetchFn?: typeof fetch;
  now?: () => number;
}

/**
 * Jupiter price API v3. This is the only pricing source today; a future engine (Rust service,
 * on-chain quotes, TWAP) plugs in by implementing `Pricer` and nothing else changes.
 */
export class JupiterPricer implements Pricer {
  private readonly cache = new Map<string, { price: number | null; expires: number }>();
  private readonly host: string;
  private readonly apiKey?: string;
  private readonly ttlMs: number;
  private readonly fetchFn: typeof fetch;
  private readonly now: () => number;

  constructor(opts: JupiterPricerOptions) {
    this.host = opts.host.replace(/\/$/, "");
    this.apiKey = opts.apiKey;
    this.ttlMs = opts.ttlMs ?? 60_000;
    this.fetchFn = opts.fetchFn ?? fetch;
    this.now = opts.now ?? Date.now;
  }

  async prices(mints: string[]): Promise<Map<string, number>> {
    const out = new Map<string, number>();
    const missing: string[] = [];
    const now = this.now();
    for (const mint of new Set(mints)) {
      const hit = this.cache.get(mint);
      if (!hit || hit.expires <= now) missing.push(mint);
      else if (hit.price !== null) out.set(mint, hit.price);
    }
    for (let i = 0; i < missing.length; i += MAX_IDS) {
      const chunk = missing.slice(i, i + MAX_IDS);
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
        const price = body[mint]?.usdPrice ?? null;
        this.cache.set(mint, { price, expires: now + this.ttlMs });
        if (price !== null && Number.isFinite(price) && price > 0) out.set(mint, price);
      }
    }
    return out;
  }
}
