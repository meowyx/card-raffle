use anchor_lang::prelude::*;

#[event]
pub struct ConfigInitialized {
    pub admin: Pubkey,
    pub backend_signer: Pubkey,
    pub max_credit_per_call: u64,
    pub max_credit_per_epoch: u64,
    pub token_mint: Pubkey,
    pub ticket_price: u64,
}

#[event]
pub struct AdminProposed {
    pub current: Pubkey,
    pub proposed: Pubkey,
}

#[event]
pub struct AdminAccepted {
    pub previous: Pubkey,
    pub new: Pubkey,
}

#[event]
pub struct PauseChanged {
    pub paused: bool,
}

#[event]
pub struct UserInitialized {
    pub owner: Pubkey,
}

#[event]
pub struct TicketsCredited {
    pub user: Pubkey,
    pub amount: u64,
    pub payment_id: [u8; 32],
}

#[event]
pub struct TicketsPurchased {
    pub user: Pubkey,
    pub ticket_count: u64,
    pub total_price: u64,
    pub payment_id: [u8; 32],
}

#[event]
pub struct DropInitialized {
    pub drop: Pubkey,
    pub drop_id: u64,
    pub ticket_cost: u64,
    pub max_entries: u64,
    pub deadline_ts: i64,
    pub prize_amount: u64,
}

#[event]
pub struct EntryCreated {
    pub drop: Pubkey,
    pub user: Pubkey,
    pub entry_count: u64,
    pub start_index: u64,
}

#[event]
pub struct EntriesClosed {
    pub drop: Pubkey,
    pub total_entries: u64,
}

#[event]
pub struct RandomnessRequested {
    pub drop: Pubkey,
    pub randomness_account: Pubkey,
    pub commit_slot: u64,
}

#[event]
pub struct DropSettled {
    pub drop: Pubkey,
    pub winner: Pubkey,
    pub winning_index: u64,
    pub random_value: [u8; 32],
    pub prize_paid: u64,
}
