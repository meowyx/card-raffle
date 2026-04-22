use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub admin: Pubkey,
    pub pending_admin: Option<Pubkey>,
    pub backend_signer: Pubkey,
    pub paused: bool,
    pub max_credit_per_call: u64,
    pub max_credit_per_epoch: u64,
    pub current_epoch_total: u64,
    pub current_epoch_start_ts: i64,
    pub token_mint: Pubkey,
    /// Ticket price in token units (USDC has 6 decimals, so $19 = 19_000_000).
    pub ticket_price: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct UserAccount {
    pub owner: Pubkey,
    pub ticket_balance: u64,
    pub total_purchased: u64,
    pub total_spent: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct PaymentReceipt {
    pub payment_id: [u8; 32],
    pub user: Pubkey,
    pub amount: u64,
    pub credited_at: i64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Raffle {
    pub drop_id: u64,
    pub ticket_cost: u64,
    pub max_entries: u64,
    pub total_entries: u64,
    pub deadline_ts: i64,
    pub status: DropStatus,
    pub randomness_account: Option<Pubkey>,
    /// Solana slot (not unix timestamp) — Switchboard's unit for commit freshness.
    pub commit_slot: u64,
    pub random_value: [u8; 32],
    pub winner: Option<Pubkey>,
    pub winning_index: u64,
    /// Token units paid on settle. 0 = physical-prize-only drop.
    pub prize_amount: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Entry {
    pub drop: Pubkey,
    pub user: Pubkey,
    pub entry_count: u64,
    pub start_index: u64,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub enum DropStatus {
    Open,
    Closed,
    RandomnessRequested,
    Settled,
}
