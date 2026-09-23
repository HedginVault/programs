import {
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { AccountMeta, PublicKey } from "@solana/web3.js";
import {
  DLMM_EVENT_AUTHORITY,
  DLMM_PROGRAM_ID,
  PHOENIX_GLOBAL_CONFIGURATION,
} from "../../utils/constants";
import { getConfigPda, getStrategyPda } from "../../utils/pda";
import { getPhoenixExchange } from "../../utils/phoenix";
import { DLMM_POSITION, VAULT } from "./params";
import {
  connection,
  fetchTokenProgram,
  log,
  program,
  run,
  wallet,
} from "./setup";

/// Strategy to close, the DLMM position, the Jupiter target mint or getPhoenixTraderAccount(VAULT).
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
  it("vault_close_strategy", async () => {
    const strategy = getStrategyPda(VAULT, PROTOCOL_ACCOUNT);
    const strategyAccount = await program.account.strategy.fetch(strategy);
    log("Strategy", strategyAccount);

    const strategyType = strategyAccount.strategyType as any;
    const remainingAccounts = await getRemainingAccounts(strategyType);

    const ix = await program.methods
      .vaultCloseStrategy()
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

async function getRemainingAccounts(strategyType: any): Promise<AccountMeta[]> {
  if (strategyType.meteoraDlmm) {
    return [
      writable(strategyType.meteoraDlmm.position),
      readonly(DLMM_PROGRAM_ID),
      readonly(DLMM_EVENT_AUTHORITY),
    ];
  }

  if (strategyType.phoenixPerp) {
    const { canonicalMint } = await getPhoenixExchange(connection);

    return [
      readonly(strategyType.phoenixPerp.traderAccount),
      readonly(PHOENIX_GLOBAL_CONFIGURATION),
      writable(getAssociatedTokenAddressSync(canonicalMint, VAULT, true)),
      readonly(TOKEN_PROGRAM_ID),
    ];
  }

  const { targetMint } = strategyType.jupiterSwap;
  const tokenProgram = await fetchTokenProgram(targetMint);

  return [
    writable(
      getAssociatedTokenAddressSync(targetMint, VAULT, true, tokenProgram)
    ),
    readonly(tokenProgram),
  ];
}
