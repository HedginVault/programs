import { Keypair, PublicKey, type TransactionInstruction } from "@solana/web3.js";
import BN from "bn.js";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../src/tx", () => ({ buildTx: vi.fn(), simulateTx: vi.fn(), sendTx: vi.fn() }));

import { Chain } from "../src/chain";
import { chunkRequests, selectReady, settleVault, type PendingRequest, type Requests } from "../src/settle";
import { buildTx, sendTx, simulateTx } from "../src/tx";

const USDC = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
const TOKEN_PROGRAM_ID = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const vaultKey = PublicKey.unique();
const normal = { normal: {} };
const config = { status: normal } as any;

const vaultAccount = (over: Record<string, unknown> = {}) =>
  ({ navEpoch: new BN(5), navPerShare: new BN(1_000_000_000), status: normal, depositPaused: 0, withdrawalPaused: 0, pendingDeposits: new BN(1), pendingWithdrawalShares: new BN(1), depositMint: USDC, ...over }) as any;
const request = (epoch: number, createdTs: number) => ({
  key: PublicKey.unique(),
  account: { authority: PublicKey.unique(), vault: vaultKey, epoch: new BN(epoch), createdTs: new BN(createdTs) } as any,
});
const anchorLog = (name: string) => `Program log: AnchorError occurred. Error Code: ${name}. Error Number: 6000. Error Message: x.`;

describe("selectReady", () => {
  const d1 = request(4, 20);
  const d0 = request(3, 10);
  const dLater = request(5, 1);
  const w = request(4, 5);
  const requests: Requests = { deposits: [d1, dLater, d0], withdrawals: [w] };

  it("keeps requests from earlier epochs, deposits first, oldest first", () => {
    expect(selectReady(vaultAccount(), config, requests).map((r) => [r.kind, r.key.toBase58()])).toEqual([
      ["deposit", d0.key.toBase58()],
      ["deposit", d1.key.toBase58()],
      ["withdrawal", w.key.toBase58()],
    ]);
  });

  it.each([
    ["the protocol is reduce-only", vaultAccount(), { status: { reduceOnly: {} } }],
    ["the vault is reduce-only", vaultAccount({ status: { reduceOnly: {} } }), config],
    ["the vault NAV is zero", vaultAccount({ navPerShare: new BN(0) }), config],
    ["the manager paused deposits", vaultAccount({ depositPaused: 1 }), config],
  ])("drops deposits when %s", (_, vault, cfg) => {
    expect(selectReady(vault, cfg as any, requests).map((r) => r.kind)).toEqual(["withdrawal"]);
  });

  it("drops withdrawals when the manager paused withdrawals", () => {
    expect(selectReady(vaultAccount({ withdrawalPaused: 1 }), config, requests).map((r) => r.kind)).toEqual(["deposit", "deposit"]);
  });
});

describe("chunkRequests", () => {
  it("caps chunks at 5 and never mixes kinds", () => {
    const mk = (kind: PendingRequest["kind"]): PendingRequest => ({ kind, key: PublicKey.unique(), owner: PublicKey.unique(), epoch: 1, createdTs: 1 });
    const chunks = chunkRequests([...Array.from({ length: 7 }, () => mk("deposit")), mk("withdrawal"), mk("withdrawal")]);
    expect(chunks.map((c) => [c[0].kind, c.length])).toEqual([
      ["deposit", 5],
      ["deposit", 2],
      ["withdrawal", 2],
    ]);
  });
});

describe("settleVault", () => {
  let open: Set<string>;
  let failing: Set<string>;
  const requestKeysOf = (ixs: TransactionInstruction[]) => ixs.flatMap((ix) => ix.keys.map((k) => k.pubkey.toBase58())).filter((k) => open.has(k));

  function setup(requests: Requests, dryRun = false) {
    open = new Set([...requests.deposits, ...requests.withdrawals].map((r) => r.key.toBase58()));
    failing = new Set();
    const chain = Chain.create("http://localhost:8899");
    const fakes = {
      fetchRequests: vi.fn(async () => requests),
      fetchMintInfos: vi.fn(async () => new Map([[USDC.toBase58(), { decimals: 6, tokenProgram: TOKEN_PROGRAM_ID }]])),
      fetchAccountInfos: vi.fn(async (keys: PublicKey[]) => keys.map((k) => (open.has(k.toBase58()) ? ({} as any) : null))),
    };
    Object.assign(chain, fakes);
    (buildTx as any).mockImplementation(async (_c: unknown, _p: unknown, instructions: TransactionInstruction[]) => ({ instructions }));
    (simulateTx as any).mockImplementation(async (_c: unknown, built: { instructions: TransactionInstruction[] }) =>
      requestKeysOf(built.instructions).some((k) => failing.has(k))
        ? { ok: false, logs: [anchorLog("EpochOutflowCapReached")], err: {} }
        : { ok: true, unitsConsumed: 1 },
    );
    (sendTx as any).mockImplementation(async (_c: unknown, _k: unknown, built: { instructions: TransactionInstruction[] }) => {
      for (const k of requestKeysOf(built.instructions)) open.delete(k);
      return { signature: "sig" };
    });
    const alerter = { fire: vi.fn(async () => {}), send: vi.fn(async () => {}) };
    return { deps: { chain, alerter: alerter as any, keypair: Keypair.generate(), dryRun }, fakes, alerter };
  }

  const builtSizes = () => (buildTx as any).mock.calls.map((c: any[]) => c[2].length);

  beforeEach(() => vi.clearAllMocks());

  it("does no RPC when nothing is pending or the vault is paused", async () => {
    const { deps, fakes } = setup({ deposits: [request(4, 1)], withdrawals: [] });
    await settleVault(deps, vaultKey, vaultAccount({ pendingDeposits: new BN(0), pendingWithdrawalShares: new BN(0) }), config);
    await settleVault(deps, vaultKey, vaultAccount({ status: { paused: {} } }), config);
    expect(fakes.fetchRequests).not.toHaveBeenCalled();
  });

  it("settles ready deposits five per transaction", async () => {
    const deposits = Array.from({ length: 7 }, (_, i) => request(4, i));
    const { deps, alerter } = setup({ deposits: [...deposits, request(5, 99)], withdrawals: [] });
    const result = await settleVault(deps, vaultKey, vaultAccount(), config);
    expect(builtSizes()).toEqual([5, 2]);
    expect(sendTx).toHaveBeenCalledTimes(2);
    expect(result.settled).toHaveLength(7);
    expect(result.failed).toEqual([]);
    expect(alerter.fire).toHaveBeenCalledTimes(7);
    expect(alerter.fire).toHaveBeenCalledWith(expect.stringMatching(/^settle:.+:deposit:.+:4$/), null);
    expect(alerter.send).toHaveBeenCalledWith(expect.objectContaining({ level: "info", reason: "settled", message: "deposits=7 withdrawals=0" }));
  });

  it("falls back to one transaction per request when a chunk fails", async () => {
    const deposit = request(4, 0);
    const withdrawals = [request(4, 1), request(4, 2), request(4, 3)];
    const { deps, alerter } = setup({ deposits: [deposit], withdrawals });
    failing.add(withdrawals[1].key.toBase58());
    const result = await settleVault(deps, vaultKey, vaultAccount(), config);
    expect(builtSizes()).toEqual([1, 3, 1, 1, 1]);
    expect(result.settled.map((r) => r.key.toBase58())).toEqual([deposit.key.toBase58(), withdrawals[0].key.toBase58(), withdrawals[2].key.toBase58()]);
    expect(result.failed).toEqual([{ request: expect.objectContaining({ kind: "withdrawal" }), error: "EpochOutflowCapReached" }]);
    expect(alerter.fire).toHaveBeenCalledWith(
      `settle:${vaultKey.toBase58()}:withdrawal:${withdrawals[1].account.authority.toBase58()}:4`,
      expect.objectContaining({ level: "warn", reason: "settle_failed", message: expect.stringContaining("EpochOutflowCapReached") }),
    );
  });

  it("only simulates in dry run", async () => {
    const { deps, alerter } = setup({ deposits: [request(4, 0)], withdrawals: [request(4, 1)] }, true);
    const result = await settleVault(deps, vaultKey, vaultAccount(), config);
    expect(sendTx).not.toHaveBeenCalled();
    expect(simulateTx).toHaveBeenCalledTimes(2);
    expect(result.settled).toHaveLength(2);
    expect(alerter.send).toHaveBeenCalledWith(expect.objectContaining({ reason: "would_settle" }));
  });

  it("reports a request still open after confirmation as not settled", async () => {
    const { deps } = setup({ deposits: [request(4, 0)], withdrawals: [] });
    (sendTx as any).mockImplementation(async () => ({ signature: "sig" }));
    const result = await settleVault(deps, vaultKey, vaultAccount(), config);
    expect(result.failed).toEqual([{ request: expect.objectContaining({ kind: "deposit" }), error: "not_settled" }]);
  });
});
