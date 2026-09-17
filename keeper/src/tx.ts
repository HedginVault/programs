import {
  ComputeBudgetProgram,
  TransactionExpiredBlockheightExceededError,
  TransactionMessage,
  VersionedTransaction,
  type Keypair,
  type PublicKey,
  type TransactionInstruction,
} from "@solana/web3.js";
import type { Chain } from "./chain";
import { log } from "./log";

export interface BuiltTx {
  tx: VersionedTransaction;
  blockhash: string;
  lastValidBlockHeight: number;
}

export type Simulation = { ok: true; unitsConsumed: number } | { ok: false; logs: string[]; err: unknown };

export async function buildTx(chain: Chain, payer: PublicKey, instructions: TransactionInstruction[], computeUnits: number): Promise<BuiltTx> {
  const { blockhash, lastValidBlockHeight } = await chain.connection.getLatestBlockhash("confirmed");
  const message = new TransactionMessage({
    payerKey: payer,
    recentBlockhash: blockhash,
    instructions: [ComputeBudgetProgram.setComputeUnitLimit({ units: computeUnits }), ...instructions],
  }).compileToV0Message();
  return { tx: new VersionedTransaction(message), blockhash, lastValidBlockHeight };
}

export async function simulateTx(chain: Chain, { tx }: BuiltTx): Promise<Simulation> {
  const sim = await chain.connection.simulateTransaction(tx, { sigVerify: false, replaceRecentBlockhash: true });
  return sim.value.err ? { ok: false, logs: sim.value.logs ?? [], err: sim.value.err } : { ok: true, unitsConsumed: sim.value.unitsConsumed ?? 0 };
}

/**
 * Signs, sends and confirms. A confirmation error is returned. An expired blockhash is not, because
 * the transaction may still have landed: callers decide from chain state either way.
 */
export async function sendTx(chain: Chain, keypair: Keypair, { tx, blockhash, lastValidBlockHeight }: BuiltTx): Promise<{ signature: string; error?: string }> {
  tx.sign([keypair]);
  const signature = await chain.connection.sendRawTransaction(tx.serialize(), { skipPreflight: false, preflightCommitment: "confirmed" });
  try {
    const conf = await chain.connection.confirmTransaction({ signature, blockhash, lastValidBlockHeight }, "confirmed");
    if (conf.value.err) return { signature, error: `confirm:${JSON.stringify(conf.value.err)}` };
  } catch (e) {
    if (!(e instanceof TransactionExpiredBlockheightExceededError)) throw e;
    log.warn("blockhash expired before confirmation, re-reading chain state", { signature });
  }
  return { signature };
}
