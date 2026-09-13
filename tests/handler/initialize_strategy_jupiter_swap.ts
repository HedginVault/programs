import { getStrategyPda } from "../../utils/pda";
import { TARGET_MINT, VAULT } from "./params";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("initialize_strategy_jupiter_swap", async () => {
    log("Strategy", getStrategyPda(VAULT, TARGET_MINT));

    const ix = await program.methods
      .initializeStrategyJupiterSwap()
      .accounts({
        authority: wallet.publicKey,
        vault: VAULT,
        destinationMint: TARGET_MINT,
      })
      .instruction();

    await run([ix]);
  });
});
