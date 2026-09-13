import { getDepositRequestPda } from "../../utils/pda";
import { VAULT } from "./params";
import { fetchTokenProgram, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("cancel_deposit_request", async () => {
    const vault = await program.account.vault.fetch(VAULT);

    const ix = await program.methods
      .cancelDepositRequest()
      .accounts({
        depositor: wallet.publicKey,
        vault: VAULT,
        depositRequest: getDepositRequestPda(VAULT, wallet.publicKey),
        depositMint: vault.depositMint,
        depositMintTokenProgram: await fetchTokenProgram(vault.depositMint),
      })
      .instruction();

    await run([ix]);
  });
});
