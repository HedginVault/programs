import type { Keypair, PublicKey, TransactionInstruction } from "@solana/web3.js";
import type { Alerter } from "./alerts";
import type { Chain, ConfigAccount, DepositRequestAccount, VaultAccount, WithdrawalRequestAccount } from "./chain";
import { log } from "./log";
import { decodeAnchorError } from "./post";
import { buildTx, sendTx, simulateTx, type BuiltTx, type Simulation } from "./tx";

export type RequestKind = "deposit" | "withdrawal";

export interface PendingRequest {
  kind: RequestKind;
  /** Request PDA. */
  key: PublicKey;
  owner: PublicKey;
  epoch: number;
  createdTs: number;
}

export interface Requests {
  deposits: { key: PublicKey; account: DepositRequestAccount }[];
  withdrawals: { key: PublicKey; account: WithdrawalRequestAccount }[];
}

/** Resolves per transaction. Six of one kind fit the 1232-byte packet; mixing kinds does not. */
export const RESOLVES_PER_TX = 6;
const CHUNK_COMPUTE_UNITS = 1_000_000;
const SINGLE_COMPUTE_UNITS = 200_000;

const isNormal = (status: object) => "normal" in status;

type RequestRow = { key: PublicKey; account: { authority: PublicKey; epoch: { toNumber(): number }; createdTs: { toNumber(): number } } };

/**
 * Requests the program accepts now, in settlement order: deposits first (they add idle cash and raise
 * the outflow cap for withdrawals), each kind oldest first.
 */
export function selectReady(vault: VaultAccount, config: ConfigAccount, requests: Requests): PendingRequest[] {
  const navEpoch = vault.navEpoch.toNumber();
  const ready = (kind: RequestKind, rows: RequestRow[]) =>
    rows
      .map((r): PendingRequest => ({ kind, key: r.key, owner: r.account.authority, epoch: r.account.epoch.toNumber(), createdTs: r.account.createdTs.toNumber() }))
      .filter((r) => r.epoch < navEpoch)
      .sort((a, b) => a.createdTs - b.createdTs || a.key.toBase58().localeCompare(b.key.toBase58()));
  const depositsOpen = isNormal(config.status) && isNormal(vault.status) && vault.depositPaused === 0 && !vault.navPerShare.isZero();
  const withdrawalsOpen = vault.withdrawalPaused === 0;
  return [...(depositsOpen ? ready("deposit", requests.deposits) : []), ...(withdrawalsOpen ? ready("withdrawal", requests.withdrawals) : [])];
}

/** Consecutive chunks of at most `size`, each of a single kind. */
export function chunkRequests(requests: PendingRequest[], size = RESOLVES_PER_TX): PendingRequest[][] {
  const chunks: PendingRequest[][] = [];
  for (const r of requests) {
    const last = chunks[chunks.length - 1];
    if (last && last.length < size && last[0].kind === r.kind) last.push(r);
    else chunks.push([r]);
  }
  return chunks;
}

export interface SettleDeps {
  chain: Chain;
  alerter: Alerter;
  keypair: Keypair;
  dryRun: boolean;
}

export interface SettleResult {
  settled: PendingRequest[];
  failed: { request: PendingRequest; error: string }[];
}

const simulationError = (sim: Extract<Simulation, { ok: false }>) => decodeAnchorError(sim.logs)?.code ?? `simulation:${JSON.stringify(sim.err)}`;

/**
 * Resolves every ready request of one vault. A chunk that simulates cleanly goes out as one
 * transaction; a failing chunk is retried one request per transaction, so one bad request cannot
 * block the rest. A request counts as settled only once its account is closed on chain.
 */
export async function settleVault(deps: SettleDeps, vault: PublicKey, account: VaultAccount, config: ConfigAccount): Promise<SettleResult> {
  const { chain, alerter, keypair, dryRun } = deps;
  const result: SettleResult = { settled: [], failed: [] };
  if ((account.pendingDeposits.isZero() && account.pendingWithdrawalShares.isZero()) || "paused" in account.status) return result;

  const ready = selectReady(account, config, await chain.fetchRequests(vault));
  if (ready.length === 0) return result;

  const vaultKey = vault.toBase58();
  const tokenProgram = (await chain.fetchMintInfos([account.depositMint])).get(account.depositMint.toBase58())!.tokenProgram;
  const alertKey = (r: PendingRequest) => `settle:${vaultKey}:${r.kind}:${r.owner.toBase58()}:${r.epoch}`;

  const instruction = (r: PendingRequest): Promise<TransactionInstruction> => {
    const common = {
      resolver: keypair.publicKey,
      config: chain.configPda(),
      vault,
      depositMint: account.depositMint,
      shareMint: chain.shareMintPda(vault),
      depositMintTokenProgram: tokenProgram,
    };
    return r.kind === "deposit"
      ? chain.program.methods.depositRequestResolve().accounts({ ...common, depositor: r.owner, depositRequest: r.key }).instruction()
      : chain.program.methods.withdrawalRequestResolve().accounts({ ...common, withdrawer: r.owner, withdrawalRequest: r.key }).instruction();
  };
  const build = async (requests: PendingRequest[], units: number) => buildTx(chain, keypair.publicKey, await Promise.all(requests.map(instruction)), units);

  const settled = async (r: PendingRequest) => {
    result.settled.push(r);
    await alerter.fire(alertKey(r), null);
  };
  const failed = async (r: PendingRequest, error: string) => {
    result.failed.push({ request: r, error });
    await alerter.fire(alertKey(r), {
      level: "warn",
      reason: "settle_failed",
      message: `${r.kind} of ${r.owner.toBase58()} (epoch ${r.epoch}): ${error}`,
      vault: vaultKey,
      epoch: r.epoch,
    });
  };

  const submit = async (requests: PendingRequest[], built: BuiltTx) => {
    if (dryRun) {
      result.settled.push(...requests);
      return;
    }
    const { signature, error } = await sendTx(chain, keypair, built);
    log.info("settle sent", { vault: vaultKey, kind: requests[0].kind, count: requests.length, signature, error });
    // The chain decides: a resolved request's account is closed.
    const infos = await chain.fetchAccountInfos(requests.map((r) => r.key));
    for (const [i, r] of requests.entries()) {
      if (infos[i]) await failed(r, error ?? "not_settled");
      else await settled(r);
    }
  };

  for (const chunk of chunkRequests(ready)) {
    const built = await build(chunk, CHUNK_COMPUTE_UNITS);
    const sim = await simulateTx(chain, built);
    if (sim.ok) {
      await submit(chunk, built);
      continue;
    }
    for (const r of chunk) {
      const single = await build([r], SINGLE_COMPUTE_UNITS);
      const s = await simulateTx(chain, single);
      if (s.ok) await submit([r], single);
      else await failed(r, simulationError(s));
    }
  }

  if (result.settled.length > 0) {
    const count = (kind: RequestKind) => result.settled.filter((r) => r.kind === kind).length;
    await alerter.send({ level: "info", reason: dryRun ? "would_settle" : "settled", message: `deposits=${count("deposit")} withdrawals=${count("withdrawal")}`, vault: vaultKey });
  }
  return result;
}
