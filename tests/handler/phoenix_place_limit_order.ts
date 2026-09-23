import { BN } from "bn.js";
import { HAWKEYE_PROGRAM_ID } from "../../utils/constants";
import { getPhoenixMarket } from "../../utils/phoenix";
import { getPhoenixContext } from "./phoenix";
import { PHOENIX_SYMBOL, VAULT } from "./params";
import { log, program, run } from "./setup";

/// Limit price in market ticks, read it off the Phoenix orderbook before running.
const PRICE_IN_TICKS = 0;
const NUM_BASE_LOTS = 10;

describe("hedge_vault", () => {
  it("phoenix_place_limit_order", async () => {
    if (PRICE_IN_TICKS === 0)
      throw new Error("Set PRICE_IN_TICKS in phoenix_place_limit_order.ts");

    const { exchange, trader, accounts } = await getPhoenixContext(VAULT);
    if (!trader?.isHot)
      log("Warning", "Phoenix rejects resting orders from a cold trader");

    const market = await getPhoenixMarket(PHOENIX_SYMBOL);
    log("Market", market);

    const params = {
      side: { bid: {} },
      priceInTicks: new BN(PRICE_IN_TICKS),
      numBaseLots: new BN(NUM_BASE_LOTS),
      postOnly: true,
      slide: true,
      selfTradeBehavior: { abort: {} },
      matchLimit: null,
      clientOrderId: new BN(Date.now()),
      lastValidSlot: null,
      reduceOnly: false,
      cancelExisting: false,
    };

    const ix = await program.methods
      .phoenixPlaceLimitOrder(params)
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
