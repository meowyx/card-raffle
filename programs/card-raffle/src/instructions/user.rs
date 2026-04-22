use anchor_lang::prelude::*;

use crate::events::UserInitialized;
use crate::state::UserAccount;

pub fn handle_initialize_user(ctx: Context<InitializeUser>) -> Result<()> {
    let user = &mut ctx.accounts.user_account;
    user.owner = ctx.accounts.user_signer.key();
    user.ticket_balance = 0;
    user.total_purchased = 0;
    user.total_spent = 0;
    user.bump = ctx.bumps.user_account;
    emit!(UserInitialized { owner: user.owner });
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeUser<'info> {
    #[account(mut)]
    pub user_signer: Signer<'info>,
    #[account(
        init,
        payer = user_signer,
        space = 8 + UserAccount::INIT_SPACE,
        seeds = [b"user", user_signer.key().as_ref()],
        bump,
    )]
    pub user_account: Account<'info, UserAccount>,
    pub system_program: Program<'info, System>,
}
