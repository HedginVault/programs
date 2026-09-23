import { AccountMeta, Connection, PublicKey } from "@solana/web3.js";
import { PHOENIX_API_URL, PHOENIX_GLOBAL_CONFIGURATION, PHOENIX_PROGRAM_ID } from "./constants";

/// Capability bits an onboarded trader holds: place market, deposit, withdraw.
const TRADER_READY_FLAGS = (1 << 2) | (1 << 4) | (1 << 5);
const TRADER_HOT_FLAG = 1 << 0;

const find = (seeds: Buffer[]) => PublicKey.findProgramAddressSync(seeds, PHOENIX_PROGRAM_ID)[0];

/// Cross-margin trader account `(0, 0)` the program registers for a vault.
export const getPhoenixTraderAccount = (authority: PublicKey) =>
  find([Buffer.from("trader"), authority.toBuffer(), Buffer.from([0, 0])]);

export const getPhoenixSplineCollection = (orderbook: PublicKey) =>
  find([Buffer.from("spline"), orderbook.toBuffer()]);

export type PhoenixExchange = {
  canonicalMint: PublicKey;
  globalVault: PublicKey;
  perpAssetMap: PublicKey;
  withdrawQueue: PublicKey;
  /// Global trader index then active trader buffer, headers first, passed as remaining accounts.
  tail: AccountMeta[];
};

export type PhoenixMarket = {
  symbol: string;
  orderbook: PublicKey;
  splineCollection: PublicKey;
  tickSize: number;
  baseLotsDecimals: number;
};

export type PhoenixTrader = {
  authority: PublicKey;
  quoteLotCollateral: bigint;
  flags: number;
  withdrawQueueNode: number;
  positionCount: bigint;
  isReady: boolean;
  isHot: boolean;
};

const readKey = (data: Buffer, offset: number) => new PublicKey(data.subarray(offset, offset + 32));

/// Arena accounts are PDAs `[seed, [index]]`, index 0 being the header named by the global config.
async function getArenaAccounts(connection: Connection, header: PublicKey, seed: string) {
  const data = (await connection.getAccountInfo(header))!.data;
  const count = Math.min(data.readUInt16LE(52), data.readUInt16LE(54));
  const arenas = Array.from({ length: count - 1 }, (_, i) => find([Buffer.from(seed), Buffer.from([i + 1])]));

  return [header, ...arenas];
}

/// Exchange-wide accounts, read from the Phoenix global configuration the same way the program checks them.
export async function getPhoenixExchange(connection: Connection): Promise<PhoenixExchange> {
  const data = (await connection.getAccountInfo(PHOENIX_GLOBAL_CONFIGURATION))!.data;

  const tail = [
    ...(await getArenaAccounts(connection, readKey(data, 392), "global_trader_index")),
    ...(await getArenaAccounts(connection, readKey(data, 424), "active_trader_buffer")),
  ].map((pubkey) => ({ pubkey, isSigner: false, isWritable: true }));

  return {
    canonicalMint: readKey(data, 296),
    globalVault: readKey(data, 328),
    perpAssetMap: readKey(data, 360),
    withdrawQueue: readKey(data, 472),
    tail,
  };
}

export async function getPhoenixMarket(symbol: string): Promise<PhoenixMarket> {
  const response = await fetch(`${PHOENIX_API_URL}/v1/view/exchange/market/${encodeURIComponent(symbol)}`);
  if (!response.ok) throw new Error(`Phoenix market ${symbol}: ${response.status} ${await response.text()}`);

  const market = await response.json();
  const orderbook = new PublicKey(market.marketPubkey);

  return {
    symbol: market.symbol,
    orderbook,
    splineCollection: getPhoenixSplineCollection(orderbook),
    tickSize: market.tickSize,
    baseLotsDecimals: market.baseLotsDecimals,
  };
}

export async function getPhoenixTrader(connection: Connection, traderAccount: PublicKey): Promise<PhoenixTrader | null> {
  const info = await connection.getAccountInfo(traderAccount);
  if (!info) return null;

  const flags = info.data.readUInt32LE(96);

  return {
    authority: readKey(info.data, 56),
    quoteLotCollateral: info.data.readBigInt64LE(88),
    flags,
    withdrawQueueNode: info.data.readUInt32LE(108),
    positionCount: info.data.readBigUInt64LE(224),
    isReady: (flags & TRADER_READY_FLAGS) === TRADER_READY_FLAGS,
    isHot: (flags & TRADER_HOT_FLAG) !== 0,
  };
}

async function postPhoenix(path: string, body: object) {
  const response = await fetch(`${PHOENIX_API_URL}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) throw new Error(`Phoenix ${path}: ${response.status} ${await response.text()}`);

  return response.json();
}

/// Phoenix's public builder API returns the onboarding instruction, co-signed later by its onboarder.
export const buildPhoenixRegisterIxs = (traderAuthority: PublicKey, txFeePayer: PublicKey) =>
  postPhoenix("/v1/exchange/build-register-ixs", {
    traderAuthority: traderAuthority.toBase58(),
    txFeePayer: txFeePayer.toBase58(),
    maxPositions: 128,
  });

export const sendPhoenixRegisterIxs = (transaction: string, traderAuthority: PublicKey, txFeePayer: PublicKey) =>
  postPhoenix("/v1/exchange/send-register-ixs", {
    transaction,
    traderAuthority: traderAuthority.toBase58(),
    txFeePayer: txFeePayer.toBase58(),
    maxPositions: 128,
    traderPdaIndex: 0,
    traderSubaccountIndex: 0,
  });
