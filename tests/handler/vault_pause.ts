import { getConfigPda } from "../../utils/pda";
import { VAULT } from "./params";
import { program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("vault_pause", async () => {
    const ix = await program.methods
      .vaultPause()
      .accounts({
        guardian: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
      })
      .instruction();

    await run([ix]);
  });
});
