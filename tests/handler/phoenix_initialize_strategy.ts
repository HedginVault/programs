import { getPhoenixContext } from "./phoenix";
import { VAULT } from "./params";
import { log, program, run } from "./setup";

describe("hedge_vault", () => {
  it("phoenix_initialize_strategy", async () => {
    const { exchange, accounts } = await getPhoenixContext(VAULT);
    log("Strategy", accounts.strategy);

    const ix = await program.methods
      .phoenixInitializeStrategy()
      .accountsPartial({
        ...accounts,
        canonicalMint: exchange.canonicalMint,
      })
      .instruction();

    await run([ix]);
  });
});
