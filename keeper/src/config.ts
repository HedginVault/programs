import { Keypair } from "@solana/web3.js";

/** Loop period. */
export const TICK_INTERVAL_SECS = 60;
/** Wait after an epoch boundary before posting, absorbs chain clock lag. */
export const EPOCH_POST_OFFSET_SECS = 300;
/** `low_sol` alert threshold for the updater key, in SOL. */
export const MIN_SOL_BALANCE = 0.05;
/** Skip posting while the protocol status is Paused; a pause may be a reaction to bad pricing. */
export const POST_WHILE_PAUSED = false;
/** Forward successful posts to the webhook as well as warn/error alerts. */
export const ALERT_INFO = false;
/** Jupiter price API host; the key is required. */
export const JUPITER_API_HOST = "https://api.jup.ag";

export interface KeeperConfig {
  rpcUrl: string;
  keypair: Keypair;
  databaseUrl: string;
  programId: string;
  jupiterApiKey: string;
  dryRun: boolean;
  alertWebhookUrl?: string;
}

const str = (env: NodeJS.ProcessEnv, key: string) => env[key]?.trim() || undefined;

function required(env: NodeJS.ProcessEnv, key: string): string {
  const v = str(env, key);
  if (!v) throw new Error(`${key} is required`);
  return v;
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
  return {
    rpcUrl: required(env, "RPC_URL"),
    keypair: keypair(env),
    databaseUrl: required(env, "DATABASE_URL"),
    programId: required(env, "PROGRAM_ID"),
    jupiterApiKey: required(env, "JUPITER_API_KEY"),
    dryRun: bool(env, "DRY_RUN"),
    alertWebhookUrl: str(env, "ALERT_WEBHOOK_URL"),
  };
}
