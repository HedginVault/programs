import * as anchor from "@coral-xyz/anchor";
import {
  AddressLookupTableAccount,
  ComputeBudgetProgram,
  Signer,
  TransactionInstruction,
  VersionedTransaction,
} from "@solana/web3.js";

export async function createTransaction(
  provider: anchor.AnchorProvider,
  instructions: TransactionInstruction[],
  lookupTables: AddressLookupTableAccount[] = [],
): Promise<VersionedTransaction> {
  const { blockhash } = await provider.connection.getLatestBlockhash();
  const message = new anchor.web3.TransactionMessage({
    payerKey: provider.wallet.publicKey,
    recentBlockhash: blockhash,
    instructions: [
      ComputeBudgetProgram.setComputeUnitLimit({ units: 1_400_000 }),
      ...instructions,
    ],
  }).compileToV0Message(lookupTables);

  return new anchor.web3.VersionedTransaction(message);
}

export async function simulateTransaction(
  provider: anchor.AnchorProvider,
  tx: VersionedTransaction,
): Promise<boolean> {
  const simulation = await provider.connection.simulateTransaction(tx, {
    sigVerify: false,
    replaceRecentBlockhash: true,
  });

  console.log("Simulation result:", simulation.value.err ? "Error" : "Success");
  console.log("Compute units:", simulation.value.unitsConsumed);
  if (simulation.value.err) console.error("Simulation error:", simulation.value.err);
  simulation.value.logs?.forEach((log) => console.log(log));

  return !simulation.value.err;
}

export async function sendTransaction(
  provider: anchor.AnchorProvider,
  tx: VersionedTransaction,
  signers: Signer[] = [],
): Promise<string> {
  const signature = await provider.sendAndConfirm(tx, signers, { skipPreflight: true });
  console.log("Transaction confirmed:", signature);

  return signature;
}
