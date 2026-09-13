use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::{
    self,
    extension::{
        default_account_state::DefaultAccountState, transfer_fee::TransferFeeConfig,
        transfer_hook::TransferHook, BaseStateWithExtensions, ExtensionType, StateWithExtensions,
    },
    state::{AccountState, Mint},
};

use crate::{error::HedgeVaultError, validate};

/// Rejects Token-2022 deposit mints whose extensions break the vault's exclusive custody of
/// escrow and vault balances. SPL Token mints have no extensions and always pass.
pub fn validate_deposit_mint_extensions(mint: &AccountInfo, epoch: u64) -> Result<()> {
    if mint.owner != &spl_token_2022::ID {
        return Ok(());
    }

    let data = mint.try_borrow_data()?;

    check_deposit_mint_extensions(&data, epoch)
}

/// Allowlist over the mint's extension types. Types this program cannot parse are rejected.
pub fn check_deposit_mint_extensions(data: &[u8], epoch: u64) -> Result<()> {
    let mint = StateWithExtensions::<Mint>::unpack(data)
        .map_err(|_| HedgeVaultError::InvalidDepositMintExtension)?;
    let extension_types = mint
        .get_extension_types()
        .map_err(|_| HedgeVaultError::InvalidDepositMintExtension)?;

    for extension_type in extension_types {
        match extension_type {
            // a fee would make the escrow receive less than the recorded pending amount
            ExtensionType::TransferFeeConfig => {
                let fee = mint.get_extension::<TransferFeeConfig>()?.get_epoch_fee(epoch);
                validate!(
                    u16::from(fee.transfer_fee_basis_points) == 0,
                    HedgeVaultError::InvalidDepositMintExtension
                )?;
            }
            ExtensionType::TransferHook => {
                let hook = mint.get_extension::<TransferHook>()?;
                validate!(
                    Option::<Pubkey>::from(hook.program_id).is_none(),
                    HedgeVaultError::InvalidDepositMintExtension
                )?;
            }
            // frozen-by-default escrow and vault accounts could never receive deposits
            ExtensionType::DefaultAccountState => {
                let default_state = mint.get_extension::<DefaultAccountState>()?;
                validate!(
                    default_state.state != AccountState::Frozen as u8,
                    HedgeVaultError::InvalidDepositMintExtension
                )?;
            }
            // issuer trust or display only, public balances in vault accounts are unaffected
            ExtensionType::MintCloseAuthority
            | ExtensionType::ConfidentialTransferMint
            | ExtensionType::ConfidentialTransferFeeConfig
            | ExtensionType::InterestBearingConfig
            | ExtensionType::PermanentDelegate
            | ExtensionType::MetadataPointer
            | ExtensionType::TokenMetadata
            | ExtensionType::GroupPointer
            | ExtensionType::TokenGroup
            | ExtensionType::GroupMemberPointer
            | ExtensionType::TokenGroupMember => {}
            _ => return err!(HedgeVaultError::InvalidDepositMintExtension),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::solana_program::{program_option::COption, program_pack::Pack};
    use anchor_spl::token_2022::spl_token_2022::{
        extension::{
            metadata_pointer::MetadataPointer, non_transferable::NonTransferable,
            permanent_delegate::PermanentDelegate, BaseStateWithExtensionsMut,
            StateWithExtensionsMut,
        },
        state::Account,
    };

    fn base_mint() -> Mint {
        Mint {
            mint_authority: COption::None,
            supply: 0,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        }
    }

    fn mint_with(
        extensions: &[ExtensionType],
        init: impl FnOnce(&mut StateWithExtensionsMut<Mint>),
    ) -> Vec<u8> {
        let len = ExtensionType::try_calculate_account_len::<Mint>(extensions).unwrap();
        let mut data = vec![0u8; len];
        {
            let mut state = StateWithExtensionsMut::<Mint>::unpack_uninitialized(&mut data).unwrap();
            init(&mut state);
            state.base = base_mint();
            state.pack_base();
            state.init_account_type().unwrap();
        }
        data
    }

    fn assert_rejected(data: &[u8]) {
        assert_eq!(
            check_deposit_mint_extensions(data, 0).unwrap_err(),
            anchor_lang::error::Error::from(HedgeVaultError::InvalidDepositMintExtension)
        );
    }

    #[test]
    fn accepts_mint_without_extensions() {
        let mut data = vec![0u8; Mint::LEN];
        base_mint().pack_into_slice(&mut data);

        check_deposit_mint_extensions(&data, 0).unwrap();
    }

    #[test]
    fn accepts_zero_transfer_fee() {
        let data = mint_with(&[ExtensionType::TransferFeeConfig], |state| {
            state.init_extension::<TransferFeeConfig>(true).unwrap();
        });

        check_deposit_mint_extensions(&data, 0).unwrap();
    }

    #[test]
    fn rejects_non_zero_transfer_fee() {
        let data = mint_with(&[ExtensionType::TransferFeeConfig], |state| {
            let config = state.init_extension::<TransferFeeConfig>(true).unwrap();
            config.newer_transfer_fee.transfer_fee_basis_points = 50.into();
        });

        assert_rejected(&data);
    }

    #[test]
    fn accepts_transfer_hook_without_program() {
        let data = mint_with(&[ExtensionType::TransferHook], |state| {
            state.init_extension::<TransferHook>(true).unwrap();
        });

        check_deposit_mint_extensions(&data, 0).unwrap();
    }

    #[test]
    fn rejects_transfer_hook_program() {
        let data = mint_with(&[ExtensionType::TransferHook], |state| {
            let hook = state.init_extension::<TransferHook>(true).unwrap();
            bytemuck::bytes_of_mut(&mut hook.program_id).copy_from_slice(Pubkey::new_unique().as_ref());
        });

        assert_rejected(&data);
    }

    #[test]
    fn rejects_non_transferable() {
        let data = mint_with(&[ExtensionType::NonTransferable], |state| {
            state.init_extension::<NonTransferable>(true).unwrap();
        });

        assert_rejected(&data);
    }

    #[test]
    fn rejects_frozen_default_account_state() {
        let data = mint_with(&[ExtensionType::DefaultAccountState], |state| {
            state.init_extension::<DefaultAccountState>(true).unwrap().state =
                AccountState::Frozen as u8;
        });

        assert_rejected(&data);
    }

    #[test]
    fn accepts_permanent_delegate() {
        let data = mint_with(&[ExtensionType::PermanentDelegate], |state| {
            state.init_extension::<PermanentDelegate>(true).unwrap();
        });

        check_deposit_mint_extensions(&data, 0).unwrap();
    }

    #[test]
    fn rejects_unknown_extension_type() {
        let mut data = mint_with(&[ExtensionType::MetadataPointer], |state| {
            state.init_extension::<MetadataPointer>(true).unwrap();
        });
        // first TLV entry starts after the padded base (Account::LEN) and the account type byte
        data[Account::LEN + 1..Account::LEN + 3].copy_from_slice(&u16::MAX.to_le_bytes());

        assert_rejected(&data);
    }
}
