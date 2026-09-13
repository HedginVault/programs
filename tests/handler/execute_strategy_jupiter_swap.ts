import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { BN } from "bn.js";
import { getJupiterSwap } from "../../utils/jupiter";
import { getConfigPda, getStrategyPda } from "../../utils/pda";
import { DEPOSIT_MINT_DECIMALS, TARGET_MINT, VAULT } from "./params";
import { connection, fetchTokenProgram, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("execute_strategy_jupiter_swap", async () => {
    const amount = 1 * 10 ** DEPOSIT_MINT_DECIMALS;
    const slippageBps = 100; // 1 %

    const vault = await program.account.vault.fetch(VAULT);
    const depositMintTokenProgram = await fetchTokenProgram(vault.depositMint);
    const targetMintTokenProgram = await fetchTokenProgram(TARGET_MINT);

    const { swapData, remainingAccounts, lookupTables } = await getJupiterSwap(
      connection,
      vault.depositMint,
      TARGET_MINT,
      amount,
      slippageBps,
      VAULT,
    );

    const ix = await program.methods
      .executeStrategyJupiterSwap(swapData, new BN(amount), slippageBps)
      .accounts({
        authority: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        strategy: getStrategyPda(VAULT, TARGET_MINT),
        sourceMint: vault.depositMint,
        destinationMint: TARGET_MINT,
        vaultSourceTokenAccount: getAssociatedTokenAddressSync(
          vault.depositMint,
          VAULT,
          true,
          depositMintTokenProgram,
        ),
        tokenProgram: targetMintTokenProgram,
      })
      .remainingAccounts(remainingAccounts)
      .instruction();

    await run([ix], [], lookupTables);
  });
});
