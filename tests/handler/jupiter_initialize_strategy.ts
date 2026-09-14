import { getConfigPda, getStrategyPda } from "../../utils/pda";
import { TARGET_MINT, VAULT } from "./params";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("jupiter_initialize_strategy", async () => {
    log("Strategy", getStrategyPda(VAULT, TARGET_MINT));

    const ix = await program.methods
      .jupiterInitializeStrategy()
      .accounts({
        authority: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        destinationMint: TARGET_MINT,
      })
      .instruction();

    await run([ix]);
  });
});
