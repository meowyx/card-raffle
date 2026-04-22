use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

pub use errors::*;
pub use events::*;
pub use instructions::EPOCH_LENGTH_SECS;
pub use state::*;

declare_id!("BzoGMtd43CgMjmwVkV1u4hAwA1vbj9Td5gSeJXTDaQTw");

#[program]
pub mod card_raffle {
    use super::*;

    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        backend_signer: Pubkey,
        max_credit_per_call: u64,
        max_credit_per_epoch: u64,
        ticket_price: u64,
    ) -> Result<()> {
        instructions::config::handle_initialize_config(
            ctx,
            backend_signer,
            max_credit_per_call,
            max_credit_per_epoch,
            ticket_price,
        )
    }

    pub fn propose_admin(ctx: Context<ProposeAdmin>, new_admin: Pubkey) -> Result<()> {
        instructions::config::handle_propose_admin(ctx, new_admin)
    }

    pub fn accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
        instructions::config::handle_accept_admin(ctx)
    }

    pub fn set_pause(ctx: Context<SetPause>, paused: bool) -> Result<()> {
        instructions::config::handle_set_pause(ctx, paused)
    }

    pub fn initialize_user(ctx: Context<InitializeUser>) -> Result<()> {
        instructions::user::handle_initialize_user(ctx)
    }

    pub fn credit_tickets(
        ctx: Context<CreditTickets>,
        payment_id: [u8; 32],
        amount: u64,
    ) -> Result<()> {
        instructions::tickets::handle_credit_tickets(ctx, payment_id, amount)
    }

    pub fn buy_tickets(
        ctx: Context<BuyTickets>,
        payment_id: [u8; 32],
        ticket_count: u64,
    ) -> Result<()> {
        instructions::tickets::handle_buy_tickets(ctx, payment_id, ticket_count)
    }

    pub fn initialize_drop(
        ctx: Context<InitializeDrop>,
        drop_id: u64,
        ticket_cost: u64,
        max_entries: u64,
        deadline_ts: i64,
        prize_amount: u64,
    ) -> Result<()> {
        instructions::drops::handle_initialize_drop(
            ctx,
            drop_id,
            ticket_cost,
            max_entries,
            deadline_ts,
            prize_amount,
        )
    }

    pub fn enter_drop(ctx: Context<EnterDrop>, entry_count: u64) -> Result<()> {
        instructions::drops::handle_enter_drop(ctx, entry_count)
    }

    pub fn close_entries(ctx: Context<CloseEntries>) -> Result<()> {
        instructions::drops::handle_close_entries(ctx)
    }

    pub fn request_randomness(ctx: Context<RequestRandomness>) -> Result<()> {
        instructions::randomness::handle_request_randomness(ctx)
    }

    pub fn settle_drop(ctx: Context<SettleDrop>) -> Result<()> {
        instructions::randomness::handle_settle_drop(ctx)
    }
}
