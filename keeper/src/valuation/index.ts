import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import type { Chain, VaultAccount } from "../chain";
import type { PositionReader } from "./dlmm";
import type { Pricer } from "./pricer";

export type HoldingKind = "idle" | "jupiter" | "dlmm_x" | "dlmm_y" | "dlmm_fee_x" | "dlmm_fee_y";

export interface RawHolding {
  kind: HoldingKind;
  /** Strategy PDA, null for the idle balance. */
  strategy: string | null;
  /** Token account or DLMM position the amount was read from. */
  account: string;
  mint: string;
  decimals: number;
  amount: bigint;
}

export interface Holding extends Omit<RawHolding, "amount"> {
  amount: string;
  priceUsd: number;
  /** Deposit-mint base units. */
  value: string;
}

export interface Valuation {
  vault: string;
  epoch: number;
  totalAssets: bigint;
  idleBalance: bigint;
  depositPriceUsd: number;
  holdings: Holding[];
}

export class ValuationError extends Error {
  constructor(readonly reason: string) {
    super(reason);
    this.name = "ValuationError";
  }
}

/** Pending DLMM fees count net of the 10 % treasury cut taken on claim. */
export const FEE_RETAINED_BPS = 9000n;
const MAX_BPS = 10_000n;
const PRICE_SCALE = 1_000_000_000n;
const U64_MAX = 2n ** 64n - 1n;

const scalePrice = (price: number) => BigInt(Math.round(price * Number(PRICE_SCALE)));
const isFee = (kind: HoldingKind) => kind === "dlmm_fee_x" || kind === "dlmm_fee_y";

export interface ComputeArgs {
  vault: string;
  epoch: number;
  deposit: { mint: string; decimals: number };
  holdings: RawHolding[];
  prices: Map<string, number>;
}

/**
 * value = amount × 10^(depositDecimals − decimals) × price(mint) / price(deposit), fees × 0.9,
 * all in bigint with prices at 1e9 fixed point, rounded down. Pure: no I/O.
 */
export function computeValuation({ vault, epoch, deposit, holdings, prices }: ComputeArgs): Valuation {
  const needsConversion = holdings.some((h) => h.mint !== deposit.mint);
  const depositPriceUsd = prices.get(deposit.mint) ?? 0;
  if (needsConversion && depositPriceUsd <= 0) throw new ValuationError(`missing_price:${deposit.mint}`);
  const depositScaled = scalePrice(depositPriceUsd);

  let total = 0n;
  let idleBalance = 0n;
  const out: Holding[] = holdings.map((h) => {
    let value: bigint;
    let priceUsd: number;
    if (h.mint === deposit.mint) {
      priceUsd = depositPriceUsd;
      value = h.amount;
    } else {
      priceUsd = prices.get(h.mint) ?? 0;
      if (priceUsd <= 0) throw new ValuationError(`missing_price:${h.mint}`);
      const num = h.amount * scalePrice(priceUsd) * 10n ** BigInt(deposit.decimals);
      const den = depositScaled * 10n ** BigInt(h.decimals);
      value = num / den;
    }
    if (isFee(h.kind)) value = (value * FEE_RETAINED_BPS) / MAX_BPS;
    if (h.kind === "idle") idleBalance += h.amount;
    total += value;
    return { ...h, amount: h.amount.toString(), priceUsd, value: value.toString() };
  });

  if (total > U64_MAX) throw new ValuationError("total_overflow");
  return { vault, epoch, totalAssets: total, idleBalance, depositPriceUsd, holdings: out };
}

/**
 * Enumerates what counts as a vault asset: the idle deposit-mint ATA, the vault's ATA for each
 * Jupiter strategy's target mint, and the position named by each DLMM strategy. Token accounts the
 * vault merely owns are never counted, so out-of-band transfers cannot move NAV.
 */
export async function collectHoldings(
  chain: Chain,
  positions: PositionReader,
  vault: PublicKey,
  account: VaultAccount,
): Promise<{ deposit: { mint: string; decimals: number }; holdings: RawHolding[] }> {
  const strategies = await chain.fetchStrategies(vault);
  const jupiter = strategies.flatMap((s) =>
    "jupiterSwap" in s.account.strategyType
      ? [{ strategy: s.key, targetMint: (s.account.strategyType as { jupiterSwap: { targetMint: PublicKey } }).jupiterSwap.targetMint }]
      : [],
  );
  const dlmm = strategies.flatMap((s) =>
    "meteoraDlmm" in s.account.strategyType
      ? [{ strategy: s.key, position: (s.account.strategyType as { meteoraDlmm: { position: PublicKey } }).meteoraDlmm.position }]
      : [],
  );

  const mintInfos = await chain.fetchMintInfos([account.depositMint, ...jupiter.map((j) => j.targetMint)]);
  const info = (mint: PublicKey) => mintInfos.get(mint.toBase58())!;
  const ata = (mint: PublicKey) => getAssociatedTokenAddressSync(mint, vault, true, info(mint).tokenProgram);

  const accounts = [ata(account.depositMint), ...jupiter.map((j) => ata(j.targetMint))];
  const [balances, positionAmounts] = await Promise.all([
    chain.fetchTokenBalances(accounts),
    positions.read(dlmm.map((d) => d.position)),
  ]);

  const depositMint = account.depositMint.toBase58();
  const holdings: RawHolding[] = [
    { kind: "idle", strategy: null, account: accounts[0].toBase58(), mint: depositMint, decimals: info(account.depositMint).decimals, amount: balances[0] },
  ];
  jupiter.forEach((j, i) => {
    holdings.push({
      kind: "jupiter",
      strategy: j.strategy.toBase58(),
      account: accounts[i + 1].toBase58(),
      mint: j.targetMint.toBase58(),
      decimals: info(j.targetMint).decimals,
      amount: balances[i + 1],
    });
  });
  for (const d of dlmm) {
    const key = d.position.toBase58();
    const p = positionAmounts.get(key);
    if (!p) throw new ValuationError(`position_missing:${key}`);
    const strategy = d.strategy.toBase58();
    holdings.push(
      { kind: "dlmm_x", strategy, account: key, mint: p.tokenX.mint, decimals: p.tokenX.decimals, amount: p.amountX },
      { kind: "dlmm_y", strategy, account: key, mint: p.tokenY.mint, decimals: p.tokenY.decimals, amount: p.amountY },
      { kind: "dlmm_fee_x", strategy, account: key, mint: p.tokenX.mint, decimals: p.tokenX.decimals, amount: p.feeX },
      { kind: "dlmm_fee_y", strategy, account: key, mint: p.tokenY.mint, decimals: p.tokenY.decimals, amount: p.feeY },
    );
  }
  return { deposit: { mint: depositMint, decimals: info(account.depositMint).decimals }, holdings };
}

export interface ValuationDeps {
  chain: Chain;
  positions: PositionReader;
  pricer: Pricer;
}

export async function valueVault(deps: ValuationDeps, vault: PublicKey, account: VaultAccount, epoch: number): Promise<Valuation> {
  const { deposit, holdings } = await collectHoldings(deps.chain, deps.positions, vault, account);
  const mints = [...new Set([deposit.mint, ...holdings.map((h) => h.mint)])];
  const prices = await deps.pricer.prices(mints);
  return computeValuation({ vault: vault.toBase58(), epoch, deposit, holdings, prices });
}
