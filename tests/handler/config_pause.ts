import { getConfigPda } from "../../utils/pda";
import { program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("config_pause", async () => {
    const ix = await program.methods
      .configPause()
      .accounts({ guardian: wallet.publicKey, config: getConfigPda() })
      .instruction();

    await run([ix]);
  });
});
