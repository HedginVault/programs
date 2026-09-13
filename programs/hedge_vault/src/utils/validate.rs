use anchor_lang::prelude::*;

use crate::ID;

pub fn validate_pda(seeds: &[&[u8]], key: Pubkey, error: Error) -> Result<()> {
    let pda = Pubkey::create_program_address(seeds, &ID).unwrap();

    if pda != key {
        return Err(error);
    }

    Ok(())
}

/// https://github.com/drift-labs/drift-vaults/blob/769bf863222332747c1fc39bb7aee34e9b13cb6d/programs/drift_vaults/src/macros.rs#L2
/// Use this in place of require! to return custom errors with logging
#[macro_export]
macro_rules! validate {
        ($assert:expr, $err:expr) => {{
            if ($assert) {
                Ok(())
            } else {
                let error_code: HedgeVaultError = $err;
                msg!("Error {} thrown at {}:{}", error_code, file!(), line!());
                Err(error_code)
            }
        }};
        ($assert:expr, $err:expr, $($arg:tt)+) => {{
        if ($assert) {
            Ok(())
        } else {
            let error_code: HedgeVaultError = $err;
            msg!("Error {} thrown at {}:{}", error_code, file!(), line!());
            msg!($($arg)*);
            Err(error_code)
        }
    }};
}
