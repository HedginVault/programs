import { getConfigPda } from "../../utils/pda";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("accept_admin", async () => {
    const ix = await program.methods
      .acceptAdmin()
      .accounts({ pendingAdmin: wallet.publicKey, config: getConfigPda() })
      .instruction();

    await run([ix]);

    log("Config", await program.account.config.fetch(getConfigPda()));
  });
});
