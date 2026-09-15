import { Keypair } from "@solana/web3.js";

export interface KeeperConfig {
  rpcUrl: string;
  keypair: Keypair;
  databaseUrl: string;
  tickIntervalSecs: number;
  epochPostOffsetSecs: number;
  minSolBalance: number;
  postWhilePaused: boolean;
  dryRun: boolean;
  alertWebhookUrl?: string;
  alertInfo: boolean;
  jupiterApiHost: string;
  jupiterApiKey?: string;
  programId?: string;
}

const str = (env: NodeJS.ProcessEnv, key: string) => env[key]?.trim() || undefined;

function required(env: NodeJS.ProcessEnv, key: string): string {
  const v = str(env, key);
  if (!v) throw new Error(`${key} is required`);
  return v;
}

function num(env: NodeJS.ProcessEnv, key: string, fallback: number): number {
  const v = str(env, key);
  if (v === undefined) return fallback;
  const n = Number(v);
  if (!Number.isFinite(n) || n < 0) throw new Error(`${key} must be a non-negative number`);
  return n;
}

const bool = (env: NodeJS.ProcessEnv, key: string) => (str(env, key) ?? "false").toLowerCase() === "true";

function keypair(env: NodeJS.ProcessEnv): Keypair {
  const raw = required(env, "KEEPER_KEYPAIR");
  let arr: unknown;
  try {
    arr = JSON.parse(raw);
  } catch {
    throw new Error("KEEPER_KEYPAIR must be a JSON array of u8");
  }
  if (!Array.isArray(arr) || arr.length !== 64 || !arr.every((n) => Number.isInteger(n) && n >= 0 && n <= 255)) {
    throw new Error("KEEPER_KEYPAIR must contain exactly 64 u8 numbers");
  }
  return Keypair.fromSecretKey(Uint8Array.from(arr as number[]));
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): KeeperConfig {
  const jupiterApiKey = str(env, "JUPITER_API_KEY");
  return {
    rpcUrl: required(env, "RPC_URL"),
    keypair: keypair(env),
    databaseUrl: required(env, "DATABASE_URL"),
    tickIntervalSecs: num(env, "TICK_INTERVAL_SECS", 60),
    epochPostOffsetSecs: num(env, "EPOCH_POST_OFFSET_SECS", 300),
    minSolBalance: num(env, "MIN_SOL_BALANCE", 0.05),
    postWhilePaused: bool(env, "POST_WHILE_PAUSED"),
    dryRun: bool(env, "DRY_RUN"),
    alertWebhookUrl: str(env, "ALERT_WEBHOOK_URL"),
    alertInfo: bool(env, "ALERT_INFO"),
    jupiterApiHost: str(env, "JUPITER_API_HOST") ?? (jupiterApiKey ? "https://api.jup.ag" : "https://lite-api.jup.ag"),
    jupiterApiKey,
    programId: str(env, "PROGRAM_ID"),
  };
}
