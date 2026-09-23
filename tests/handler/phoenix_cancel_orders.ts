import { getPhoenixMarket } from "../../utils/phoenix";
import { getPhoenixContext } from "./phoenix";
import { PHOENIX_SYMBOL, VAULT } from "./params";
import { log, program, run } from "./setup";

describe("hedge_vault", () => {
  it("phoenix_cancel_orders", async () => {
    const { exchange, accounts } = await getPhoenixContext(VAULT);
    const market = await getPhoenixMarket(PHOENIX_SYMBOL);
    log("Market", market);

    // or { upTo: { side, numOrdersToCancel, tickLimit } } / { byId: { orders } } from PhoenixOrderPlaced
    const mode = { all: {} };

    const ix = await program.methods
      .phoenixCancelOrders(mode)
      .accountsPartial({
        ...accounts,
        perpAssetMap: exchange.perpAssetMap,
        orderbook: market.orderbook,
        splineCollection: market.splineCollection,
      })
      .remainingAccounts(exchange.tail)
      .instruction();

    await run([ix]);
  });
});
