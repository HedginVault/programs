import { BN } from "bn.js";
import { USDC_MINT } from "../../utils/constants";
import { getPhoenixContext } from "./phoenix";
import { DEPOSIT_MINT_DECIMALS, VAULT } from "./params";
import { program, run } from "./setup";

describe("hedge_vault", () => {
  it("phoenix_deposit_funds", async () => {
    const amount = new BN(1 * 10 ** DEPOSIT_MINT_DECIMALS);

    const { exchange, trader, accounts } = await getPhoenixContext(VAULT);
    if (!trader?.isReady)
      throw new Error("Trader is not onboarded, run phoenix-onboard first");

    const ix = await program.methods
      .phoenixDepositFunds(amount)
      .accountsPartial({
        ...accounts,
        usdcMint: USDC_MINT,
        canonicalMint: exchange.canonicalMint,
        globalVault: exchange.globalVault,
      })
      .remainingAccounts(exchange.tail)
      .instruction();

    await run([ix]);
  });
});
