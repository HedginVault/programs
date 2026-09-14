import { getConfigPda } from "../../utils/pda";
import { VAULT } from "./params";
import { program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("pause_vault", async () => {
    const ix = await program.methods
      .pauseVault()
      .accounts({
        guardian: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
      })
      .instruction();

    await run([ix]);
  });
});
