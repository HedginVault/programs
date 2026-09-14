import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { AccountMeta, PublicKey } from "@solana/web3.js";
import { DLMM_EVENT_AUTHORITY, DLMM_PROGRAM_ID } from "../../utils/constants";
import { getConfigPda, getStrategyPda } from "../../utils/pda";
import { DLMM_POSITION, VAULT } from "./params";
import { fetchTokenProgram, log, program, run, wallet } from "./setup";

/// Strategy to close, the DLMM position or the Jupiter target mint.
const PROTOCOL_ACCOUNT = DLMM_POSITION;

const readonly = (pubkey: PublicKey): AccountMeta => ({
  pubkey,
  isSigner: false,
  isWritable: false,
});
const writable = (pubkey: PublicKey): AccountMeta => ({
  pubkey,
  isSigner: false,
  isWritable: true,
});

describe("hedge_vault", () => {
  it("close_strategy", async () => {
    const strategy = getStrategyPda(VAULT, PROTOCOL_ACCOUNT);
    const strategyAccount = await program.account.strategy.fetch(strategy);
    log("Strategy", strategyAccount);

    const strategyType = strategyAccount.strategyType as any;
    const remainingAccounts: AccountMeta[] = strategyType.meteoraDlmm
      ? [
          writable(strategyType.meteoraDlmm.position),
          readonly(DLMM_PROGRAM_ID),
          readonly(DLMM_EVENT_AUTHORITY),
        ]
      : [
          writable(
            getAssociatedTokenAddressSync(
              strategyType.jupiterSwap.targetMint,
              VAULT,
              true,
              await fetchTokenProgram(strategyType.jupiterSwap.targetMint)
            )
          ),
          readonly(
            await fetchTokenProgram(strategyType.jupiterSwap.targetMint)
          ),
        ];

    const ix = await program.methods
      .closeStrategy()
      .accounts({
        authority: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        strategy,
      })
      .remainingAccounts(remainingAccounts)
      .instruction();

    await run([ix]);
  });
});
