import { StrategyType, toStrategyParameters } from "@meteora-ag/dlmm";
import { BN } from "bn.js";
import { DLMM_EVENT_AUTHORITY } from "../../utils/constants";
import { getConfigPda, getStrategyPda } from "../../utils/pda";
import { getDlmmContext } from "./dlmm";
import { DLMM_POSITION, LB_PAIR, VAULT } from "./params";
import { log, program, run } from "./setup";

describe("hedge_vault", () => {
  it("execute_strategy_meteora_dlmm", async () => {
    const amountX = new BN(0);
    const amountY = new BN(1_000_000);
    const maxActiveBinSlippage = 50;

    const { dlmm, lowerBinId, upperBinId, accounts, createAtaIxs, remainingAccountsInfo, remainingAccounts } =
      await getDlmmContext(VAULT, LB_PAIR, DLMM_POSITION);

    const activeId = (await dlmm.getActiveBin()).binId;
    log("Bin range", `${lowerBinId} .. ${upperBinId} (active ${activeId})`);

    const params = {
      liquidityParameter: {
        amountX,
        amountY,
        activeId,
        maxActiveBinSlippage,
        strategyParameters: toStrategyParameters({
          minBinId: lowerBinId,
          maxBinId: upperBinId,
          strategyType: StrategyType.Spot,
        }),
      },
      remainingAccountsInfo,
    };

    const ix = await program.methods
      .executeStrategyMeteoraDlmm(params)
      .accounts({
        ...accounts,
        config: getConfigPda(),
        strategy: getStrategyPda(VAULT, DLMM_POSITION),
        eventAuthority: DLMM_EVENT_AUTHORITY,
      })
      .remainingAccounts(remainingAccounts)
      .instruction();

    await run([...createAtaIxs, ix]);
  });
});
