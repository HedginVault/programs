import BN from "bn.js";
import {
  ComputeBudgetProgram,
  Keypair,
  PublicKey,
  TransactionExpiredBlockheightExceededError,
  TransactionMessage,
  VersionedTransaction,
} from "@solana/web3.js";
import idl from "../idl/hedge_vault.json";
import { type Chain, type VaultAccount } from "./chain";
import { log } from "./log";

export type RunStatus = "posted" | "skipped" | "needs_override" | "failed" | "dry_run";

export type PostOutcome =
  | { status: "posted"; signature: string; navAfter: bigint }
  | { status: "dry_run"; unitsConsumed: number }
  | { status: "skipped" | "needs_override" | "failed"; error: string };

const byCode = new Map(idl.errors.map((e) => [e.code, e]));
const byName = new Map(idl.errors.map((e) => [e.name, e]));
const ANCHOR_RE = /Error Code: (\w+)\. Error Number: (\d+)\. Error Message: (.*?)\.?$/;
const CUSTOM_RE = /custom program error: 0x([0-9a-f]+)/i;

export function decodeAnchorError(logs: string[]): { code: string; message: string } | null {
  for (const line of logs) {
    const m = ANCHOR_RE.exec(line);
    if (m) return { code: m[1], message: byName.get(m[1])?.msg ?? m[3] };
  }
  for (const line of logs) {
    const m = CUSTOM_RE.exec(line);
    if (m) {
      const known = byCode.get(parseInt(m[1], 16));
      return known ? { code: known.name, message: known.msg } : { code: `Custom0x${m[1]}`, message: `custom error 0x${m[1]}` };
    }
  }
  return null;
}

/** Simulation is the oracle for fee and deviation math; the keeper never reimplements `update_nav`. */
export function classifySimulation(logs: string[], err: unknown): { status: "skipped" | "needs_override" | "failed"; error: string } {
  const decoded = decodeAnchorError(logs);
  if (decoded?.code === "NavDeviationExceeded") return { status: "needs_override", error: decoded.code };
  if (decoded?.code === "NavAlreadyUpdatedThisEpoch") return { status: "skipped", error: decoded.code };
  return { status: "failed", error: decoded?.code ?? `simulation:${JSON.stringify(err)}` };
}

const COMPUTE_UNITS = 200_000;

export interface PostArgs {
  chain: Chain;
  keypair: Keypair;
  vault: PublicKey;
  account: VaultAccount;
  totalAssets: bigint;
  epoch: number;
  dryRun: boolean;
}

export async function postNav({ chain, keypair, vault, account, totalAssets, epoch, dryRun }: PostArgs): Promise<PostOutcome> {
  const { connection, program } = chain;
  const mintInfo = (await chain.fetchMintInfos([account.depositMint])).get(account.depositMint.toBase58())!;

  const ix = await program.methods
    .navUpdate(new BN(totalAssets.toString()))
    .accounts({
      navUpdater: keypair.publicKey,
      config: chain.configPda(),
      vault,
      depositMint: account.depositMint,
      shareMint: chain.shareMintPda(vault),
      depositMintTokenProgram: mintInfo.tokenProgram,
    })
    .instruction();

  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash("confirmed");
  const message = new TransactionMessage({
    payerKey: keypair.publicKey,
    recentBlockhash: blockhash,
    instructions: [ComputeBudgetProgram.setComputeUnitLimit({ units: COMPUTE_UNITS }), ix],
  }).compileToV0Message();
  const tx = new VersionedTransaction(message);

  const sim = await connection.simulateTransaction(tx, { sigVerify: false, replaceRecentBlockhash: true });
  if (sim.value.err) return classifySimulation(sim.value.logs ?? [], sim.value.err);
  if (dryRun) return { status: "dry_run", unitsConsumed: sim.value.unitsConsumed ?? 0 };

  tx.sign([keypair]);
  const signature = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false, preflightCommitment: "confirmed" });
  log.info("nav_update sent", { vault: vault.toBase58(), epoch, signature });

  try {
    const conf = await connection.confirmTransaction({ signature, blockhash, lastValidBlockHeight }, "confirmed");
    if (conf.value.err) return { status: "failed", error: `confirm:${JSON.stringify(conf.value.err)}` };
  } catch (e) {
    if (!(e instanceof TransactionExpiredBlockheightExceededError)) throw e;
    log.warn("blockhash expired before confirmation, re-reading vault", { vault: vault.toBase58(), signature });
  }

  // The chain decides whether it landed, not the confirmation path.
  const after = await chain.fetchVault(vault);
  if (after && after.navEpoch.toNumber() === epoch) {
    return { status: "posted", signature, navAfter: BigInt(after.navPerShare.toString()) };
  }
  return { status: "failed", error: `not_landed:${signature}` };
}
