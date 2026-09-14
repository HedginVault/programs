import { getConfigPda } from "../../utils/pda";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("admin_accept", async () => {
    const ix = await program.methods
      .adminAccept()
      .accounts({ pendingAdmin: wallet.publicKey, config: getConfigPda() })
      .instruction();

    await run([ix]);

    log("Config", await program.account.config.fetch(getConfigPda()));
  });
});
