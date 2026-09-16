import { unpackMint } from "@solana/spl-token";
import { PublicKey, type AccountInfo } from "@solana/web3.js";
import BN from "bn.js";
import { describe, expect, it } from "vitest";
import { decodeClock, decodeLbPairMints, planPosition, readPosition, type PositionPlan } from "../src/valuation/dlmm";
import { binArrayAccount, binArrayKey, clockAccount, dlmmProgram, lbPairAccount, mintAccount, positionAccount, Q64 } from "./fixtures";

const lbPair = PublicKey.unique();
const position = PublicKey.unique();
const mintX = PublicKey.unique();
const mintY = PublicKey.unique();
// 1000 of the 2000 shares in bin 2, which holds 1000 X / 2000 Y and has accrued 5 X per share; 7 X already pending
const positionInfo = positionAccount(lbPair, -5, 5, { 2: Q64.muln(1000) }, { 2: new BN(7) });

function accounts(overrides: [PublicKey, AccountInfo<Buffer> | null][] = []) {
  const m = new Map<string, AccountInfo<Buffer> | null>([
    [position.toBase58(), positionInfo],
    [lbPair.toBase58(), lbPairAccount(mintX, mintY)],
    [binArrayKey(lbPair, -1).toBase58(), binArrayAccount(lbPair, -1, {})],
    [binArrayKey(lbPair, 0).toBase58(), binArrayAccount(lbPair, 0, { 2: { amountX: new BN(1000), amountY: new BN(2000), liquiditySupply: Q64.muln(2000), feeAmountXPerTokenStored: Q64.muln(5) } })],
  ]);
  for (const [k, v] of overrides) m.set(k.toBase58(), v);
  return m;
}

const plan = (): PositionPlan => ({ ...planPosition(dlmmProgram, position, positionInfo), ...decodeLbPairMints(dlmmProgram, lbPairAccount(mintX, mintY)) });
const mint = (key: PublicKey, decimals: number) => unpackMint(key, mintAccount(decimals), mintAccount(decimals).owner);
const read = (m: Map<string, AccountInfo<Buffer> | null>) => readPosition(dlmmProgram, plan(), m, decodeClock(clockAccount()), mint(mintX, 9), mint(mintY, 6));

describe("planPosition", () => {
  it("learns the pair, range, covered bin arrays and pool mints", () => {
    const p = plan();
    expect(p.lbPair.equals(lbPair)).toBe(true);
    expect([p.lowerBinId, p.upperBinId]).toEqual([-5, 5]);
    expect(p.binArrays.map((k) => k.toBase58())).toEqual([binArrayKey(lbPair, -1), binArrayKey(lbPair, 0)].map((k) => k.toBase58()));
    expect(p.tokenXMint.equals(mintX) && p.tokenYMint.equals(mintY)).toBe(true);
  });
});

describe("readPosition", () => {
  it("computes amounts and pending fees from snapshot accounts only", async () => {
    expect(await read(accounts())).toEqual({
      lbPair: lbPair.toBase58(),
      tokenX: { mint: mintX.toBase58(), decimals: 9 },
      tokenY: { mint: mintY.toBase58(), decimals: 6 },
      amountX: 500n,
      amountY: 1000n,
      feeX: 5007n,
      feeY: 0n,
    });
  });

  it("tolerates an absent bin array that holds none of the position's liquidity", async () => {
    await expect(read(accounts([[binArrayKey(lbPair, -1), null]]))).resolves.toMatchObject({ amountX: 500n });
  });

  it.each([
    ["the position is closed", () => accounts([[position, null]]), `position_missing:${position.toBase58()}`],
    ["the range changed", () => accounts([[position, positionAccount(lbPair, -5, 6, { 2: Q64.muln(1000) })]]), `snapshot_drift:position_range:${position.toBase58()}`],
    ["the pair is missing", () => accounts([[lbPair, null]]), `account_missing:${lbPair.toBase58()}`],
    ["a bin array holding liquidity is missing", () => accounts([[binArrayKey(lbPair, 0), null]]), `bin_array_missing:${binArrayKey(lbPair, 0).toBase58()}`],
  ])("fails when %s", async (_, snapshot, reason) => {
    await expect(read(snapshot())).rejects.toMatchObject({ reason });
  });
});
