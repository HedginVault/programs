import { getConfigPda, getManagerPda } from "../../utils/pda";
import { MANAGER_AUTHORITY } from "./params";
import { program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("remove_manager", async () => {
    const ix = await program.methods
      .removeManager()
      .accounts({
        admin: wallet.publicKey,
        config: getConfigPda(),
        manager: getManagerPda(MANAGER_AUTHORITY),
      })
      .instruction();

    await run([ix]);
  });
});
