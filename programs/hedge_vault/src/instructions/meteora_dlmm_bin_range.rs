use anchor_lang::prelude::*;

use crate::error::HedgeVaultError;

pub fn checked_bin_range(
    position_lower: i32,
    position_upper: i32,
    requested: Option<(i32, i32)>,
) -> Result<(i32, i32)> {
    let (from, to) = requested.unwrap_or((position_lower, position_upper));
    require!(
        from >= position_lower && to <= position_upper && from <= to,
        HedgeVaultError::InvalidPositionBinRange
    );
    Ok((from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_partial_operations_to_position() {
        assert_eq!(checked_bin_range(-10, 1390, None).unwrap(), (-10, 1390));
        assert_eq!(
            checked_bin_range(-10, 1390, Some((0, 69))).unwrap(),
            (0, 69)
        );
        for range in [(-11, 0), (0, 1391), (70, 69)] {
            assert!(checked_bin_range(-10, 1390, Some(range)).is_err());
        }
    }
}
