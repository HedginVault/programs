pub const CONFIG: &[u8] = b"config";
pub const MANAGER: &[u8] = b"manager";
pub const VAULT: &[u8] = b"vault";
pub const SHARE_MINT: &[u8] = b"share_mint";
pub const DEPOSIT_ESCROW: &[u8] = b"deposit_escrow";
pub const SHARE_ESCROW: &[u8] = b"share_escrow";
pub const STRATEGY: &[u8] = b"strategy";
pub const DEPOSIT_REQUEST: &[u8] = b"deposit_request";
pub const WITHDRAWAL_REQUEST: &[u8] = b"withdrawal_request";

#[macro_export]
macro_rules! config_seeds {
    ($bump: expr) => {
        &[CONFIG, &[$bump]]
    };
}

#[macro_export]
macro_rules! manager_seeds {
    ($authority: expr, $bump: expr) => {
        &[MANAGER, $authority.as_ref(), &[$bump]]
    };
}

#[macro_export]
macro_rules! vault_seeds {
    ($vault_id: expr, $bump: expr) => {
        &[VAULT, $vault_id.as_ref(), &[$bump]]
    };
}

#[macro_export]
macro_rules! strategy_seeds {
    ($vault_key: expr, $protocol_account: expr, $bump: expr) => {
        &[
            STRATEGY,
            $vault_key.as_ref(),
            $protocol_account.as_ref(),
            &[$bump],
        ]
    };
}

#[macro_export]
macro_rules! deposit_request_seeds {
    ($vault: expr, $authority: expr, $bump: expr) => {
        &[
            DEPOSIT_REQUEST,
            $vault.as_ref(),
            $authority.as_ref(),
            &[$bump],
        ]
    };
}

#[macro_export]
macro_rules! withdrawal_request_seeds {
    ($vault: expr, $authority: expr, $bump: expr) => {
        &[
            WITHDRAWAL_REQUEST,
            $vault.as_ref(),
            $authority.as_ref(),
            &[$bump],
        ]
    };
}
