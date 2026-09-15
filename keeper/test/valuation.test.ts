import { describe, expect, it } from "vitest";
import { computeValuation, ValuationError, type RawHolding } from "../src/valuation/index";

const USDC = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const SOL = "So11111111111111111111111111111111111111112";
const deposit = { mint: USDC, decimals: 6 };
const idle = (amount: bigint): RawHolding => ({ kind: "idle", strategy: null, account: "ata", mint: USDC, decimals: 6, amount });
const sol = (kind: RawHolding["kind"], amount: bigint): RawHolding => ({ kind, strategy: "strat", account: "acc", mint: SOL, decimals: 9, amount });

describe("computeValuation", () => {
  it("values idle only without needing any price", () => {
    const v = computeValuation({ vault: "v", epoch: 1, deposit, holdings: [idle(5_000_000n)], prices: new Map() });
    expect(v.totalAssets).toBe(5_000_000n);
    expect(v.idleBalance).toBe(5_000_000n);
    expect(v.holdings[0].value).toBe("5000000");
    expect(v.holdings[0].priceUsd).toBe(0);
  });

  it("converts a Jupiter holding across decimals", () => {
    // 2 SOL at $150 with USDC at $1 = 300 USDC = 300_000_000 base units
    const prices = new Map([[SOL, 150], [USDC, 1]]);
    const v = computeValuation({ vault: "v", epoch: 1, deposit, holdings: [idle(0n), sol("jupiter", 2_000_000_000n)], prices });
    expect(v.totalAssets).toBe(300_000_000n);
    expect(v.depositPriceUsd).toBe(1);
    expect(v.holdings[1].priceUsd).toBe(150);
  });

  it("uses the deposit price as denominator", () => {
    // USDC at $0.5 doubles the USDC-denominated value
    const prices = new Map([[SOL, 150], [USDC, 0.5]]);
    const v = computeValuation({ vault: "v", epoch: 1, deposit, holdings: [sol("jupiter", 1_000_000_000n)], prices });
    expect(v.totalAssets).toBe(300_000_000n);
  });

  it("counts DLMM fees at 90 percent and amounts in full", () => {
    const prices = new Map([[SOL, 100], [USDC, 1]]);
    const holdings: RawHolding[] = [
      sol("dlmm_x", 1_000_000_000n),                                                     // 100 USDC
      { kind: "dlmm_y", strategy: "s", account: "p", mint: USDC, decimals: 6, amount: 50_000_000n }, // 50 USDC
      sol("dlmm_fee_x", 100_000_000n),                                                   // 10 USDC → 9
      { kind: "dlmm_fee_y", strategy: "s", account: "p", mint: USDC, decimals: 6, amount: 10_000_000n }, // 10 → 9
    ];
    const v = computeValuation({ vault: "v", epoch: 1, deposit, holdings, prices });
    expect(v.totalAssets).toBe(168_000_000n);
    expect(v.idleBalance).toBe(0n);
  });

  it("aborts on a missing price for a counted mint", () => {
    expect(() => computeValuation({ vault: "v", epoch: 1, deposit, holdings: [sol("jupiter", 1n)], prices: new Map([[USDC, 1]]) }))
      .toThrow(ValuationError);
    try {
      computeValuation({ vault: "v", epoch: 1, deposit, holdings: [sol("jupiter", 1n)], prices: new Map([[USDC, 1]]) });
    } catch (e) {
      expect((e as ValuationError).reason).toBe(`missing_price:${SOL}`);
    }
  });

  it("aborts on a missing deposit price only when a conversion is needed", () => {
    expect(() => computeValuation({ vault: "v", epoch: 1, deposit, holdings: [sol("jupiter", 1n)], prices: new Map([[SOL, 1]]) }))
      .toThrow(`missing_price:${USDC}`);
    expect(() => computeValuation({ vault: "v", epoch: 1, deposit, holdings: [idle(1n)], prices: new Map() })).not.toThrow();
  });

  it("rounds down and rejects totals above u64", () => {
    const prices = new Map([[SOL, 1.000000001], [USDC, 3]]);
    const v = computeValuation({ vault: "v", epoch: 1, deposit, holdings: [sol("jupiter", 1n)], prices });
    expect(v.totalAssets).toBe(0n);
    const huge = idle(2n ** 64n);
    expect(() => computeValuation({ vault: "v", epoch: 1, deposit, holdings: [huge], prices })).toThrow("total_overflow");
  });
});
