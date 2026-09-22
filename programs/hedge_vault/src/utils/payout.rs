use anchor_lang::{
    prelude::*,
    solana_program::program_pack::Pack,
    system_program::{transfer, Transfer},
};
use anchor_spl::{
    associated_token::{create_idempotent, Create},
    token_2022::spl_token_2022::{
        extension::{BaseStateWithExtensions, ExtensionType, StateWithExtensions},
        state::{Account, Mint},
    },
};

use crate::error::HedgeVaultError;

/// Byte length of a token account for `mint`: the classic 165 bytes, or the Token-2022 length its
/// extensions imply plus the `ImmutableOwner` the associated token program always adds.
fn payout_account_len(mint: &AccountInfo) -> Result<usize> {
    if mint.owner != &anchor_spl::token_2022::ID {
        return Ok(Account::LEN);
    }

    let data = mint.try_borrow_data()?;
    let state = StateWithExtensions::<Mint>::unpack(&data)
        .map_err(|_| HedgeVaultError::InvalidDepositMintExtension)?;
    let extensions = state
        .get_extension_types()
        .map_err(|_| HedgeVaultError::InvalidDepositMintExtension)?;
    let mut required = ExtensionType::get_required_init_account_extensions(&extensions);
    required.push(ExtensionType::ImmutableOwner);

    ExtensionType::try_calculate_account_len::<Account>(&required)
        .map_err(|_| HedgeVaultError::InvalidDepositMintExtension.into())
}

/// Charges `payer` the rent for the payout account a request will need, held in the request until it
/// settles or closes. Sized for the larger of the two mints, since only one account is ever created.
pub fn escrow_payout_rent<'info>(
    payer: &AccountInfo<'info>,
    request: &AccountInfo<'info>,
    share_mint: &AccountInfo<'info>,
    deposit_mint: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<u64> {
    let rent = Rent::get()?;
    let escrow = rent
        .minimum_balance(payout_account_len(share_mint)?)
        .max(rent.minimum_balance(payout_account_len(deposit_mint)?));

    transfer(
        CpiContext::new(
            system_program.clone(),
            Transfer {
                from: payer.clone(),
                to: request.clone(),
            },
        ),
        escrow,
    )?;

    Ok(escrow)
}

/// Creates the payout account of `create` when its owner closed it, repaying the payer out of
/// `escrow` so settlement costs them nothing. Does nothing when the account already exists. Returns
/// the lamports drawn, which fall short only for a request predating the escrow.
pub fn create_payout_account<'info>(
    create: CpiContext<'_, '_, '_, 'info, Create<'info>>,
    escrow: &AccountInfo<'info>,
    escrow_lamports: u64,
) -> Result<u64> {
    let payout = create.accounts.associated_token.clone();

    if !payout.data_is_empty() {
        return Ok(0);
    }

    let payer = create.accounts.payer.clone();
    let funded = Rent::get()?
        .minimum_balance(payout_account_len(&create.accounts.mint)?)
        .saturating_sub(payout.lamports());

    create_idempotent(create)?;

    // repaid afterwards, not prefunded: the runtime balances lamports across the accounts a CPI is
    // given, and the request holding the escrow is not one of them
    let drawn = funded.min(escrow_lamports).min(escrow.lamports());
    **escrow.try_borrow_mut_lamports()? -= drawn;
    **payer.try_borrow_mut_lamports()? += drawn;

    Ok(drawn)
}
