import { IdlTypes } from "@coral-xyz/anchor";
import DLMM, { getBinArrayAccountMetasCoverage } from "@meteora-ag/dlmm";
import { createAssociatedTokenAccountIdempotentInstruction, getAssociatedTokenAddressSync } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import { BN } from "bn.js";
import { HedgeVault } from "../../target/types/hedge_vault";
import { DLMM_PROGRAM_ID } from "../../utils/constants";
import { connection, program, wallet } from "./setup";

/// Accounts and remaining accounts shared by the DLMM execute and exit handlers.
export async function getDlmmContext(vault: PublicKey, lbPair: PublicKey, position: PublicKey) {
  const dlmm = await DLMM.create(connection, lbPair);
  const positionAccount = await program.account.positionV2.fetch(position);
  const { lowerBinId, upperBinId } = positionAccount;

  const vaultTokenX = getAssociatedTokenAddressSync(dlmm.tokenX.publicKey, vault, true, dlmm.tokenX.owner);
  const vaultTokenY = getAssociatedTokenAddressSync(dlmm.tokenY.publicKey, vault, true, dlmm.tokenY.owner);

  const createAtaIxs = [
    createAssociatedTokenAccountIdempotentInstruction(
      wallet.publicKey,
      vaultTokenX,
      vault,
      dlmm.tokenX.publicKey,
      dlmm.tokenX.owner,
    ),
    createAssociatedTokenAccountIdempotentInstruction(
      wallet.publicKey,
      vaultTokenY,
      vault,
      dlmm.tokenY.publicKey,
      dlmm.tokenY.owner,
    ),
  ];

  const transferHookX = dlmm.tokenX.transferHookAccountMetas;
  const transferHookY = dlmm.tokenY.transferHookAccountMetas;
  const remainingAccountsInfo: IdlTypes<HedgeVault>["remainingAccountsInfo"] = {
    slices: [
      { accountsType: { transferHookX: {} }, length: transferHookX.length },
      { accountsType: { transferHookY: {} }, length: transferHookY.length },
    ],
  };
  const remainingAccounts = [
    ...transferHookX,
    ...transferHookY,
    ...getBinArrayAccountMetasCoverage(new BN(lowerBinId), new BN(upperBinId), lbPair, DLMM_PROGRAM_ID),
  ];

  const accounts = {
    authority: wallet.publicKey,
    vault,
    position,
    lbPair,
    binArrayBitmapExtension: dlmm.binArrayBitmapExtension?.publicKey ?? null,
    reserveX: dlmm.lbPair.reserveX,
    reserveY: dlmm.lbPair.reserveY,
    tokenXMint: dlmm.tokenX.publicKey,
    tokenYMint: dlmm.tokenY.publicKey,
    tokenXProgram: dlmm.tokenX.owner,
    tokenYProgram: dlmm.tokenY.owner,
  };

  return {
    dlmm,
    lowerBinId,
    upperBinId,
    accounts,
    createAtaIxs,
    remainingAccountsInfo,
    remainingAccounts,
  };
}
