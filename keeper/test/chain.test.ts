import { AccountLayout, MintLayout } from "@solana/spl-token";
import { PublicKey, type AccountInfo } from "@solana/web3.js";
import { describe, expect, it, vi } from "vitest";
import { Chain, decodeMint, decodeTokenAmount } from "../src/chain";

describe("Chain pdas", () => {
  const chain = Chain.create("http://localhost:8899");
  it("derives the config pda from the program id", () => {
    const [expected] = PublicKey.findProgramAddressSync([Buffer.from("config")], chain.programId);
    expect(chain.configPda().equals(expected)).toBe(true);
    expect(chain.programId.toBase58()).toBe("r2ahBQ6gbPCJ9FxBymYcXuwXi8NmenRry7SE7QR7FAt");
  });
  it("derives the share mint pda", () => {
    const vault = PublicKey.unique();
    const [expected] = PublicKey.findProgramAddressSync([Buffer.from("share_mint"), vault.toBuffer()], chain.programId);
    expect(chain.shareMintPda(vault).equals(expected)).toBe(true);
  });
  it("honours a program id override", () => {
    const id = PublicKey.unique().toBase58();
    expect(Chain.create("http://localhost:8899", id).programId.toBase58()).toBe(id);
  });
});

describe("decoders", () => {
  it("decodes a token amount and treats missing accounts as zero", () => {
    const data = Buffer.alloc(AccountLayout.span);
    AccountLayout.encode(
      { mint: PublicKey.default, owner: PublicKey.default, amount: 12345n, delegateOption: 0, delegate: PublicKey.default, state: 1, isNativeOption: 0, isNative: 0n, delegatedAmount: 0n, closeAuthorityOption: 0, closeAuthority: PublicKey.default },
      data,
    );
    expect(decodeTokenAmount({ data, executable: false, lamports: 0, owner: PublicKey.default })).toBe(12345n);
    expect(decodeTokenAmount(null)).toBe(0n);
  });
  it("decodes mint decimals and rejects short data", () => {
    const data = Buffer.alloc(MintLayout.span);
    MintLayout.encode({ mintAuthorityOption: 0, mintAuthority: PublicKey.default, supply: 7n, decimals: 6, isInitialized: true, freezeAuthorityOption: 0, freezeAuthority: PublicKey.default }, data);
    expect(decodeMint({ data, executable: false, lamports: 0, owner: PublicKey.default })).toEqual({ decimals: 6, supply: 7n });
    expect(decodeMint({ data: Buffer.alloc(3), executable: false, lamports: 0, owner: PublicKey.default })).toBeNull();
  });
});

describe("fetchSnapshot", () => {
  function chainWith(slots: number[]) {
    const chain = Chain.create("http://localhost:8899");
    const call = vi.fn(async (keys: PublicKey[], _config?: unknown) => ({
      context: { slot: slots[call.mock.calls.length - 1] },
      value: keys.map((k, i) => (i === 0 ? ({ data: Buffer.from([1]), owner: k, lamports: 1, executable: false } as AccountInfo<Buffer>) : null)),
    }));
    (chain.connection as any).getMultipleAccountsInfoAndContext = call;
    return { chain, call };
  }

  it("reads a small snapshot in exactly one call", async () => {
    const { chain, call } = chainWith([77]);
    const keys = [PublicKey.unique(), PublicKey.unique()];
    const snap = await chain.fetchSnapshot([...keys, keys[0]]);
    expect(call).toHaveBeenCalledTimes(1);
    expect(call.mock.calls[0][0]).toHaveLength(2);
    expect(snap.slot).toBe(77);
    expect(snap.accounts.get(keys[0].toBase58())?.owner.equals(keys[0])).toBe(true);
    expect(snap.accounts.get(keys[1].toBase58())).toBeNull();
  });

  it("pins chunks after the first 100 keys to the first slot", async () => {
    const { chain, call } = chainWith([500, 500]);
    const snap = await chain.fetchSnapshot(Array.from({ length: 150 }, () => PublicKey.unique()));
    expect(call.mock.calls.map((c) => c[0].length)).toEqual([100, 50]);
    expect(call.mock.calls[0][1]).toBeUndefined();
    expect(call.mock.calls[1][1]).toEqual({ minContextSlot: 500 });
    expect(snap.slot).toBe(500);
    expect(snap.accounts.size).toBe(150);
  });
});
