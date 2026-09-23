use hedge_vault_litesvm_phoenix::*;

#[test]
fn initialize_registers_the_vault_as_a_phoenix_trader() {
    let mut ctx = PhoenixVaultContext::new();

    ctx.initialize_strategy().unwrap();

    let strategy = ctx.strategy().unwrap();
    assert_eq!(
        strategy.strategy_type,
        StrategyType::PhoenixPerp {
            trader_account: to_anchor(ctx.trader_account)
        }
    );
    assert_eq!(ctx.open_strategy_count(), 1);
    assert!(ctx.account_exists(&ctx.vault_canonical_token_account()));

    let trader = ctx.trader();
    assert_eq!(trader.quote_lot_collateral, 0);
    assert_eq!(trader.position_count, 0);
}

#[test]
fn initialize_adopts_a_trader_registered_through_the_phoenix_api() {
    let mut ctx = PhoenixVaultContext::new();
    ctx.register_trader_externally();

    ctx.initialize_strategy().unwrap();

    assert!(ctx.strategy().is_some());
    ctx.deposit(100 * USDC).unwrap();
}

#[test]
fn initialize_rejects_a_trader_account_other_than_the_vaults() {
    let mut ctx = PhoenixVaultContext::new();
    let other_trader = ctx.phoenix.actor_trader("taker0");

    let initialize = ctx.initialize_strategy_ix(other_trader);

    assert_error(
        ctx.send(&[initialize]),
        HedgeVaultError::InvalidPhoenixTrader,
    );
}

#[test]
fn close_requires_an_empty_trader_and_closes_the_canonical_account() {
    let mut ctx = PhoenixVaultContext::new();
    ctx.initialize_strategy().unwrap();
    ctx.deposit(100 * USDC).unwrap();

    assert_error(
        ctx.close_strategy(),
        HedgeVaultError::PhoenixStrategyNotEmpty,
    );

    ctx.warp_slots(1_000);
    ctx.withdraw(100 * USDC).unwrap();
    ctx.close_strategy().unwrap();

    assert!(ctx.strategy().is_none());
    assert!(!ctx.account_exists(&ctx.vault_canonical_token_account()));
    assert_eq!(ctx.open_strategy_count(), 0);
    assert_eq!(ctx.vault_usdc(), VAULT_USDC);
}
