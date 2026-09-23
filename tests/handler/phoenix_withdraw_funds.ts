import { BN } from "bn.js";
import { USDC_MINT } from "../../utils/constants";
import { getPhoenixContext } from "./phoenix";
import { DEPOSIT_MINT_DECIMALS, VAULT } from "./params";
import { program, run } from "./setup";

/// Canonical units, 1:1 with USDC. Phoenix rejects more than the margin-adjusted withdrawable collateral.
const AMOUNT = 1 * 10 ** DEPOSIT_MINT_DECIMALS;

describe("hedge_vault", () => {
  it("phoenix_withdraw_funds", async () => {
    const { exchange, accounts } = await getPhoenixContext(VAULT);

    const ix = await program.methods
      .phoenixWithdrawFunds(new BN(AMOUNT))
      .accountsPartial({
        ...accounts,
        usdcMint: USDC_MINT,
        canonicalMint: exchange.canonicalMint,
        globalVault: exchange.globalVault,
        perpAssetMap: exchange.perpAssetMap,
        withdrawQueue: exchange.withdrawQueue,
      })
      .remainingAccounts(exchange.tail)
      .instruction();

    await run([ix]);
  });
});
