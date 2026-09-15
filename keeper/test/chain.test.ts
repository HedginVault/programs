import { AccountLayout, MintLayout } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import { describe, expect, it } from "vitest";
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
