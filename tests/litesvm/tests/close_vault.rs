use hedge_vault_litesvm::*;

#[test]
fn close_vault_requires_every_strategy_to_be_closed() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let target_mint = ctx.create_mint(&TOKEN_PROGRAM, None);

    ctx.jupiter_initialize_strategy(&v, &target_mint).unwrap();
    assert_eq!(ctx.vault(&v.address).open_strategy_count, 1);

    assert_error(ctx.close_vault(&v), HedgeVaultError::VaultHasOpenStrategies);

    ctx.close_jupiter_strategy(&v, &v.address, &target_mint)
        .unwrap();
    assert_eq!(ctx.vault(&v.address).open_strategy_count, 0);

    ctx.close_vault(&v).unwrap();
    assert!(ctx.svm.get_account(&v.address).is_none_or(|a| a.data.is_empty()));
}

#[test]
fn close_vault_requires_zero_total_assets() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();

    // assets held outside the vault ATA are only visible through the posted NAV
    ctx.warp_days(1);
    ctx.update_nav(&v, 50 * USDC).unwrap();
    assert_eq!(ctx.vault(&v.address).total_assets, 50 * USDC);

    assert_error(ctx.close_vault(&v), HedgeVaultError::VaultHasAssets);

    // a final NAV of zero after the unwind is what makes the vault closable
    ctx.warp_days(1);
    ctx.update_nav(&v, 0).unwrap();
    ctx.close_vault(&v).unwrap();
}
