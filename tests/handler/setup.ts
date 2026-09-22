import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  AddressLookupTableAccount,
  PublicKey,
  Signer,
  TransactionInstruction,
} from "@solana/web3.js";
import { HedgeVault } from "../../target/types/hedge_vault";
import {
  createTransaction,
  sendTransaction,
  simulateTransaction,
} from "../../utils/transaction";

/// Confirmed commitment for sends and reads, so a post-transaction fetch never sees a stale account.
export const provider = new anchor.AnchorProvider(
  new anchor.web3.Connection(process.env.ANCHOR_PROVIDER_URL!, "confirmed"),
  anchor.AnchorProvider.env().wallet,
  { commitment: "confirmed", preflightCommitment: "confirmed" }
);
anchor.setProvider(provider);

export const connection = provider.connection;
export const wallet = provider.wallet;
export const program = anchor.workspace.hedgeVault as Program<HedgeVault>;

/// Set SEND=true to land the transaction, otherwise it is only simulated.
export const SEND = process.env.SEND === "true";

export async function run(
  instructions: TransactionInstruction[],
  signers: Signer[] = [],
  lookupTables: AddressLookupTableAccount[] = []
) {
  const tx = await createTransaction(provider, instructions, lookupTables);
  const simulated = await simulateTransaction(provider, tx);

  if (!simulated) throw new Error("Simulation failed");
  if (!SEND) return;

  return sendTransaction(provider, tx, signers);
}

export async function fetchTokenProgram(mint: PublicKey) {
  return (await connection.getAccountInfo(mint))!.owner;
}

export function log(label: string, value: unknown) {
  console.log(
    `${label}:`,
    value instanceof PublicKey ? value.toBase58() : value
  );
}

/// Throws when a params.ts placeholder was left at the default pubkey.
export function requireParam(label: string, key: PublicKey) {
  if (key.equals(PublicKey.default)) {
    throw new Error(
      `${label} in tests/handler/params.ts is still the placeholder`
    );
  }
}
