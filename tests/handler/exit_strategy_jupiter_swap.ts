import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { BN } from "bn.js";
import { getJupiterSwap } from "../../utils/jupiter";
import { getConfigPda, getStrategyPda } from "../../utils/pda";
import { TARGET_MINT, VAULT } from "./params";
import { connection, fetchTokenProgram, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("exit_strategy_jupiter_swap", async () => {
    const slippageBps = 100; // 1 %

    const vault = await program.account.vault.fetch(VAULT);
    const depositMintTokenProgram = await fetchTokenProgram(vault.depositMint);
    const targetMintTokenProgram = await fetchTokenProgram(TARGET_MINT);

    const vaultTargetTokenAccount = getAssociatedTokenAddressSync(
      TARGET_MINT,
      VAULT,
      true,
      targetMintTokenProgram,
    );
    const amount = Number((await connection.getTokenAccountBalance(vaultTargetTokenAccount)).value.amount);

    const { swapData, remainingAccounts, lookupTables } = await getJupiterSwap(
      connection,
      TARGET_MINT,
      vault.depositMint,
      amount,
      slippageBps,
      VAULT,
    );

    const ix = await program.methods
      .exitStrategyJupiterSwap(swapData, new BN(amount), slippageBps)
      .accounts({
        authority: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        strategy: getStrategyPda(VAULT, TARGET_MINT),
        sourceMint: TARGET_MINT,
        destinationMint: vault.depositMint,
        vaultSourceTokenAccount: vaultTargetTokenAccount,
        destinationTokenProgram: depositMintTokenProgram,
      })
      .remainingAccounts(remainingAccounts)
      .instruction();

    await run([ix], [], lookupTables);
  });
});
