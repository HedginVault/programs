import { PublicKey } from "@solana/web3.js";
import {
  PHOENIX_GLOBAL_CONFIGURATION,
  PHOENIX_LOG_AUTHORITY,
  PHOENIX_PROGRAM_ID,
} from "../../utils/constants";
import { getConfigPda, getStrategyPda } from "../../utils/pda";
import {
  getPhoenixExchange,
  getPhoenixTrader,
  getPhoenixTraderAccount,
} from "../../utils/phoenix";
import { connection, log, wallet } from "./setup";

/// Accounts shared by every Phoenix handler, plus the exchange accounts and the dynamic tail.
export async function getPhoenixContext(vault: PublicKey) {
  const exchange = await getPhoenixExchange(connection);
  const traderAccount = getPhoenixTraderAccount(vault);
  const trader = await getPhoenixTrader(connection, traderAccount);

  log("Trader account", traderAccount);
  log("Trader", trader);

  return {
    exchange,
    traderAccount,
    trader,
    accounts: {
      authority: wallet.publicKey,
      config: getConfigPda(),
      vault,
      strategy: getStrategyPda(vault, traderAccount),
      traderAccount,
      globalConfig: PHOENIX_GLOBAL_CONFIGURATION,
      logAuthority: PHOENIX_LOG_AUTHORITY,
      phoenixProgram: PHOENIX_PROGRAM_ID,
    },
  };
}
