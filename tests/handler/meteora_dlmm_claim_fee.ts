import { DLMM_EVENT_AUTHORITY, MEMO_PROGRAM_ID } from "../../utils/constants";
import { getConfigPda, getStrategyPda } from "../../utils/pda";
import { getDlmmContext } from "./dlmm";
import { DLMM_POSITION, LB_PAIR, VAULT } from "./params";
import { program, run } from "./setup";

describe("hedge_vault", () => {
  it("meteora_dlmm_claim_fee", async () => {
    const config = await program.account.config.fetch(getConfigPda());

    const { accounts, remainingAccountsInfo, remainingAccounts } = await getDlmmContext(
      VAULT,
      LB_PAIR,
      DLMM_POSITION,
    );

    const ix = await program.methods
      .meteoraDlmmClaimFee(remainingAccountsInfo)
      .accounts({
        ...accounts,
        config: getConfigPda(),
        strategy: getStrategyPda(VAULT, DLMM_POSITION),
        treasuryAuthority: config.treasuryAuthority,
        memoProgram: MEMO_PROGRAM_ID,
        eventAuthority: DLMM_EVENT_AUTHORITY,
      })
      .remainingAccounts(remainingAccounts)
      .instruction();

    await run([ix]);
  });
});
