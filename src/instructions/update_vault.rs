use anchor_lang::prelude::*;

use crate::{events::VaultUpdated, seeds::VAULT, vault_seeds, Vault, VaultStatus};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpdateVaultArgs {
    pub description: Option<[u8; 64]>,
    pub performance_fee_bps: Option<u16>,
    pub management_fee_bps: Option<u16>,
    pub deposit_cap: Option<u64>,
    pub min_deposit: Option<u64>,
    pub min_withdrawal_shares: Option<u64>,
    pub status: Option<VaultStatus>,
}

#[derive(Accounts)]
pub struct UpdateVault<'info> {
    pub authority: Signer<'info>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
}

impl<'info> UpdateVault<'info> {
    pub fn handler(ctx: Context<UpdateVault>, args: UpdateVaultArgs) -> Result<()> {
        let UpdateVault {
            authority, vault, ..
        } = ctx.accounts;

        let vault_key = vault.key();
        let vault = &mut vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;

        if let Some(description) = args.description {
            vault.description = description;
        }

        // only touch fees when asked, so a status-only update never restarts a pending delay
        if args.performance_fee_bps.is_some() || args.management_fee_bps.is_some() {
            let now = Clock::get()?.unix_timestamp;
            vault.update_fees(args.performance_fee_bps, args.management_fee_bps, now)?;
        }

        if let Some(deposit_cap) = args.deposit_cap {
            vault.deposit_cap = deposit_cap;
        }

        if let Some(min_deposit) = args.min_deposit {
            vault.min_deposit = min_deposit;
        }

        if let Some(min_withdrawal_shares) = args.min_withdrawal_shares {
            vault.min_withdrawal_shares = min_withdrawal_shares;
        }

        if let Some(status) = args.status {
            vault.status = status;
        }

        emit!(VaultUpdated {
            vault: vault_key,
            performance_fee_bps: vault.performance_fee_bps,
            management_fee_bps: vault.management_fee_bps,
            pending_performance_fee_bps: vault.pending_performance_fee_bps,
            pending_management_fee_bps: vault.pending_management_fee_bps,
            fee_effective_ts: vault.fee_effective_ts,
            deposit_cap: vault.deposit_cap,
            min_deposit: vault.min_deposit,
            min_withdrawal_shares: vault.min_withdrawal_shares,
            status: vault.status,
        });

        Ok(())
    }
}
