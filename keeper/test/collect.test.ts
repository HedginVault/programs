import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import BN from "bn.js";
import { describe, expect, it } from "vitest";
import { TOKEN_PROGRAM_ID, type Chain } from "../src/chain";
import type { PositionReader } from "../src/valuation/dlmm";
import { collectHoldings, ValuationError } from "../src/valuation/index";

const USDC = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
const SOL = new PublicKey("So11111111111111111111111111111111111111112");
const vault = PublicKey.unique();
const position = PublicKey.unique();
const jupStrat = PublicKey.unique();
const dlmmStrat = PublicKey.unique();

function fakeChain(balances: Record<string, bigint>): Chain {
  return {
    fetchStrategies: async () => [
      { key: jupStrat, account: { id: 0, strategyType: { jupiterSwap: { targetMint: SOL } } } },
      { key: dlmmStrat, account: { id: 1, strategyType: { meteoraDlmm: { position } } } },
    ],
    fetchMintInfos: async (mints: PublicKey[]) =>
      new Map(mints.map((m) => [m.toBase58(), { decimals: m.equals(USDC) ? 6 : 9, tokenProgram: TOKEN_PROGRAM_ID }])),
    fetchTokenBalances: async (accounts: PublicKey[]) => accounts.map((a) => balances[a.toBase58()] ?? 0n),
  } as unknown as Chain;
}

const positions = (present: boolean): PositionReader => ({
  read: async () =>
    present
      ? new Map([[position.toBase58(), { lbPair: "lb", tokenX: { mint: SOL.toBase58(), decimals: 9 }, tokenY: { mint: USDC.toBase58(), decimals: 6 }, amountX: 10n, amountY: 20n, feeX: 1n, feeY: 2n }]])
      : new Map(),
});

const account = { depositMint: USDC } as any;

describe("collectHoldings", () => {
  it("collects idle, jupiter ata and dlmm position amounts, nothing else", async () => {
    const idleAta = getAssociatedTokenAddressSync(USDC, vault, true, TOKEN_PROGRAM_ID);
    const solAta = getAssociatedTokenAddressSync(SOL, vault, true, TOKEN_PROGRAM_ID);
    const { deposit, holdings } = await collectHoldings(
      fakeChain({ [idleAta.toBase58()]: 100n, [solAta.toBase58()]: 7n }),
      positions(true),
      vault,
      account,
    );
    expect(deposit).toEqual({ mint: USDC.toBase58(), decimals: 6 });
    expect(holdings.map((h) => [h.kind, h.amount, h.mint === USDC.toBase58() ? "USDC" : "SOL"])).toEqual([
      ["idle", 100n, "USDC"],
      ["jupiter", 7n, "SOL"],
      ["dlmm_x", 10n, "SOL"],
      ["dlmm_y", 20n, "USDC"],
      ["dlmm_fee_x", 1n, "SOL"],
      ["dlmm_fee_y", 2n, "USDC"],
    ]);
    expect(holdings[1].strategy).toBe(jupStrat.toBase58());
    expect(holdings[2].strategy).toBe(dlmmStrat.toBase58());
    expect(holdings[2].account).toBe(position.toBase58());
    expect(new BN(0).toNumber()).toBe(0);
  });

  it("aborts when an open strategy's position is missing", async () => {
    await expect(collectHoldings(fakeChain({}), positions(false), vault, account)).rejects.toThrow(ValuationError);
    await expect(collectHoldings(fakeChain({}), positions(false), vault, account)).rejects.toThrow(`position_missing:${position.toBase58()}`);
  });
});
