import { getAssociatedTokenAddressSync, unpackMint, type Mint } from "@solana/spl-token";
import { SYSVAR_CLOCK_PUBKEY, type PublicKey } from "@solana/web3.js";
import { decodeTokenAmount, type Chain, type Snapshot, type VaultAccount } from "../chain";
import { decodeClock, decodeLbPairMints, planPosition, readPosition, type PositionPlan } from "./dlmm";
import { ValuationError } from "./errors";
import type { Pricer } from "./pricer";

export { ValuationError } from "./errors";

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
  /** Slot the account snapshot was read at. */
  slot: number;
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
export function computeValuation({ vault, epoch, deposit, holdings, prices }: ComputeArgs): Omit<Valuation, "slot"> {
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

export interface HoldingsPlan {
  vault: PublicKey;
  /** `next_strategy_id` from a vault read taken before the strategy lookup. */
  nextStrategyId: number;
  strategies: PublicKey[];
  depositMint: PublicKey;
  idleAta: PublicKey;
  jupiter: { strategy: PublicKey; mint: PublicKey; ata: PublicKey }[];
  dlmm: { strategy: PublicKey; position: PositionPlan }[];
  /** Every account `readHoldings` decodes, deduplicated, fetched as one snapshot. */
  keys: PublicKey[];
}

const dedupe = (keys: PublicKey[]) => [...new Map(keys.map((k) => [k.toBase58(), k])).values()];

/**
 * Lookup: which accounts hold the vault's assets. Amounts read here are never used. Assets are the idle
 * deposit-mint ATA, the vault's ATA for each Jupiter strategy's target mint, and the position named by
 * each DLMM strategy. Token accounts the vault merely owns are never counted, so out-of-band transfers
 * cannot move NAV. `account` must have been read before this call.
 */
export async function planHoldings(chain: Chain, vault: PublicKey, account: VaultAccount): Promise<HoldingsPlan> {
  const strategies = await chain.fetchStrategies(vault);
  const jupiterStrategies = strategies.flatMap((s) =>
    "jupiterSwap" in s.account.strategyType
      ? [{ strategy: s.key, targetMint: (s.account.strategyType as { jupiterSwap: { targetMint: PublicKey } }).jupiterSwap.targetMint }]
      : [],
  );
  const dlmmStrategies = strategies.flatMap((s) =>
    "meteoraDlmm" in s.account.strategyType
      ? [{ strategy: s.key, position: (s.account.strategyType as { meteoraDlmm: { position: PublicKey } }).meteoraDlmm.position }]
      : [],
  );

  const [mintInfos, positionInfos] = await Promise.all([
    chain.fetchMintInfos([account.depositMint, ...jupiterStrategies.map((j) => j.targetMint)]),
    chain.fetchAccountInfos(dlmmStrategies.map((d) => d.position)),
  ]);
  const positions = dlmmStrategies.map((d, i) => {
    const info = positionInfos[i];
    if (!info) throw new ValuationError(`position_missing:${d.position.toBase58()}`);
    return { strategy: d.strategy, position: planPosition(chain.dlmmProgram, d.position, info) };
  });
  const lbPairs = dedupe(positions.map((p) => p.position.lbPair));
  const lbPairInfos = await chain.fetchAccountInfos(lbPairs);
  const pairMints = new Map(
    lbPairs.map((k, i) => {
      const info = lbPairInfos[i];
      if (!info) throw new ValuationError(`account_missing:${k.toBase58()}`);
      return [k.toBase58(), decodeLbPairMints(chain.dlmmProgram, info)];
    }),
  );

  const ata = (mint: PublicKey) => getAssociatedTokenAddressSync(mint, vault, true, mintInfos.get(mint.toBase58())!.tokenProgram);
  const idleAta = ata(account.depositMint);
  const jupiter = jupiterStrategies.map((j) => ({ strategy: j.strategy, mint: j.targetMint, ata: ata(j.targetMint) }));
  const dlmm = positions.map((p) => ({ strategy: p.strategy, position: { ...p.position, ...pairMints.get(p.position.lbPair.toBase58())! } }));
  const keys = dedupe([
    SYSVAR_CLOCK_PUBKEY,
    vault,
    ...strategies.map((s) => s.key),
    idleAta,
    ...jupiter.map((j) => j.ata),
    ...dlmm.flatMap(({ position: p }) => [p.position, p.lbPair, ...p.binArrays, p.tokenXMint, p.tokenYMint]),
    account.depositMint,
    ...jupiter.map((j) => j.mint),
  ]);
  return { vault, nextStrategyId: account.nextStrategyId, strategies: strategies.map((s) => s.key), depositMint: account.depositMint, idleAta, jupiter, dlmm, keys };
}

/** Verify + decode: every amount comes from `snapshot`, which must still match `plan`. No I/O. */
export async function readHoldings(chain: Chain, plan: HoldingsPlan, snapshot: Snapshot): Promise<{ deposit: { mint: string; decimals: number }; holdings: RawHolding[] }> {
  const get = (key: PublicKey) => snapshot.accounts.get(key.toBase58()) ?? null;

  const vaultInfo = get(plan.vault);
  const vault = vaultInfo && chain.program.coder.accounts.decode<VaultAccount>("vault", vaultInfo.data);
  if (!vault || vault.nextStrategyId !== plan.nextStrategyId) throw new ValuationError("snapshot_drift:strategy_count");
  for (const s of plan.strategies) {
    if (!get(s)) throw new ValuationError(`snapshot_drift:strategy_closed:${s.toBase58()}`);
  }

  const required = (key: PublicKey) => {
    const info = get(key);
    if (!info) throw new ValuationError(`account_missing:${key.toBase58()}`);
    return info;
  };
  const mints = new Map<string, Mint>();
  const mint = (key: PublicKey): Mint => {
    let m = mints.get(key.toBase58());
    if (!m) {
      const info = required(key);
      m = unpackMint(key, info, info.owner);
      mints.set(key.toBase58(), m);
    }
    return m;
  };
  const clock = decodeClock(required(SYSVAR_CLOCK_PUBKEY));

  const depositMint = plan.depositMint.toBase58();
  const depositDecimals = mint(plan.depositMint).decimals;
  const holdings: RawHolding[] = [
    { kind: "idle", strategy: null, account: plan.idleAta.toBase58(), mint: depositMint, decimals: depositDecimals, amount: decodeTokenAmount(get(plan.idleAta)) },
    ...plan.jupiter.map((j): RawHolding => ({
      kind: "jupiter",
      strategy: j.strategy.toBase58(),
      account: j.ata.toBase58(),
      mint: j.mint.toBase58(),
      decimals: mint(j.mint).decimals,
      amount: decodeTokenAmount(get(j.ata)),
    })),
  ];
  for (const { strategy, position } of plan.dlmm) {
    const p = await readPosition(chain.dlmmProgram, position, snapshot.accounts, clock, mint(position.tokenXMint), mint(position.tokenYMint));
    const base = { strategy: strategy.toBase58(), account: position.position.toBase58() };
    holdings.push(
      { ...base, kind: "dlmm_x", mint: p.tokenX.mint, decimals: p.tokenX.decimals, amount: p.amountX },
      { ...base, kind: "dlmm_y", mint: p.tokenY.mint, decimals: p.tokenY.decimals, amount: p.amountY },
      { ...base, kind: "dlmm_fee_x", mint: p.tokenX.mint, decimals: p.tokenX.decimals, amount: p.feeX },
      { ...base, kind: "dlmm_fee_y", mint: p.tokenY.mint, decimals: p.tokenY.decimals, amount: p.feeY },
    );
  }
  return { deposit: { mint: depositMint, decimals: depositDecimals }, holdings };
}

export interface ValuationDeps {
  chain: Chain;
  pricer: Pricer;
}

/** Lookup → one account snapshot → one price call → compute. */
export async function valueVault(deps: ValuationDeps, vault: PublicKey, account: VaultAccount, epoch: number): Promise<Valuation> {
  const plan = await planHoldings(deps.chain, vault, account);
  const snapshot = await deps.chain.fetchSnapshot(plan.keys);
  const { deposit, holdings } = await readHoldings(deps.chain, plan, snapshot);
  const prices = await deps.pricer.prices([...new Set([deposit.mint, ...holdings.map((h) => h.mint)])]);
  return { ...computeValuation({ vault: vault.toBase58(), epoch, deposit, holdings, prices }), slot: snapshot.slot };
}
