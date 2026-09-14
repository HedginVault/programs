use hedge_vault_litesvm::*;

/// Vault plus a fresh SPL mint to use as a Jupiter strategy target.
fn vault_and_target(ctx: &mut TestContext) -> (TestVault, Pubkey) {
    let v = ctx.setup_vault();
    let target_mint = ctx.create_mint(&TOKEN_PROGRAM, None);

    (v, target_mint)
}

#[test]
fn jupiter_strategy_rejects_the_deposit_and_share_mints() {
    let mut ctx = TestContext::new();
    let (v, target_mint) = vault_and_target(&mut ctx);

    assert_error(
        ctx.jupiter_initialize_strategy(&v, &v.deposit_mint),
        HedgeVaultError::InvalidStrategyMint,
    );
    assert_error(
        ctx.jupiter_initialize_strategy(&v, &v.share_mint),
        HedgeVaultError::InvalidStrategyMint,
    );

    ctx.jupiter_initialize_strategy(&v, &target_mint).unwrap();
    assert_eq!(ctx.vault(&v.address).open_strategy_count, 1);
}

#[test]
fn jupiter_swap_rejects_the_deposit_escrow_as_source() {
    let mut ctx = TestContext::new();
    let (v, target_mint) = vault_and_target(&mut ctx);
    ctx.jupiter_initialize_strategy(&v, &target_mint).unwrap();

    // the escrow is a vault-owned deposit mint account, but it is not the vault's ATA
    let result = ctx.jupiter_swap(
        &v,
        &deposit_escrow_pda(&v.address),
        &v.deposit_mint,
        &target_mint,
        USDC,
    );

    assert_error_code(result, ANCHOR_CONSTRAINT_ASSOCIATED);
}

#[test]
fn close_strategy_rejects_a_strategy_from_another_vault() {
    let mut ctx = TestContext::new();
    let (owner_vault, target_mint) = vault_and_target(&mut ctx);
    ctx.jupiter_initialize_strategy(&owner_vault, &target_mint)
        .unwrap();

    let other_vault = ctx.setup_vault();

    assert_error(
        ctx.close_jupiter_strategy(&other_vault, &owner_vault.address, &target_mint),
        HedgeVaultError::InvalidStrategy,
    );

    // the strategy is untouched and still closable by its own vault
    assert!(ctx
        .strategy(&strategy_pda(&owner_vault.address, &target_mint))
        .is_some());
    ctx.close_jupiter_strategy(&owner_vault, &owner_vault.address, &target_mint)
        .unwrap();
}

#[test]
fn paused_vault_blocks_a_new_strategy() {
    let mut ctx = TestContext::new();
    let (v, target_mint) = vault_and_target(&mut ctx);

    ctx.pause_vault(&v).unwrap();

    assert_error(
        ctx.jupiter_initialize_strategy(&v, &target_mint),
        HedgeVaultError::VaultNotOperational,
    );
}

#[test]
fn paused_protocol_blocks_a_new_strategy() {
    let mut ctx = TestContext::new();
    let (v, target_mint) = vault_and_target(&mut ctx);

    let mut args = TestContext::update_config_args();
    args.status = Some(ProtocolStatus::Paused);
    ctx.update_config(args).unwrap();

    assert_error(
        ctx.jupiter_initialize_strategy(&v, &target_mint),
        HedgeVaultError::ProtocolNotOperational,
    );
}
