import { Keypair } from "@solana/web3.js";
import { describe, expect, it } from "vitest";
import { loadConfig } from "../src/config";

const kp = Keypair.generate();
const base = {
  RPC_URL: "http://localhost:8899",
  KEEPER_KEYPAIR: JSON.stringify(Array.from(kp.secretKey)),
  DATABASE_URL: "postgres://localhost/x",
  PROGRAM_ID: "r2ahBQ6gbPCJ9FxBymYcXuwXi8NmenRry7SE7QR7FAt",
  JUPITER_API_KEY: "k",
};

describe("loadConfig", () => {
  it("parses the keypair from a JSON u8 array and applies defaults", () => {
    const c = loadConfig(base);
    expect(c.keypair.publicKey.equals(kp.publicKey)).toBe(true);
    expect(c.programId).toBe(base.PROGRAM_ID);
    expect(c.jupiterApiKey).toBe("k");
    expect(c.dryRun).toBe(false);
    expect(c.alertWebhookUrl).toBeUndefined();
  });

  it("parses the dry run flag and webhook", () => {
    const c = loadConfig({ ...base, DRY_RUN: "TRUE", ALERT_WEBHOOK_URL: "https://hook" });
    expect(c.dryRun).toBe(true);
    expect(c.alertWebhookUrl).toBe("https://hook");
  });

  it("rejects a missing or malformed keypair", () => {
    expect(() => loadConfig({ ...base, KEEPER_KEYPAIR: "" })).toThrow(/KEEPER_KEYPAIR/);
    expect(() => loadConfig({ ...base, KEEPER_KEYPAIR: "[1,2,3]" })).toThrow(/64/);
    expect(() => loadConfig({ ...base, KEEPER_KEYPAIR: "not json" })).toThrow(/KEEPER_KEYPAIR/);
  });

  it("rejects every missing required var", () => {
    for (const key of ["RPC_URL", "DATABASE_URL", "PROGRAM_ID", "JUPITER_API_KEY"]) {
      expect(() => loadConfig({ ...base, [key]: "" })).toThrow(new RegExp(key));
    }
  });
});
