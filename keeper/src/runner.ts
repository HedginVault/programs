import type { Keypair, PublicKey } from "@solana/web3.js";
import { runAlert, type Alerter } from "./alerts";
import type { Chain, VaultAccount } from "./chain";
import type { Db, RunRecord } from "./db";
import { log } from "./log";
import { postNav, type RunStatus } from "./post";
import { ValuationError, valueVault } from "./valuation/index";
import type { Pricer } from "./valuation/pricer";

export interface RunnerDeps {
  chain: Chain;
  pricer: Pricer;
  db: Db;
  alerter: Alerter;
  keypair: Keypair;
  dryRun: boolean;
}

/** One vault, one epoch: value → post → record → alert. Never throws; every path becomes a run row. */
export async function runVault(deps: RunnerDeps, vault: PublicKey, account: VaultAccount, epoch: number, overdue: boolean): Promise<RunStatus> {
  const key = vault.toBase58();
  const navBefore = BigInt(account.navPerShare.toString());
  let record: RunRecord;

  try {
    const valuation = await valueVault(deps, vault, account, epoch);
    log.info("valuation", { vault: key, epoch, totalAssets: valuation.totalAssets, idleBalance: valuation.idleBalance, holdings: valuation.holdings.length, slot: valuation.slot });
    const outcome = await postNav({ chain: deps.chain, keypair: deps.keypair, vault, account, totalAssets: valuation.totalAssets, epoch, dryRun: deps.dryRun });
    record = {
      vault: key,
      epoch,
      status: outcome.status,
      totalAssets: valuation.totalAssets,
      idleBalance: valuation.idleBalance,
      breakdown: valuation.holdings,
      slot: valuation.slot,
      navBefore,
      ...(outcome.status === "posted" ? { signature: outcome.signature, navAfter: outcome.navAfter } : {}),
      ...("error" in outcome ? { error: outcome.error } : {}),
    };
  } catch (e) {
    const error = e instanceof ValuationError ? e.reason : e instanceof Error ? e.message : String(e);
    record = { vault: key, epoch, status: "failed", navBefore, error };
  }

  const { previous } = await deps.db.upsertRun(record);
  log.info("run recorded", { vault: key, epoch, status: record.status, error: record.error, attempts: (previous?.attempts ?? 0) + 1 });

  const alert = runAlert(record, previous);
  if (alert) await deps.alerter.send(alert);

  if (record.status === "posted" || !overdue) {
    await deps.alerter.fire(`overdue:${key}`, null);
  } else {
    const navEpoch = account.navEpoch.toNumber();
    const behind = navEpoch === 0 ? "has never had a NAV posted" : `is ${epoch - navEpoch} epochs behind`;
    await deps.alerter.fire(`overdue:${key}`, { level: "error", reason: "overdue", message: `vault ${behind} (${record.status}: ${record.error ?? ""})`, vault: key, epoch });
  }
  return record.status;
}
