import { getConfigPda, getManagerPda } from "../../utils/pda";
import { MANAGER_AUTHORITY } from "./params";
import { program, requireParam, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("config_remove_manager", async () => {
    requireParam("MANAGER_AUTHORITY", MANAGER_AUTHORITY);

    const ix = await program.methods
      .configRemoveManager()
      .accounts({
        admin: wallet.publicKey,
        config: getConfigPda(),
        manager: getManagerPda(MANAGER_AUTHORITY),
      })
      .instruction();

    await run([ix]);
  });
});
