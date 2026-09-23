import {
  PublicKey,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
} from "@solana/web3.js";
import {
  buildPhoenixRegisterIxs,
  getPhoenixTrader,
  getPhoenixTraderAccount,
  sendPhoenixRegisterIxs,
} from "../../utils/phoenix";
import { VAULT } from "./params";
import { connection, log, SEND, wallet } from "./setup";

/// Not a program instruction: Phoenix's builder API enables the vault's trader capabilities and its
/// onboarder co-signs, the manager only pays the fee. Run after phoenix_initialize_strategy.
describe("hedge_vault", () => {
  it("phoenix_onboard", async () => {
    const traderAccount = getPhoenixTraderAccount(VAULT);
    const trader = await getPhoenixTrader(connection, traderAccount);
    log("Trader", trader);

    if (trader?.isReady) return log("Onboarding", "trader is already ready");

    const built = await buildPhoenixRegisterIxs(VAULT, wallet.publicKey);
    log("Onboarder", built.traderOnboarder);
    log("Includes RegisterTrader", built.includeRegisterTrader);

    const instructions = built.instructions.map(
      (ix: any) =>
        new TransactionInstruction({
          programId: new PublicKey(ix.programId),
          keys: ix.keys.map((key: any) => ({
            ...key,
            pubkey: new PublicKey(key.pubkey),
          })),
          data: Buffer.from(ix.data),
        })
    );

    const { blockhash } = await connection.getLatestBlockhash("finalized");
    const tx = new VersionedTransaction(
      new TransactionMessage({
        payerKey: wallet.publicKey,
        recentBlockhash: blockhash,
        instructions,
      }).compileToV0Message()
    );

    if (!SEND)
      return log(
        "Onboarding",
        "SEND=true signs as fee payer and submits through the Phoenix API"
      );

    const signed = await wallet.signTransaction(tx);
    const sent = await sendPhoenixRegisterIxs(
      Buffer.from(signed.serialize()).toString("base64"),
      VAULT,
      wallet.publicKey
    );
    log("Signature", sent.signature);
  });
});
