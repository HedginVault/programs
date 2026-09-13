import { getShareMintPda } from "../../utils/pda";
import { VAULT } from "./params";
import { fetchTokenProgram, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("close_vault", async () => {
    const vault = await program.account.vault.fetch(VAULT);

    const ix = await program.methods
      .closeVault()
      .accounts({
        authority: wallet.publicKey,
        vault: VAULT,
        depositMint: vault.depositMint,
        shareMint: getShareMintPda(VAULT),
        depositMintTokenProgram: await fetchTokenProgram(vault.depositMint),
      })
      .instruction();

    await run([ix]);
  });
});
