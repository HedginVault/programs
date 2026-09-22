use hedge_vault_litesvm::*;

#[test]
fn admin_transfer_requires_acceptance() {
    let mut ctx = TestContext::new();
    let old_admin = ctx.admin.pubkey();
    let new_admin = ctx.new_user();

    let mut args = TestContext::update_config_args();
    args.pending_admin = Some(new_admin.pubkey());
    ctx.config_update(args).unwrap();

    assert_eq!(ctx.config().admin, old_admin);
    assert_eq!(ctx.config().pending_admin, new_admin.pubkey());

    let stranger = ctx.new_user();
    assert_error(ctx.admin_accept(&stranger), HedgeVaultError::InvalidPendingAdmin);

    ctx.admin_accept(&new_admin).unwrap();
    assert_eq!(ctx.config().admin, new_admin.pubkey());
    assert_eq!(ctx.config().pending_admin, Pubkey::default());

    // the harness signs config_update with the old admin, which is no longer allowed
    assert_error(
        ctx.config_update(TestContext::update_config_args()),
        HedgeVaultError::InvalidAdmin,
    );
}
