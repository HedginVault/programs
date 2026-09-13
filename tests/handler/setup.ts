import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  AddressLookupTableAccount,
  PublicKey,
  Signer,
  TransactionInstruction,
} from "@solana/web3.js";
import { HedgeVault } from "../../target/types/hedge_vault";
import { createTransaction, sendTransaction, simulateTransaction } from "../../utils/transaction";

anchor.setProvider(anchor.AnchorProvider.env());

export const provider = anchor.AnchorProvider.env();
export const connection = provider.connection;
export const wallet = provider.wallet;
export const program = anchor.workspace.hedgeVault as Program<HedgeVault>;

/// Set SEND=true to land the transaction, otherwise it is only simulated.
export const SEND = process.env.SEND === "true";

export async function run(
  instructions: TransactionInstruction[],
  signers: Signer[] = [],
  lookupTables: AddressLookupTableAccount[] = [],
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
  console.log(`${label}:`, value instanceof PublicKey ? value.toBase58() : value);
}
