/// taken from https://github.com/L0STE/anchor-jupiter-CPI/blob/main/programs/anchor-jupiter-cpi/src/swap.rs
use anchor_lang::{
    prelude::*,
    solana_program::{instruction::Instruction, program::invoke_signed},
};
use std::mem::size_of;

use crate::jupiter::client::args::{
    ExactOutRoute, Route, SharedAccountsExactOutRoute, SharedAccountsRoute,
};

pub const JUPITER_AGGREGATOR_EVENT_AUTHORITY: Pubkey =
    pubkey!("D8cy77BBepLMngZx6ZukaTff5hCt1HrWyKk3Hnd9oitf");

pub struct JupiterSwapCpi<'info> {
    pub token_program: AccountInfo<'info>,
    pub token_account_authority: AccountInfo<'info>,
    pub source_token_account: AccountInfo<'info>,
    pub destination_token_account: AccountInfo<'info>,
    pub source_mint: AccountInfo<'info>,
    pub destination_mint: AccountInfo<'info>,
    pub event_authority: AccountInfo<'info>,
    pub jupiter_program: AccountInfo<'info>,
}

impl<'info> JupiterSwapCpi<'info> {
    pub fn check_amount_and_slippage(
        swap_data: &[u8],
        amount: u64,
        slippage_bps: u16,
    ) -> Result<()> {
        let swap_data_length = swap_data.len();
        let bps_offset = swap_data_length - size_of::<u16>() - size_of::<u8>();
        let amount_offset = bps_offset - size_of::<u64>() - size_of::<u64>();

        require_eq!(
            amount,
            u64::from_le_bytes(
                swap_data[amount_offset..amount_offset + size_of::<u64>()]
                    .try_into()
                    .unwrap()
            )
        );
        require_gte!(
            slippage_bps,
            u16::from_le_bytes(
                swap_data[bps_offset..bps_offset + size_of::<u16>()]
                    .try_into()
                    .unwrap()
            )
        );

        Ok(())
    }

    pub fn swap(
        &mut self,
        swap_data: &[u8],
        remaining_accounts: &[AccountInfo<'info>],
        signer_seeds: &[&[u8]],
    ) -> Result<()> {
        let (account_infos, accounts) = match swap_data {
            data if data.starts_with(ExactOutRoute::DISCRIMINATOR) => {
                let mut account_infos = vec![
                    self.token_program.to_account_info(),
                    self.token_account_authority.to_account_info(),
                    self.source_token_account.to_account_info(),
                    self.destination_token_account.to_account_info(),
                    self.source_mint.to_account_info(),
                    self.destination_mint.to_account_info(),
                    self.event_authority.to_account_info(),
                    self.jupiter_program.to_account_info(),
                ];
                account_infos.extend(
                    remaining_accounts
                        .iter()
                        .map(|acc| AccountInfo { ..acc.clone() }),
                );

                let mut accounts = vec![
                    AccountMeta::new_readonly(self.token_program.key(), false), // token program
                    AccountMeta::new_readonly(self.token_account_authority.key(), true), // user transfer authority
                    AccountMeta::new(self.source_token_account.key(), false), // user source token account
                    AccountMeta::new(self.destination_token_account.key(), false), // user destination token account
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] destination token account
                    AccountMeta::new_readonly(self.source_mint.key(), false),     // source mint
                    AccountMeta::new_readonly(self.destination_mint.key(), false), // destination mint
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] platform fee account
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] token 2022 program
                    AccountMeta::new_readonly(self.event_authority.key(), false), // event authority
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // jupiter program
                ];
                accounts.extend(remaining_accounts.iter().map(|acc| AccountMeta {
                    pubkey: *acc.key,
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                }));

                (account_infos, accounts)
            }
            data if data.starts_with(Route::DISCRIMINATOR) => {
                let mut account_infos = vec![
                    self.token_program.to_account_info(),
                    self.token_account_authority.to_account_info(),
                    self.source_token_account.to_account_info(),
                    self.destination_token_account.to_account_info(),
                    self.destination_mint.to_account_info(),
                    self.event_authority.to_account_info(),
                    self.jupiter_program.to_account_info(),
                ];
                account_infos.extend(
                    remaining_accounts
                        .iter()
                        .map(|acc| AccountInfo { ..acc.clone() }),
                );

                let mut accounts = vec![
                    AccountMeta::new_readonly(self.token_program.key(), false), // token program
                    AccountMeta::new_readonly(self.token_account_authority.key(), true), // user transfer authority
                    AccountMeta::new(self.source_token_account.key(), false), // user source token account
                    AccountMeta::new(self.destination_token_account.key(), false), // user destination token account
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] destination token account
                    AccountMeta::new_readonly(self.destination_mint.key(), false), // destination mint
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] platform fee account
                    AccountMeta::new_readonly(self.event_authority.key(), false), // event authority
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // jupiter program
                ];
                accounts.extend(remaining_accounts.iter().map(|acc| AccountMeta {
                    pubkey: *acc.key,
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                }));

                (account_infos, accounts)
            }
            data if data.starts_with(SharedAccountsExactOutRoute::DISCRIMINATOR)
                || data.starts_with(SharedAccountsRoute::DISCRIMINATOR) =>
            {
                let mut account_infos = vec![
                    self.token_program.to_account_info(),
                    remaining_accounts[0].to_account_info(),
                    self.token_account_authority.to_account_info(),
                    self.source_token_account.to_account_info(),
                    remaining_accounts[1].to_account_info(),
                    remaining_accounts[2].to_account_info(),
                    self.destination_token_account.to_account_info(),
                    self.source_mint.to_account_info(),
                    self.destination_mint.to_account_info(),
                    self.event_authority.to_account_info(),
                    self.jupiter_program.to_account_info(),
                ];
                account_infos.extend(
                    remaining_accounts
                        .iter()
                        .map(|acc| AccountInfo { ..acc.clone() }),
                );

                let mut accounts = vec![
                    AccountMeta::new_readonly(self.token_program.key(), false), // token program
                    AccountMeta::new_readonly(remaining_accounts[0].key(), false), // program authority
                    AccountMeta::new_readonly(self.token_account_authority.key(), true), // user transfer authority
                    AccountMeta::new(self.source_token_account.key(), false), // source token account
                    AccountMeta::new(remaining_accounts[1].key(), false), // program source token account
                    AccountMeta::new(remaining_accounts[2].key(), false), // program destination token account
                    AccountMeta::new(self.destination_token_account.key(), false), // destination token account
                    AccountMeta::new_readonly(self.source_mint.key(), false),      // source mint
                    AccountMeta::new_readonly(self.destination_mint.key(), false), // destination mint
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] platform fee account
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] token 2022 program
                    AccountMeta::new_readonly(self.event_authority.key(), false), // event authority
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // jupiter program
                ];
                accounts.extend(remaining_accounts.iter().skip(3).map(|acc| AccountMeta {
                    pubkey: *acc.key,
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                }));

                (account_infos, accounts)
            }
            _ => {
                return Err(Error::from(ProgramError::InvalidInstructionData));
            }
        };

        invoke_signed(
            &Instruction {
                program_id: self.jupiter_program.key(),
                accounts,
                data: swap_data.to_vec(),
            },
            &account_infos,
            &[signer_seeds],
        )?;

        Ok(())
    }
}
