import { getConfigPda, getManagerPda } from "../../utils/pda";
import { MANAGER_AUTHORITY } from "./params";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("config_add_manager", async () => {
    log("Manager", getManagerPda(MANAGER_AUTHORITY));

    const ix = await program.methods
      .configAddManager()
      .accounts({
        admin: wallet.publicKey,
        config: getConfigPda(),
        authority: MANAGER_AUTHORITY,
      })
      .instruction();

    await run([ix]);
  });
});
