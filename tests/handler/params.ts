import { PublicKey } from "@solana/web3.js";
import { USDC_MINT } from "../../utils/constants";

// Addresses under test, update before running a handler.

export const DEPOSIT_MINT = USDC_MINT;
export const DEPOSIT_MINT_DECIMALS = 6;

/// Existing vault, required by every handler after vault_initialize.
export const VAULT = new PublicKey("11111111111111111111111111111111");

/// Manager wallet to whitelist with config_add_manager / config_remove_manager.
export const MANAGER_AUTHORITY = new PublicKey("11111111111111111111111111111111");

/// Owner of a pending request for the *_request_resolve handlers.
export const REQUEST_AUTHORITY = new PublicKey("11111111111111111111111111111111");

/// Jupiter swap strategy target.
export const TARGET_MINT = new PublicKey("So11111111111111111111111111111111111111112");

/// Meteora DLMM pool and the position registered as a strategy.
export const LB_PAIR = new PublicKey("11111111111111111111111111111111");
export const DLMM_POSITION = new PublicKey("11111111111111111111111111111111");
