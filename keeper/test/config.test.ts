import { Keypair } from "@solana/web3.js";
import { describe, expect, it } from "vitest";
import { loadConfig } from "../src/config";

const kp = Keypair.generate();
const base = {
  RPC_URL: "http://localhost:8899",
  KEEPER_KEYPAIR: JSON.stringify(Array.from(kp.secretKey)),
  DATABASE_URL: "postgres://localhost/x",
};

describe("loadConfig", () => {
  it("parses the keypair from a JSON u8 array and applies defaults", () => {
    const c = loadConfig(base);
    expect(c.keypair.publicKey.equals(kp.publicKey)).toBe(true);
    expect(c.tickIntervalSecs).toBe(60);
    expect(c.epochPostOffsetSecs).toBe(300);
    expect(c.minSolBalance).toBe(0.05);
    expect(c.postWhilePaused).toBe(false);
    expect(c.dryRun).toBe(false);
    expect(c.alertInfo).toBe(false);
    expect(c.alertWebhookUrl).toBeUndefined();
    expect(c.jupiterApiHost).toBe("https://lite-api.jup.ag");
    expect(c.programId).toBeUndefined();
  });

  it("switches the Jupiter host when a key is present", () => {
    expect(loadConfig({ ...base, JUPITER_API_KEY: "k" }).jupiterApiHost).toBe("https://api.jup.ag");
    expect(loadConfig({ ...base, JUPITER_API_KEY: "k", JUPITER_API_HOST: "https://x" }).jupiterApiHost).toBe("https://x");
  });

  it("parses booleans and numbers", () => {
    const c = loadConfig({ ...base, DRY_RUN: "true", POST_WHILE_PAUSED: "TRUE", TICK_INTERVAL_SECS: "5", ALERT_WEBHOOK_URL: "https://hook" });
    expect(c.dryRun).toBe(true);
    expect(c.postWhilePaused).toBe(true);
    expect(c.tickIntervalSecs).toBe(5);
    expect(c.alertWebhookUrl).toBe("https://hook");
  });

  it("rejects a missing or malformed keypair", () => {
    expect(() => loadConfig({ ...base, KEEPER_KEYPAIR: "" })).toThrow(/KEEPER_KEYPAIR/);
    expect(() => loadConfig({ ...base, KEEPER_KEYPAIR: "[1,2,3]" })).toThrow(/64/);
    expect(() => loadConfig({ ...base, KEEPER_KEYPAIR: "not json" })).toThrow(/KEEPER_KEYPAIR/);
  });

  it("rejects missing required vars and bad numbers", () => {
    expect(() => loadConfig({ ...base, RPC_URL: "" })).toThrow(/RPC_URL/);
    expect(() => loadConfig({ ...base, DATABASE_URL: undefined })).toThrow(/DATABASE_URL/);
    expect(() => loadConfig({ ...base, TICK_INTERVAL_SECS: "abc" })).toThrow(/TICK_INTERVAL_SECS/);
  });
});
