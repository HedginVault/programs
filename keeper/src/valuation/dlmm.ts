import type { PublicKey } from "@solana/web3.js";
import * as sdk from "@meteora-ag/dlmm";
import type { Chain } from "../chain";

// The CJS build replaces `module.exports` with the DLMM class and copies the named exports onto
// it, so `.default` may be undefined; accept both shapes.
type Sdk = typeof import("@meteora-ag/dlmm");
const DLMM = ((sdk as unknown as { default?: Sdk["default"] }).default ?? sdk) as unknown as Sdk["default"];

export interface DlmmPositionAmounts {
  lbPair: string;
  tokenX: { mint: string; decimals: number };
  tokenY: { mint: string; decimals: number };
  amountX: bigint;
  amountY: bigint;
  feeX: bigint;
  feeY: bigint;
}

export interface PositionReader {
  /** Amounts per position pubkey. A position whose account does not exist is absent. */
  read(positions: PublicKey[]): Promise<Map<string, DlmmPositionAmounts>>;
}

type PositionAccount = { lbPair: PublicKey };

/**
 * One batched read of the position accounts (to learn each lbPair), one `DLMM.createMultiple` for
 * the distinct pools, then one `getPosition` per position for bin amounts and pending fees.
 */
export class DlmmReader implements PositionReader {
  constructor(private readonly chain: Chain) {}

  async read(positions: PublicKey[]): Promise<Map<string, DlmmPositionAmounts>> {
    const out = new Map<string, DlmmPositionAmounts>();
    if (positions.length === 0) return out;
    const infos = await this.chain.fetchAccountInfos(positions);
    const live = positions.flatMap((position, i) => {
      const info = infos[i];
      if (!info) return [];
      const account = this.chain.program.coder.accounts.decode<PositionAccount>("positionV2", info.data);
      return [{ position, lbPair: account.lbPair }];
    });
    if (live.length === 0) return out;

    const lbPairs = [...new Map(live.map((l) => [l.lbPair.toBase58(), l.lbPair])).values()];
    const pools = new Map((await DLMM.createMultiple(this.chain.connection, lbPairs)).map((p) => [p.pubkey.toBase58(), p]));

    for (const { position, lbPair } of live) {
      const pool = pools.get(lbPair.toBase58());
      if (!pool) throw new Error(`DLMM pool ${lbPair.toBase58()} unavailable`);
      const { positionData } = await pool.getPosition(position);
      out.set(position.toBase58(), {
        lbPair: lbPair.toBase58(),
        tokenX: { mint: pool.tokenX.publicKey.toBase58(), decimals: pool.tokenX.mint.decimals },
        tokenY: { mint: pool.tokenY.publicKey.toBase58(), decimals: pool.tokenY.mint.decimals },
        amountX: BigInt(positionData.totalXAmountExcludeTransferFee.toString()),
        amountY: BigInt(positionData.totalYAmountExcludeTransferFee.toString()),
        feeX: BigInt(positionData.feeXExcludeTransferFee.toString()),
        feeY: BigInt(positionData.feeYExcludeTransferFee.toString()),
      });
    }
    return out;
  }
}
