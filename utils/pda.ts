import { PublicKey } from "@solana/web3.js";
import { BN } from "bn.js";
import { HEDGE_VAULT_PROGRAM_ID } from "./constants";

const find = (seeds: Buffer[]) =>
  PublicKey.findProgramAddressSync(seeds, HEDGE_VAULT_PROGRAM_ID)[0];

export const getConfigPda = () => find([Buffer.from("config")]);

export const getManagerPda = (authority: PublicKey) =>
  find([Buffer.from("manager"), authority.toBuffer()]);

export const getVaultPda = (id: InstanceType<typeof BN> | number) =>
  find([Buffer.from("vault"), new BN(id).toArrayLike(Buffer, "le", 8)]);

export const getShareMintPda = (vault: PublicKey) =>
  find([Buffer.from("share_mint"), vault.toBuffer()]);

export const getDepositEscrowPda = (vault: PublicKey) =>
  find([Buffer.from("deposit_escrow"), vault.toBuffer()]);

export const getShareEscrowPda = (vault: PublicKey) =>
  find([Buffer.from("share_escrow"), vault.toBuffer()]);

export const getStrategyPda = (vault: PublicKey, protocolAccount: PublicKey) =>
  find([Buffer.from("strategy"), vault.toBuffer(), protocolAccount.toBuffer()]);

export const getDepositRequestPda = (vault: PublicKey, authority: PublicKey) =>
  find([Buffer.from("deposit_request"), vault.toBuffer(), authority.toBuffer()]);

export const getWithdrawalRequestPda = (vault: PublicKey, authority: PublicKey) =>
  find([Buffer.from("withdrawal_request"), vault.toBuffer(), authority.toBuffer()]);
