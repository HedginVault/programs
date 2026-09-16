import * as sdk from "@meteora-ag/dlmm";
import type { Mint } from "@solana/spl-token";
import type { AccountInfo, Connection, PublicKey } from "@solana/web3.js";
import BN from "bn.js";
import { ValuationError } from "./errors";

// The CJS build replaces `module.exports` with the DLMM class and copies the named exports onto
// it, so `.default` may be undefined; accept both shapes.
type Sdk = typeof import("@meteora-ag/dlmm");
const DLMM = ((sdk as unknown as { default?: Sdk["default"] }).default ?? sdk) as unknown as Sdk["default"];

export type DlmmProgram = ReturnType<Sdk["createProgram"]>;
export const createDlmmProgram = (connection: Connection): DlmmProgram => sdk.createProgram(connection);

export interface DlmmPositionAmounts {
  lbPair: string;
  tokenX: { mint: string; decimals: number };
  tokenY: { mint: string; decimals: number };
  amountX: bigint;
  amountY: bigint;
  feeX: bigint;
  feeY: bigint;
}

/** What the lookup learned about a position; the snapshot must still match it. */
export interface PositionPlan {
  position: PublicKey;
  lbPair: PublicKey;
  lowerBinId: number;
  upperBinId: number;
  /** Every bin array the range covers. */
  binArrays: PublicKey[];
  tokenXMint: PublicKey;
  tokenYMint: PublicKey;
}

export type AccountMap = Map<string, AccountInfo<Buffer> | null>;

export function planPosition(program: DlmmProgram, position: PublicKey, info: AccountInfo<Buffer>): Omit<PositionPlan, "tokenXMint" | "tokenYMint"> {
  const p = sdk.wrapPosition(program, position, info);
  return { position, lbPair: p.lbPair(), lowerBinId: p.lowerBinId().toNumber(), upperBinId: p.upperBinId().toNumber(), binArrays: p.getBinArrayKeysCoverage(program.programId) };
}

export function decodeLbPairMints(program: DlmmProgram, info: AccountInfo<Buffer>): { tokenXMint: PublicKey; tokenYMint: PublicKey } {
  const { tokenXMint, tokenYMint } = sdk.decodeAccount<sdk.LbPair>(program, "lbPair", info.data);
  return { tokenXMint, tokenYMint };
}

export const decodeClock = (info: AccountInfo<Buffer>): sdk.Clock => sdk.ClockLayout.decode(info.data);

type ProcessPosition = (
  program: DlmmProgram,
  lbPair: sdk.LbPair,
  clock: sdk.Clock,
  position: sdk.IPosition,
  baseMint: Mint,
  quoteMint: Mint,
  rewardMint0: Mint | undefined,
  rewardMint1: Mint | undefined,
  binArrayMap: Map<string, sdk.BinArray>,
) => Promise<sdk.PositionData | null>;
// Private in the SDK typings; it is the math `DLMM.getPosition` runs once its own fetches are done.
const processPosition = (DLMM as unknown as { processPosition: ProcessPosition }).processPosition.bind(DLMM);

/** Amounts for one position from snapshot accounts, after checking the position still matches its plan. */
export async function readPosition(program: DlmmProgram, plan: PositionPlan, accounts: AccountMap, clock: sdk.Clock, mintX: Mint, mintY: Mint): Promise<DlmmPositionAmounts> {
  const key = plan.position.toBase58();
  const info = accounts.get(key);
  if (!info) throw new ValuationError(`position_missing:${key}`);
  const position = sdk.wrapPosition(program, plan.position, info);
  if (!position.lbPair().equals(plan.lbPair) || position.lowerBinId().toNumber() !== plan.lowerBinId || position.upperBinId().toNumber() !== plan.upperBinId) {
    throw new ValuationError(`snapshot_drift:position_range:${key}`);
  }
  const lbPairInfo = accounts.get(plan.lbPair.toBase58());
  if (!lbPairInfo) throw new ValuationError(`account_missing:${plan.lbPair.toBase58()}`);
  const lbPair = sdk.decodeAccount<sdk.LbPair>(program, "lbPair", lbPairInfo.data);

  const binArrays = new Map<string, sdk.BinArray>();
  for (const k of plan.binArrays) {
    const binArray = accounts.get(k.toBase58());
    if (binArray) binArrays.set(k.toBase58(), sdk.decodeAccount<sdk.BinArray>(program, "binArray", binArray.data));
  }
  // The SDK reads an absent bin array as empty bins, which would value the liquidity in it at zero.
  position.liquidityShares().forEach((share, i) => {
    if (share.isZero()) return;
    const index = sdk.binIdToBinArrayIndex(new BN(plan.lowerBinId + i));
    const binArray = sdk.deriveBinArray(plan.lbPair, index, program.programId)[0].toBase58();
    if (!binArrays.has(binArray)) throw new ValuationError(`bin_array_missing:${binArray}`);
  });

  const data = await processPosition(program, lbPair, clock, position, mintX, mintY, undefined, undefined, binArrays);
  const amount = (v: { toString(): string } | undefined) => BigInt(v?.toString() ?? "0");
  return {
    lbPair: plan.lbPair.toBase58(),
    tokenX: { mint: plan.tokenXMint.toBase58(), decimals: mintX.decimals },
    tokenY: { mint: plan.tokenYMint.toBase58(), decimals: mintY.decimals },
    amountX: amount(data?.totalXAmountExcludeTransferFee),
    amountY: amount(data?.totalYAmountExcludeTransferFee),
    feeX: amount(data?.feeXExcludeTransferFee),
    feeY: amount(data?.feeYExcludeTransferFee),
  };
}
