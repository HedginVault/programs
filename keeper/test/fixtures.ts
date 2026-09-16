import * as sdk from "@meteora-ag/dlmm";
import { AccountLayout, MintLayout, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { Connection, PublicKey, type AccountInfo } from "@solana/web3.js";
import BN from "bn.js";
import { createDlmmProgram } from "../src/valuation/dlmm";

export const dlmmProgram = createDlmmProgram(new Connection("http://localhost:8899"));
/** Liquidity shares and fee-per-token values are Q64.64. */
export const Q64 = new BN(1).shln(64);
const BINS_PER_ARRAY = 70;

type Layout = { layout: { encode(value: unknown, data: Buffer, offset: number): number } };
const coder = dlmmProgram.coder.accounts as unknown as { size(name: string): number; decode(name: string, data: Buffer): any; accountLayouts: Map<string, Layout> };

export const accountInfo = (data: Buffer, owner: PublicKey): AccountInfo<Buffer> => ({ data, owner, lamports: 1, executable: false });

const withDiscriminator = (name: string) => {
  const data = Buffer.alloc(coder.size(name));
  Buffer.from(sdk.getAccountDiscriminator(name as sdk.AccountName)).copy(data);
  return data;
};
const blank = (name: string) => coder.decode(name, withDiscriminator(name));

// `coder.encode` writes into a fixed 1000-byte buffer; positions and bin arrays are larger.
function encode(name: string, value: unknown): AccountInfo<Buffer> {
  const data = withDiscriminator(name);
  coder.accountLayouts.get(name)!.layout.encode(value, data, 8);
  return accountInfo(data, dlmmProgram.programId);
}

export const lbPairAccount = (tokenXMint: PublicKey, tokenYMint: PublicKey) => encode("lbPair", { ...blank("lbPair"), tokenXMint, tokenYMint, binStep: 10 });

/** `shares` and `feeXPending` are keyed by bin id. */
export function positionAccount(lbPair: PublicKey, lowerBinId: number, upperBinId: number, shares: Record<number, BN>, feeXPending: Record<number, BN> = {}) {
  const p = blank("positionV2");
  Object.assign(p, { lbPair, lowerBinId, upperBinId });
  for (const [binId, share] of Object.entries(shares)) p.liquidityShares[Number(binId) - lowerBinId] = share;
  for (const [binId, fee] of Object.entries(feeXPending)) p.feeInfos[Number(binId) - lowerBinId].feeXPending = fee;
  return encode("positionV2", p);
}

export interface BinFixture {
  amountX?: BN;
  amountY?: BN;
  liquiditySupply?: BN;
  feeAmountXPerTokenStored?: BN;
}

/** `bins` is keyed by bin id; each id must fall inside bin array `index`. */
export function binArrayAccount(lbPair: PublicKey, index: number, bins: Record<number, BinFixture>) {
  const a = blank("binArray");
  Object.assign(a, { index: new BN(index), lbPair });
  for (const [binId, bin] of Object.entries(bins)) Object.assign(a.bins[Number(binId) - index * BINS_PER_ARRAY], bin);
  return encode("binArray", a);
}

export const binArrayKey = (lbPair: PublicKey, index: number) => sdk.deriveBinArray(lbPair, new BN(index), dlmmProgram.programId)[0];

export function mintAccount(decimals: number) {
  const data = Buffer.alloc(MintLayout.span);
  MintLayout.encode({ mintAuthorityOption: 0, mintAuthority: PublicKey.default, supply: 0n, decimals, isInitialized: true, freezeAuthorityOption: 0, freezeAuthority: PublicKey.default }, data);
  return accountInfo(data, TOKEN_PROGRAM_ID);
}

export function tokenAccount(mint: PublicKey, owner: PublicKey, amount: bigint) {
  const data = Buffer.alloc(AccountLayout.span);
  AccountLayout.encode(
    { mint, owner, amount, delegateOption: 0, delegate: PublicKey.default, state: 1, isNativeOption: 0, isNative: 0n, delegatedAmount: 0n, closeAuthorityOption: 0, closeAuthority: PublicKey.default },
    data,
  );
  return accountInfo(data, TOKEN_PROGRAM_ID);
}

export const clockAccount = () => accountInfo(Buffer.alloc(sdk.ClockLayout.span), new PublicKey("Sysvar1111111111111111111111111111111111111"));
