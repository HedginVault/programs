use anchor_lang::prelude::*;

#[error_code]
pub enum HedgeVaultError {
    // Generic
    #[msg("Invalid Program")]
    InvalidProgramId,
    #[msg("Pubkey cannot be the default pubkey")]
    InvalidPubkey,
    #[msg("Transfer amount must be greater than zero")]
    InvalidTransferAmount,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Math conversion failed")]
    ConversionFailed,
    #[msg("Basis points cannot exceed 10,000")]
    InvalidBasisPoints,
    #[msg("Invalid amount of remaining accounts passed")]
    InvalidRemainingAccounts,

    // Token & Balances
    #[msg("Invalid Token Account Mint")]
    InvalidTokenAccountMint,
    #[msg("Invalid Token Account Owner")]
    InvalidTokenAccountOwner,
    #[msg("Insufficient funds")]
    InsufficientFunds,

    // Config
    #[msg("Config address does not match")]
    InvalidConfig,
    #[msg("Admin does not match")]
    InvalidAdmin,
    #[msg("NAV updater does not match")]
    InvalidNavUpdater,
    #[msg("Treasury authority does not match")]
    InvalidTreasuryAuthority,
    #[msg("Guardian does not match")]
    InvalidGuardian,
    #[msg("Protocol is paused or in reduce-only status")]
    ProtocolNotOperational,
    #[msg("Protocol is not in a withdrawable status")]
    ProtocolNotWithdrawable,

    // Manager
    #[msg("Manager address does not match")]
    InvalidManager,
    #[msg("Manager authority does not match")]
    InvalidManagerAuthority,

    // Vault
    #[msg("Vault address does not match")]
    InvalidVault,
    #[msg("Vault authority does not match")]
    InvalidVaultAuthority,
    #[msg("Vault is paused or in reduce-only status")]
    VaultNotOperational,
    #[msg("Vault is not in a withdrawable status")]
    VaultNotWithdrawable,
    #[msg("Deposit cap for the vault has been reached")]
    DepositCapReached,
    #[msg("Deposit mint does not match")]
    InvalidDepositMint,
    #[msg("Share mint does not match")]
    InvalidShareMint,
    #[msg("Vault cannot be closed until all shares are redeemed")]
    VaultHasOutstandingShares,
    #[msg("Vault cannot be closed until all pending requests are resolved")]
    VaultHasPendingRequests,
    #[msg("Vault cannot be closed until all fee shares are claimed")]
    VaultHasUnclaimedFees,
    #[msg("Shares amount must be greater than zero")]
    InvalidSharesAmount,
    #[msg("Unclaimed fee shares is 0")]
    NoFeeToClaim,

    // NAV
    #[msg("NAV has already been updated for the current epoch")]
    NavAlreadyUpdatedThisEpoch,
    #[msg("Total fee cannot exceed total assets")]
    FeeExceedsTotalAssets,
    #[msg("NAV change exceeds the max deviation allowed per update")]
    NavDeviationExceeded,
    #[msg("Total assets cannot be lower than the vault's idle balance")]
    TotalAssetsBelowIdleBalance,
    #[msg("Withdrawals resolved this epoch have reached the outflow cap")]
    EpochOutflowCapReached,

    // Requests
    #[msg("Request address does not match")]
    InvalidRequest,
    #[msg("Request authority does not match")]
    InvalidRequestAuthority,
    #[msg("Request vault does not match")]
    InvalidRequestVault,
    #[msg("A request from a previous epoch is pending resolution")]
    PendingRequestNotResolved,
    #[msg("Request cannot be resolved until NAV is updated in a later epoch")]
    RequestNotResolvable,
    #[msg("Request can no longer be cancelled, it must be resolved")]
    RequestNotCancellable,

    // Strategy
    #[msg("Strategy address does not match")]
    InvalidStrategy,
    #[msg("Strategy type is invalid for this operation")]
    InvalidStrategyType,
    #[msg("Destination mint does not match")]
    InvalidDestinationMint,
    #[msg("Source mint does not match strategy destination mint")]
    InvalidSourceMint,

    // Jupiter
    #[msg("Target mint of token account does not match")]
    InvalidTargetMintTokenAccount,

    // Meteora
    #[msg("Position address does not match")]
    InvalidPosition,
}
