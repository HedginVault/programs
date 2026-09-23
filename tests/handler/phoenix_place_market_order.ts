import { BN } from "bn.js";
import { HAWKEYE_PROGRAM_ID } from "../../utils/constants";
import { getPhoenixMarket } from "../../utils/phoenix";
import { getPhoenixContext } from "./phoenix";
import { PHOENIX_SYMBOL, VAULT } from "./params";
import { log, program, run } from "./setup";

/// Base lots are 10^-baseLotsDecimals of the asset, e.g. 10 lots = 0.1 SOL.
const NUM_BASE_LOTS = 10;
/// true closes a position: flip the side and size it to the open position.
const REDUCE_ONLY = false;

describe("hedge_vault", () => {
  it("phoenix_place_market_order", async () => {
    const { exchange, accounts } = await getPhoenixContext(VAULT);
    const market = await getPhoenixMarket(PHOENIX_SYMBOL);
    log("Market", market);

    const params = {
      side: { bid: {} },
      priceInTicks: null,
      numBaseLots: new BN(NUM_BASE_LOTS),
      numQuoteLots: null,
      minBaseLotsToFill: new BN(0),
      minQuoteLotsToFill: new BN(0),
      selfTradeBehavior: { abort: {} },
      matchLimit: null,
      clientOrderId: new BN(Date.now()),
      lastValidSlot: null,
      reduceOnly: REDUCE_ONLY,
      cancelExisting: false,
    };

    const ix = await program.methods
      .phoenixPlaceMarketOrder(params)
      .accountsPartial({
        ...accounts,
        perpAssetMap: exchange.perpAssetMap,
        orderbook: market.orderbook,
        splineCollection: market.splineCollection,
        hawkeyeProgram: HAWKEYE_PROGRAM_ID,
      })
      .remainingAccounts(exchange.tail)
      .instruction();

    await run([ix]);
  });
});
