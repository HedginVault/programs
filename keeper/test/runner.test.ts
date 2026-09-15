import { Keypair, PublicKey } from "@solana/web3.js";
import BN from "bn.js";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../src/valuation/index", async (orig) => ({ ...(await orig<any>()), valueVault: vi.fn() }));
vi.mock("../src/post", async (orig) => ({ ...(await orig<any>()), postNav: vi.fn() }));

import { valueVault, ValuationError } from "../src/valuation/index";
import { postNav } from "../src/post";
import { runVault, type RunnerDeps } from "../src/runner";

const vault = PublicKey.unique();
const account = { navPerShare: new BN(1_000_000_000), navEpoch: new BN(1) } as any;

function deps(previous: any = null) {
  const upsertRun = vi.fn(async () => ({ previous }));
  const send = vi.fn(async () => {});
  const d: RunnerDeps = {
    chain: {} as any, positions: {} as any, pricer: {} as any, keypair: Keypair.generate(), dryRun: false,
    db: { upsertRun } as any,
    alerter: { send, fire: vi.fn(async () => {}) } as any,
  };
  return { d, upsertRun, send };
}

beforeEach(() => vi.clearAllMocks());

describe("runVault", () => {
  it("values, posts, records and alerts on success", async () => {
    (valueVault as any).mockResolvedValue({ vault: vault.toBase58(), epoch: 2, totalAssets: 500n, idleBalance: 100n, depositPriceUsd: 1, holdings: [] });
    (postNav as any).mockResolvedValue({ status: "posted", signature: "sig", navAfter: 1_010_000_000n });
    const { d, upsertRun, send } = deps();
    expect(await runVault(d, vault, account, 2, false)).toBe("posted");
    expect((postNav as any).mock.calls[0][0]).toMatchObject({ totalAssets: 500n, epoch: 2, dryRun: false });
    expect(upsertRun).toHaveBeenCalledWith(expect.objectContaining({ vault: vault.toBase58(), epoch: 2, status: "posted", totalAssets: 500n, idleBalance: 100n, navBefore: 1_000_000_000n, navAfter: 1_010_000_000n, signature: "sig" }));
    expect(send).toHaveBeenCalledWith(expect.objectContaining({ reason: "posted" }));
  });

  it("records a valuation failure without posting", async () => {
    (valueVault as any).mockRejectedValue(new ValuationError("missing_price:m"));
    const { d, upsertRun, send } = deps();
    expect(await runVault(d, vault, account, 2, false)).toBe("failed");
    expect(postNav).not.toHaveBeenCalled();
    expect(upsertRun).toHaveBeenCalledWith(expect.objectContaining({ status: "failed", error: "missing_price:m" }));
    expect(send).toHaveBeenCalledWith(expect.objectContaining({ reason: "run_failed" }));
  });

  it("records unexpected errors as failed with the message", async () => {
    (valueVault as any).mockRejectedValue(new Error("rpc down"));
    const { d, upsertRun } = deps();
    expect(await runVault(d, vault, account, 2, false)).toBe("failed");
    expect(upsertRun).toHaveBeenCalledWith(expect.objectContaining({ status: "failed", error: "rpc down" }));
  });

  it("stays quiet on a repeated outcome and escalates overdue", async () => {
    (valueVault as any).mockResolvedValue({ vault: vault.toBase58(), epoch: 2, totalAssets: 1n, idleBalance: 1n, depositPriceUsd: 1, holdings: [] });
    (postNav as any).mockResolvedValue({ status: "needs_override", error: "NavDeviationExceeded" });
    const { d, send } = deps({ status: "needs_override", last_error: "NavDeviationExceeded", attempts: 3 });
    expect(await runVault(d, vault, account, 2, true)).toBe("needs_override");
    expect(send).not.toHaveBeenCalled();
    expect(d.alerter.fire).toHaveBeenCalledWith(`overdue:${vault.toBase58()}`, expect.objectContaining({ reason: "overdue", level: "error" }));
  });

  it("describes a never-posted vault instead of counting epochs", async () => {
    (valueVault as any).mockResolvedValue({ vault: vault.toBase58(), epoch: 2, totalAssets: 1n, idleBalance: 1n, depositPriceUsd: 1, holdings: [] });
    (postNav as any).mockResolvedValue({ status: "failed", error: "x" });
    const { d } = deps();
    await runVault(d, vault, { ...account, navEpoch: new BN(0) }, 2, true);
    expect(d.alerter.fire).toHaveBeenCalledWith(`overdue:${vault.toBase58()}`, expect.objectContaining({ message: expect.stringContaining("never had a NAV posted") }));
  });

  it("clears the overdue alert on a post", async () => {
    (valueVault as any).mockResolvedValue({ vault: vault.toBase58(), epoch: 2, totalAssets: 1n, idleBalance: 1n, depositPriceUsd: 1, holdings: [] });
    (postNav as any).mockResolvedValue({ status: "posted", signature: "s", navAfter: 1n });
    const { d } = deps();
    await runVault(d, vault, account, 2, true);
    expect(d.alerter.fire).toHaveBeenCalledWith(`overdue:${vault.toBase58()}`, null);
  });
});
