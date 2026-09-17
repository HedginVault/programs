import { Program, type IdlAccounts, type Provider } from "@coral-xyz/anchor";
import { AccountLayout, MintLayout } from "@solana/spl-token";
import { Connection, LAMPORTS_PER_SOL, PublicKey, type AccountInfo } from "@solana/web3.js";
import idl from "../idl/hedge_vault.json";
import type { HedgeVault } from "../idl/hedge_vault";
import { log } from "./log";
import { createDlmmProgram, type DlmmProgram } from "./valuation/dlmm";

export const EPOCH_DURATION = 14_400;
export const NAV_PRECISION = 1_000_000_000n;
export const TOKEN_PROGRAM_ID = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
export const TOKEN_2022_PROGRAM_ID = new PublicKey("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

export type ConfigAccount = IdlAccounts<HedgeVault>["config"];
export type VaultAccount = IdlAccounts<HedgeVault>["vault"];
export type StrategyAccount = IdlAccounts<HedgeVault>["strategy"];
export type DepositRequestAccount = IdlAccounts<HedgeVault>["depositRequest"];
export type WithdrawalRequestAccount = IdlAccounts<HedgeVault>["withdrawalRequest"];

/** Accounts read together; `slot` is the context slot of the (first) `getMultipleAccountsInfo`. */
export interface Snapshot {
  slot: number;
  accounts: Map<string, AccountInfo<Buffer> | null>;
}

export interface MintInfo {
  decimals: number;
  tokenProgram: PublicKey;
}

const MAX_ACCOUNTS_PER_CALL = 100;
// Strategy layout: 8-byte discriminator, then `vault`.
const STRATEGY_VAULT_OFFSET = 8;
// Request layout: 8-byte discriminator, `authority`, then `vault`.
const REQUEST_VAULT_OFFSET = 40;

export function decodeTokenAmount(info: AccountInfo<Buffer> | null): bigint {
  if (!info || info.data.length < AccountLayout.span) return 0n;
  return AccountLayout.decode(info.data.subarray(0, AccountLayout.span)).amount;
}

export function decodeMint(info: AccountInfo<Buffer> | null): { decimals: number; supply: bigint } | null {
  if (!info || info.data.length < MintLayout.span) return null;
  const raw = MintLayout.decode(info.data.subarray(0, MintLayout.span));
  return { decimals: raw.decimals, supply: raw.supply };
}

export class Chain {
  readonly programId: PublicKey;
  private constructor(
    readonly connection: Connection,
    readonly program: Program<HedgeVault>,
    readonly dlmmProgram: DlmmProgram,
  ) {
    this.programId = program.programId;
  }

  static create(rpcUrl: string, programId?: string): Chain {
    const connection = new Connection(rpcUrl, { commitment: "confirmed", disableRetryOnRateLimit: false });
    // The keeper signs its own transactions; the program only needs a connection to fetch and build.
    const program = new Program<HedgeVault>(
      { ...(idl as HedgeVault), address: programId ?? idl.address },
      { connection } as Provider,
    );
    return new Chain(connection, program, createDlmmProgram(connection));
  }

  private pda(seeds: Buffer[]): PublicKey {
    return PublicKey.findProgramAddressSync(seeds, this.programId)[0];
  }

  configPda(): PublicKey {
    return this.pda([Buffer.from("config")]);
  }

  shareMintPda(vault: PublicKey): PublicKey {
    return this.pda([Buffer.from("share_mint"), vault.toBuffer()]);
  }

  fetchConfig(): Promise<ConfigAccount> {
    return this.program.account.config.fetch(this.configPda());
  }

  async fetchVaults(): Promise<{ key: PublicKey; account: VaultAccount }[]> {
    const all = await this.program.account.vault.all();
    return all
      .map((v) => ({ key: v.publicKey, account: v.account }))
      .sort((a, b) => a.account.id.cmp(b.account.id));
  }

  fetchVault(key: PublicKey): Promise<VaultAccount | null> {
    return this.program.account.vault.fetchNullable(key);
  }

  async fetchStrategies(vault: PublicKey): Promise<{ key: PublicKey; account: StrategyAccount }[]> {
    const rows = await this.program.account.strategy.all([
      { memcmp: { offset: STRATEGY_VAULT_OFFSET, bytes: vault.toBase58() } },
    ]);
    return rows.map((r) => ({ key: r.publicKey, account: r.account })).sort((a, b) => a.account.id - b.account.id);
  }

  async fetchRequests(vault: PublicKey): Promise<{
    deposits: { key: PublicKey; account: DepositRequestAccount }[];
    withdrawals: { key: PublicKey; account: WithdrawalRequestAccount }[];
  }> {
    const filter = [{ memcmp: { offset: REQUEST_VAULT_OFFSET, bytes: vault.toBase58() } }];
    const [deposits, withdrawals] = await Promise.all([this.program.account.depositRequest.all(filter), this.program.account.withdrawalRequest.all(filter)]);
    return {
      deposits: deposits.map((r) => ({ key: r.publicKey, account: r.account })),
      withdrawals: withdrawals.map((r) => ({ key: r.publicKey, account: r.account })),
    };
  }

  async fetchAccountInfos(keys: PublicKey[]): Promise<(AccountInfo<Buffer> | null)[]> {
    const out: (AccountInfo<Buffer> | null)[] = [];
    for (let i = 0; i < keys.length; i += MAX_ACCOUNTS_PER_CALL) {
      out.push(...(await this.connection.getMultipleAccountsInfo(keys.slice(i, i + MAX_ACCOUNTS_PER_CALL))));
    }
    return out;
  }

  /**
   * All keys in one `getMultipleAccountsInfo`, so every account is read at the same slot. Above 100
   * keys, later chunks are pinned to the first chunk's slot with `minContextSlot`.
   */
  async fetchSnapshot(keys: PublicKey[]): Promise<Snapshot> {
    const unique = [...new Map(keys.map((k) => [k.toBase58(), k])).values()];
    const accounts = new Map<string, AccountInfo<Buffer> | null>();
    let slot: number | undefined;
    for (let i = 0; i < unique.length; i += MAX_ACCOUNTS_PER_CALL) {
      const chunk = unique.slice(i, i + MAX_ACCOUNTS_PER_CALL);
      const { context, value } = await this.connection.getMultipleAccountsInfoAndContext(chunk, slot === undefined ? undefined : { minContextSlot: slot });
      if (slot === undefined) slot = context.slot;
      else if (context.slot !== slot) log.warn("snapshot chunk read at a later slot", { slot, chunkSlot: context.slot });
      chunk.forEach((k, j) => accounts.set(k.toBase58(), value[j]));
    }
    return { slot: slot ?? 0, accounts };
  }

  async fetchMintInfos(mints: PublicKey[]): Promise<Map<string, MintInfo>> {
    const unique = [...new Map(mints.map((m) => [m.toBase58(), m])).values()];
    const infos = await this.fetchAccountInfos(unique);
    const out = new Map<string, MintInfo>();
    infos.forEach((info, i) => {
      const mint = decodeMint(info);
      if (!info || !mint) throw new Error(`mint ${unique[i].toBase58()} not found`);
      out.set(unique[i].toBase58(), { decimals: mint.decimals, tokenProgram: info.owner });
    });
    return out;
  }

  async fetchSolBalance(key: PublicKey): Promise<number> {
    return (await this.connection.getBalance(key)) / LAMPORTS_PER_SOL;
  }
}
