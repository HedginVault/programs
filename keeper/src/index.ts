import "dotenv/config";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { Alerter } from "./alerts";
import { Chain } from "./chain";
import { ALERT_INFO, EPOCH_POST_OFFSET_SECS, JUPITER_API_HOST, loadConfig, MIN_SOL_BALANCE, POST_WHILE_PAUSED, TICK_INTERVAL_SECS } from "./config";
import { Db } from "./db";
import { log } from "./log";
import { runVault, type RunnerDeps } from "./runner";
import { decide } from "./scheduler";
import { settleVault } from "./settle";
import { JupiterPricer } from "./valuation/pricer";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function main() {
  const cfg = loadConfig();
  const chain = Chain.create(cfg.rpcUrl, cfg.programId);
  const db = Db.connect(cfg.databaseUrl);
  // `src/` under tsx, `dist/src/` after tsc: both resolve to keeper/migrations.
  const migrationsDir = [join(__dirname, "..", "migrations"), join(__dirname, "..", "..", "migrations")].find((d) => existsSync(d));
  if (!migrationsDir) throw new Error("migrations directory not found");
  const applied = await db.migrate(migrationsDir);
  if (applied.length) log.info("migrations applied", { applied });

  const config = await chain.fetchConfig();
  if (!config.navUpdater.equals(cfg.keypair.publicKey)) {
    log.error("keypair is not the on-chain nav_updater", { keypair: cfg.keypair.publicKey.toBase58(), navUpdater: config.navUpdater.toBase58() });
    process.exit(1);
  }
  const alerter = new Alerter({ webhookUrl: cfg.alertWebhookUrl, info: ALERT_INFO });
  const deps: RunnerDeps = {
    chain,
    pricer: new JupiterPricer({ host: JUPITER_API_HOST, apiKey: cfg.jupiterApiKey }),
    db,
    alerter,
    keypair: cfg.keypair,
    dryRun: cfg.dryRun,
  };
  log.info("keeper started", {
    updater: cfg.keypair.publicKey.toBase58(),
    programId: chain.programId.toBase58(),
    genesis: await chain.connection.getGenesisHash(),
    dryRun: cfg.dryRun,
    tickIntervalSecs: TICK_INTERVAL_SECS,
  });

  let stopping = false;
  const stop = () => {
    log.info("shutdown requested, finishing the in-flight vault");
    stopping = true;
  };
  process.on("SIGINT", stop);
  process.on("SIGTERM", stop);

  while (!stopping) {
    try {
      await tick(deps, EPOCH_POST_OFFSET_SECS, MIN_SOL_BALANCE, POST_WHILE_PAUSED, () => stopping);
    } catch (e) {
      log.error("tick failed", { error: e instanceof Error ? e.message : String(e) });
    }
    if (!stopping) await sleep(TICK_INTERVAL_SECS * 1000);
  }
  await db.close();
  log.info("keeper stopped");
}

async function tick(deps: RunnerDeps, offsetSecs: number, minSol: number, postWhilePaused: boolean, stopping: () => boolean) {
  const { chain, alerter, keypair } = deps;
  const config = await chain.fetchConfig();
  if (!config.navUpdater.equals(keypair.publicKey)) {
    await alerter.fire("updater", { level: "error", reason: "updater_mismatch", message: `on-chain nav_updater is ${config.navUpdater.toBase58()}` });
    return;
  }
  await alerter.fire("updater", null);

  const sol = await chain.fetchSolBalance(keypair.publicKey);
  await alerter.fire("sol", sol < minSol ? { level: "warn", reason: "low_sol", message: `updater balance ${sol.toFixed(4)} SOL` } : null);

  const paused = "paused" in config.status;
  if (paused && !postWhilePaused) {
    await alerter.fire("paused", { level: "warn", reason: "protocol_paused", message: "protocol is paused, not posting" });
    return;
  }
  await alerter.fire("paused", null);

  const now = Math.floor(Date.now() / 1000);
  const vaults = await chain.fetchVaults();
  let posted = false;
  for (const { key, account } of vaults) {
    if (stopping()) return;
    const decision = decide(account.navEpoch.toNumber(), now, offsetSecs);
    if (decision.action === "skip") continue;
    log.info("vault due", { vault: key.toBase58(), epoch: decision.epoch, navEpoch: account.navEpoch.toNumber(), overdue: decision.overdue });
    if ((await runVault(deps, key, account, decision.epoch, decision.overdue)) === "posted") posted = true;
  }

  // A post moves nav_epoch, which is what makes requests resolvable.
  for (const { key, account } of posted ? await chain.fetchVaults() : vaults) {
    if (stopping()) return;
    const vault = key.toBase58();
    try {
      await settleVault(deps, key, account, config);
      await alerter.fire(`settle:${vault}`, null);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      log.error("settlement failed", { vault, error: message });
      await alerter.fire(`settle:${vault}`, { level: "warn", reason: "settle_error", message, vault });
    }
  }
}

main().catch((e) => {
  log.error("fatal", { error: e instanceof Error ? e.stack ?? e.message : String(e) });
  process.exit(1);
});
