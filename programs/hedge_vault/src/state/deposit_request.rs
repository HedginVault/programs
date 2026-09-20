use anchor_lang::prelude::*;

use crate::{error::HedgeVaultError, validate, validate_pda, SafeMathAssign};

pub struct NewDepositRequestArgs {
    pub authority: Pubkey,
    pub vault: Pubkey,
    pub epoch: u64,
    pub created_ts: i64,
    pub bump: u8,
}

/// Pending deposit, resolvable once the vault NAV is updated in a later epoch.
#[account]
#[derive(InitSpace)]
pub struct DepositRequest {
    /// Authority of the request, receives the minted shares.
    pub authority: Pubkey,
    /// The vault this request is for.
    pub vault: Pubkey,
    /// Deposit mint amount held in the vault's deposit escrow.
    pub amount: u64,
    /// Epoch the request was made in.
    pub epoch: u64,
    /// Timestamp the request was created.
    pub created_ts: i64,
    pub bump: u8,
    /// Lamports held above this account's own rent, earmarked for the payout token account created
    /// at settlement. Refunded with the rent when the request closes.
    pub rent_escrow: u64,
    /// Reserved for future fields.
    pub reserved: [u8; 24],
}

impl DepositRequest {
    pub fn new(args: NewDepositRequestArgs) -> Self {
        Self {
            authority: args.authority,
            vault: args.vault,
            amount: 0,
            epoch: args.epoch,
            created_ts: args.created_ts,
            bump: args.bump,
            rent_escrow: 0,
            reserved: [0; 24],
        }
    }

    pub fn validate_address(seeds: &[&[u8]], key: Pubkey) -> Result<()> {
        validate_pda(seeds, key, HedgeVaultError::InvalidRequest.into())
    }

    pub fn validate_authority(&self, authority: Pubkey) -> Result<()> {
        validate!(
            authority == self.authority,
            HedgeVaultError::InvalidRequestAuthority
        )?;

        Ok(())
    }

    pub fn validate_vault(&self, vault: Pubkey) -> Result<()> {
        validate!(vault == self.vault, HedgeVaultError::InvalidRequestVault)?;

        Ok(())
    }

    /// Requests can only accumulate within a single epoch.
    pub fn add(&mut self, amount: u64, epoch: u64) -> Result<()> {
        validate!(
            self.epoch == epoch,
            HedgeVaultError::PendingRequestNotResolved
        )?;

        self.amount.safe_add_assign(amount)
    }

    pub fn is_resolvable(&self, vault_nav_epoch: u64) -> Result<()> {
        validate!(
            vault_nav_epoch > self.epoch,
            HedgeVaultError::RequestNotResolvable
        )?;

        Ok(())
    }

    /// Cancellable until a NAV is posted for the request, or while the vault NAV is zero and
    /// the deposit cannot resolve.
    pub fn is_cancellable(&self, vault_nav_epoch: u64, vault_nav_per_share: u64) -> Result<()> {
        validate!(
            vault_nav_per_share == 0 || vault_nav_epoch <= self.epoch,
            HedgeVaultError::RequestNotCancellable
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NAV_PRECISION;

    fn request(epoch: u64) -> DepositRequest {
        DepositRequest::new(NewDepositRequestArgs {
            authority: Pubkey::default(),
            vault: Pubkey::default(),
            epoch,
            created_ts: 0,
            bump: 0,
        })
    }

    #[test]
    fn layout_size_is_stable() {
        // rent_escrow came out of reserved, so accounts written before it decode with escrow 0
        assert_eq!(DepositRequest::INIT_SPACE, 121);
        assert_eq!(request(0).rent_escrow, 0);
    }

    #[test]
    fn cancellable_until_a_nav_is_posted_for_the_request() {
        let r = request(5);

        assert!(r.is_cancellable(5, NAV_PRECISION).is_ok());
        assert!(r.is_cancellable(6, NAV_PRECISION).is_err());
    }

    #[test]
    fn cancellable_while_vault_nav_is_zero() {
        assert!(request(5).is_cancellable(6, 0).is_ok());
    }
}
