import {
  createAssociatedTokenAccountIdempotentInstruction,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { getConfigPda, getDepositRequestPda } from "../../utils/pda";
import { REQUEST_AUTHORITY, VAULT } from "./params";
import { fetchTokenProgram, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("deposit_request_reject", async () => {
    const vault = await program.account.vault.fetch(VAULT);
    const depositMintTokenProgram = await fetchTokenProgram(vault.depositMint);

    // the refund goes to the depositor's ATA, recreated here in case it was closed
    const createAta = createAssociatedTokenAccountIdempotentInstruction(
      wallet.publicKey,
      getAssociatedTokenAddressSync(
        vault.depositMint,
        REQUEST_AUTHORITY,
        true,
        depositMintTokenProgram
      ),
      REQUEST_AUTHORITY,
      vault.depositMint,
      depositMintTokenProgram
    );

    const ix = await program.methods
      .depositRequestReject()
      .accounts({
        admin: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        depositor: REQUEST_AUTHORITY,
        depositRequest: getDepositRequestPda(VAULT, REQUEST_AUTHORITY),
        depositMint: vault.depositMint,
        depositMintTokenProgram,
      })
      .instruction();

    await run([createAta, ix]);
  });
});
