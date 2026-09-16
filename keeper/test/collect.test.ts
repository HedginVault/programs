import { getAssociatedTokenAddressSync, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { PublicKey, SYSVAR_CLOCK_PUBKEY, type AccountInfo } from "@solana/web3.js";
import BN from "bn.js";
import { describe, expect, it, vi } from "vitest";
import { Chain } from "../src/chain";
import { planHoldings, readHoldings, valueVault } from "../src/valuation/index";
import { accountInfo, binArrayAccount, binArrayKey, clockAccount, lbPairAccount, mintAccount, positionAccount, Q64, tokenAccount } from "./fixtures";

type Accounts = Map<string, AccountInfo<Buffer> | null>;

const USDC = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
const SOL = new PublicKey("So11111111111111111111111111111111111111112");
const vault = PublicKey.unique();
const position = PublicKey.unique();
const lbPair = PublicKey.unique();
const jupStrat = PublicKey.unique();
const dlmmStrat = PublicKey.unique();
const idleAta = getAssociatedTokenAddressSync(USDC, vault, true, TOKEN_PROGRAM_ID);
const solAta = getAssociatedTokenAddressSync(SOL, vault, true, TOKEN_PROGRAM_ID);
const account = { depositMint: USDC, nextStrategyId: 2 } as any;
const shares = { 2: Q64.muln(1000) };

async function vaultAccount(chain: Chain, nextStrategyId: number): Promise<AccountInfo<Buffer>> {
  const coder = chain.program.coder.accounts;
  const blank = Buffer.alloc(coder.size("vault"));
  Buffer.from(chain.program.idl.accounts!.find((a) => a.name === "vault")!.discriminator).copy(blank);
  return accountInfo(await coder.encode("vault", { ...coder.decode("vault", blank), nextStrategyId }), chain.programId);
}

/** Every account the valuation should read, as it stands on chain. */
async function world(chain: Chain): Promise<Accounts> {
  const entries: [PublicKey, AccountInfo<Buffer>][] = [
    [SYSVAR_CLOCK_PUBKEY, clockAccount()],
    [vault, await vaultAccount(chain, 2)],
    // strategies are only checked for existence
    [jupStrat, accountInfo(Buffer.alloc(1), chain.programId)],
    [dlmmStrat, accountInfo(Buffer.alloc(1), chain.programId)],
    [idleAta, tokenAccount(USDC, vault, 100_000_000n)],
    [solAta, tokenAccount(SOL, vault, 7n)],
    [position, positionAccount(lbPair, -5, 5, shares)],
    [lbPair, lbPairAccount(SOL, USDC)],
    [binArrayKey(lbPair, -1), binArrayAccount(lbPair, -1, {})],
    [binArrayKey(lbPair, 0), binArrayAccount(lbPair, 0, { 2: { amountX: new BN(1000), amountY: new BN(2000), liquiditySupply: Q64.muln(2000), feeAmountXPerTokenStored: Q64.muln(5) } })],
    [USDC, mintAccount(6)],
    [SOL, mintAccount(9)],
  ];
  return new Map(entries.map(([k, v]) => [k.toBase58(), v]));
}

async function fakeChain() {
  const chain = Chain.create("http://localhost:8899");
  const accounts = await world(chain);
  const read = (keys: PublicKey[]) => keys.map((k) => accounts.get(k.toBase58()) ?? null);
  const fakes = {
    fetchStrategies: vi.fn(async () => [
      { key: jupStrat, account: { id: 0, strategyType: { jupiterSwap: { targetMint: SOL } } } },
      { key: dlmmStrat, account: { id: 1, strategyType: { meteoraDlmm: { position } } } },
    ]),
    // lookup decimals are deliberately wrong: holdings must take decimals from the snapshot
    fetchMintInfos: vi.fn(async (mints: PublicKey[]) => new Map(mints.map((m) => [m.toBase58(), { decimals: 0, tokenProgram: TOKEN_PROGRAM_ID }]))),
    fetchAccountInfos: vi.fn(async (keys: PublicKey[]) => read(keys)),
    fetchSnapshot: vi.fn(async (keys: PublicKey[]) => ({ slot: 42, accounts: new Map(keys.map((k, i) => [k.toBase58(), read(keys)[i]])) })),
  };
  Object.assign(chain, fakes);
  return { chain, fakes, accounts };
}

describe("valueVault", () => {
  it("reads every amount from one snapshot and every price from one call", async () => {
    const { chain, fakes, accounts } = await fakeChain();
    const prices = vi.fn(async (_mints: string[]) => new Map([[USDC.toBase58(), 1], [SOL.toBase58(), 100]]));
    const v = await valueVault({ chain, pricer: { prices } }, vault, account, 9);

    expect(fakes.fetchSnapshot).toHaveBeenCalledTimes(1);
    const keys = fakes.fetchSnapshot.mock.calls[0][0].map((k) => k.toBase58());
    expect(keys).toHaveLength(accounts.size);
    expect(new Set(keys)).toEqual(new Set(accounts.keys()));
    const lookedUp = fakes.fetchAccountInfos.mock.calls.flatMap((c) => c[0].map((k) => k.toBase58()));
    expect(lookedUp).not.toContain(idleAta.toBase58());
    expect(lookedUp).not.toContain(binArrayKey(lbPair, 0).toBase58());

    expect(prices).toHaveBeenCalledTimes(1);
    expect(new Set(prices.mock.calls[0][0])).toEqual(new Set([USDC.toBase58(), SOL.toBase58()]));
    expect(v.slot).toBe(42);
    expect(v.holdings.map((h) => [h.kind, h.amount, h.decimals])).toEqual([
      ["idle", "100000000", 6],
      ["jupiter", "7", 9],
      ["dlmm_x", "500", 9],
      ["dlmm_y", "1000", 6],
      ["dlmm_fee_x", "5000", 9],
      ["dlmm_fee_y", "0", 6],
    ]);
    expect(v.holdings[1].strategy).toBe(jupStrat.toBase58());
    expect(v.holdings[2]).toMatchObject({ strategy: dlmmStrat.toBase58(), account: position.toBase58() });
  });
});

describe("readHoldings", () => {
  async function readWith(change: (chain: Chain, accounts: Accounts) => Promise<unknown> | unknown) {
    const { chain } = await fakeChain();
    const plan = await planHoldings(chain, vault, account);
    const accounts = await world(chain);
    await change(chain, accounts);
    return readHoldings(chain, plan, { slot: 1, accounts });
  }

  it.each([
    ["a strategy was added", async (c: Chain, m: Accounts) => m.set(vault.toBase58(), await vaultAccount(c, 3)), "snapshot_drift:strategy_count"],
    ["the vault is missing", (_: Chain, m: Accounts) => m.set(vault.toBase58(), null), "snapshot_drift:strategy_count"],
    ["a strategy was closed", (_: Chain, m: Accounts) => m.set(jupStrat.toBase58(), null), `snapshot_drift:strategy_closed:${jupStrat.toBase58()}`],
    ["the position was closed", (_: Chain, m: Accounts) => m.set(position.toBase58(), null), `position_missing:${position.toBase58()}`],
    ["the position was resized", (_: Chain, m: Accounts) => m.set(position.toBase58(), positionAccount(lbPair, -5, 6, shares)), `snapshot_drift:position_range:${position.toBase58()}`],
    ["a mint is missing", (_: Chain, m: Accounts) => m.set(SOL.toBase58(), null), `account_missing:${SOL.toBase58()}`],
    ["a bin array with liquidity is missing", (_: Chain, m: Accounts) => m.set(binArrayKey(lbPair, 0).toBase58(), null), `bin_array_missing:${binArrayKey(lbPair, 0).toBase58()}`],
  ])("fails when %s", async (_, change, reason) => {
    await expect(readWith(change)).rejects.toMatchObject({ reason });
  });

  it("reads an unchanged snapshot", async () => {
    await expect(readWith(() => {})).resolves.toMatchObject({ deposit: { mint: USDC.toBase58(), decimals: 6 } });
  });
});

describe("planHoldings", () => {
  it("aborts when an open strategy's position is missing", async () => {
    const { chain, accounts } = await fakeChain();
    accounts.delete(position.toBase58());
    await expect(planHoldings(chain, vault, account)).rejects.toMatchObject({ reason: `position_missing:${position.toBase58()}` });
  });
});
