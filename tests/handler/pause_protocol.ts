import { getConfigPda } from "../../utils/pda";
import { program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("pause_protocol", async () => {
    const ix = await program.methods
      .pauseProtocol()
      .accounts({ guardian: wallet.publicKey, config: getConfigPda() })
      .instruction();

    await run([ix]);
  });
});
