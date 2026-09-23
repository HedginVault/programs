import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { USDC_MINT } from "../../utils/constants";
import { getPhoenixContext } from "./phoenix";
import { VAULT } from "./params";
import { connection, log, program, run } from "./setup";

/// Unwraps canonical tokens a queued phoenix_withdraw_funds delivered after the fact.
describe("hedge_vault", () => {
  it("phoenix_ember_withdraw", async () => {
    const { exchange, accounts } = await getPhoenixContext(VAULT);

    const vaultCanonicalTokenAccount = getAssociatedTokenAddressSync(
      exchange.canonicalMint,
      VAULT,
      true
    );
    log(
      "Canonical balance",
      (await connection.getTokenAccountBalance(vaultCanonicalTokenAccount))
        .value.uiAmountString
    );

    const ix = await program.methods
      .phoenixEmberWithdraw()
      .accountsPartial({
        authority: accounts.authority,
        config: accounts.config,
        vault: VAULT,
        strategy: accounts.strategy,
        usdcMint: USDC_MINT,
        canonicalMint: exchange.canonicalMint,
        globalConfig: accounts.globalConfig,
      })
      .instruction();

    await run([ix]);
  });
});
