import { describe, expect, it, vi } from "vitest";
import { Alerter, runAlert } from "../src/alerts";

const hook = () => vi.fn(async () => ({ ok: true, status: 200, json: async () => ({}) })) as unknown as typeof fetch;

describe("Alerter", () => {
  it("posts warn and error to the webhook, info only when enabled", async () => {
    const fetchFn = hook();
    const a = new Alerter({ webhookUrl: "https://h", info: false, fetchFn });
    await a.send({ level: "warn", reason: "r", message: "m" });
    await a.send({ level: "info", reason: "posted", message: "m" });
    expect(fetchFn).toHaveBeenCalledTimes(1);
    expect(JSON.parse((fetchFn as any).mock.calls[0][1].body)).toMatchObject({ level: "warn", reason: "r" });
    const b = new Alerter({ webhookUrl: "https://h", info: true, fetchFn });
    await b.send({ level: "info", reason: "posted", message: "m" });
    expect(fetchFn).toHaveBeenCalledTimes(2);
  });

  it("does nothing without a webhook and survives a failing webhook", async () => {
    await expect(new Alerter({ info: true }).send({ level: "error", reason: "r", message: "m" })).resolves.toBeUndefined();
    const failing = vi.fn(async () => { throw new Error("down"); }) as unknown as typeof fetch;
    await expect(new Alerter({ webhookUrl: "https://h", info: true, fetchFn: failing }).send({ level: "error", reason: "r", message: "m" })).resolves.toBeUndefined();
  });

  it("fire dedupes by key until the alert changes or clears", async () => {
    const fetchFn = hook();
    const a = new Alerter({ webhookUrl: "https://h", info: true, fetchFn });
    await a.fire("k", { level: "warn", reason: "low_sol", message: "0.01" });
    await a.fire("k", { level: "warn", reason: "low_sol", message: "0.01" });
    expect(fetchFn).toHaveBeenCalledTimes(1);
    await a.fire("k", { level: "warn", reason: "low_sol", message: "0.02" });
    expect(fetchFn).toHaveBeenCalledTimes(2);
    await a.fire("k", null);
    await a.fire("k", { level: "warn", reason: "low_sol", message: "0.02" });
    expect(fetchFn).toHaveBeenCalledTimes(3);
  });
});

describe("runAlert", () => {
  const base = { vault: "v", epoch: 3, navBefore: 1n } as const;
  it("fires on a new status and stays quiet on a repeat", () => {
    expect(runAlert({ ...base, status: "needs_override", error: "NavDeviationExceeded" }, null)?.reason).toBe("needs_override");
    expect(runAlert({ ...base, status: "needs_override", error: "NavDeviationExceeded" }, { status: "needs_override", last_error: "NavDeviationExceeded", attempts: 4 })).toBeNull();
  });
  it("fires again when the error changes", () => {
    expect(runAlert({ ...base, status: "failed", error: "b" }, { status: "failed", last_error: "a", attempts: 1 })?.reason).toBe("run_failed");
  });
  it("maps statuses to reasons and levels", () => {
    expect(runAlert({ ...base, status: "posted", signature: "s", navAfter: 2n }, null)).toMatchObject({ level: "info", reason: "posted", vault: "v", epoch: 3 });
    expect(runAlert({ ...base, status: "failed", error: "x" }, null)).toMatchObject({ level: "error", reason: "run_failed" });
    expect(runAlert({ ...base, status: "needs_override", error: "x" }, null)).toMatchObject({ level: "warn" });
    expect(runAlert({ ...base, status: "skipped", error: "x" }, null)).toBeNull();
    expect(runAlert({ ...base, status: "dry_run" }, null)).toBeNull();
  });
});
