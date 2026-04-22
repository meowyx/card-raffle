#![allow(dead_code)]

use bytemuck::Zeroable;
use card_raffle::ID;
use litesvm::{types::TransactionResult, LiteSVM};
use litesvm_token::{CreateAssociatedTokenAccount, CreateMint, MintTo};
use solana_sdk::{
    account::Account,
    clock::Clock,
    instruction::Instruction,
    pubkey::Pubkey,
    rent::Rent,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::path::PathBuf;
use switchboard_on_demand::accounts::RandomnessAccountData;
use switchboard_on_demand::SWITCHBOARD_PROGRAM_ID;

pub const TEST_MINT_DECIMALS: u8 = 6;

pub fn usdc(whole: u64) -> u64 {
    whole * 1_000_000
}

pub fn setup_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    let so_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy/card_raffle.so");
    svm.add_program_from_file(ID, &so_path)
        .expect("load card_raffle.so");
    svm
}

pub fn config_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"config"], &ID).0
}

pub fn user_pda(user: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"user", user.as_ref()], &ID).0
}

pub fn drop_pda(drop_id: u64) -> Pubkey {
    Pubkey::find_program_address(&[b"drop", &drop_id.to_le_bytes()], &ID).0
}

pub fn entry_pda(drop: &Pubkey, user: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"entry", drop.as_ref(), user.as_ref()], &ID).0
}

pub fn receipt_pda(payment_id: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(&[b"receipt", payment_id.as_ref()], &ID).0
}

pub fn make_payment_id(label: &str) -> [u8; 32] {
    let mut buf = [0u8; 32];
    let bytes = label.as_bytes();
    let n = bytes.len().min(32);
    buf[..n].copy_from_slice(&bytes[..n]);
    buf
}

pub fn warp_clock(svm: &mut LiteSVM, unix_timestamp: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp = unix_timestamp;
    svm.set_sysvar(&clock);
}

pub fn warp_slot(svm: &mut LiteSVM, slot: u64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.slot = slot;
    svm.set_sysvar(&clock);
}

pub fn now_ts(svm: &LiteSVM) -> i64 {
    svm.get_sysvar::<Clock>().unix_timestamp
}

pub fn now_slot(svm: &LiteSVM) -> u64 {
    svm.get_sysvar::<Clock>().slot
}

pub fn fund(svm: &mut LiteSVM, kp: &Keypair, sol: u64) {
    svm.airdrop(&kp.pubkey(), sol * 1_000_000_000)
        .expect("airdrop");
}

pub fn create_test_mint(svm: &mut LiteSVM, payer: &Keypair, authority: &Pubkey) -> Pubkey {
    CreateMint::new(svm, payer)
        .authority(authority)
        .decimals(TEST_MINT_DECIMALS)
        .send()
        .expect("create test mint")
}

pub fn create_user_ata(svm: &mut LiteSVM, payer: &Keypair, owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    CreateAssociatedTokenAccount::new(svm, payer, mint)
        .owner(owner)
        .send()
        .expect("create user ATA")
}

pub fn mint_to_ata(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: &Pubkey,
    destination: &Pubkey,
    authority: &Keypair,
    amount: u64,
) {
    MintTo::new(svm, payer, mint, destination, amount)
        .owner(authority)
        .send()
        .expect("mint_to");
}

pub fn token_2022_program_id() -> Pubkey {
    use std::str::FromStr;
    Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
        .expect("valid Token-2022 program ID")
}

/// Synthesizes a mint-shaped account with owner = Token-2022 program ID.
/// Used to exercise the type-level rejection of Token-2022 mints.
pub fn create_fake_token_2022_mint(svm: &mut LiteSVM) -> Pubkey {
    let key = Pubkey::new_unique();
    let data = vec![0u8; 82];
    let rent: Rent = svm.get_sysvar();
    svm.set_account(
        key,
        Account {
            lamports: rent.minimum_balance(data.len()),
            data,
            owner: token_2022_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .expect("set fake token-2022 mint");
    key
}

pub fn get_token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    let account: litesvm_token::spl_token::state::Account =
        litesvm_token::get_spl_account(svm, ata).expect("get token account");
    account.amount
}

fn derive_ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    let token_program = Pubkey::new_from_array(anchor_spl::token::ID.to_bytes());
    let ata_program = Pubkey::new_from_array(anchor_spl::associated_token::ID.to_bytes());
    Pubkey::find_program_address(
        &[owner.as_ref(), token_program.as_ref(), mint.as_ref()],
        &ata_program,
    )
    .0
}

pub fn treasury_ata(mint: &Pubkey) -> Pubkey {
    derive_ata(&config_pda(), mint)
}

pub fn user_ata_for(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    derive_ata(owner, mint)
}

pub fn send_ix(
    svm: &mut LiteSVM,
    ix: Instruction,
    payer: &Keypair,
    signers: &[&Keypair],
) -> TransactionResult {
    // Rotate blockhash so repeated identical txs aren't deduplicated.
    svm.expire_blockhash();
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        signers,
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx)
}

const RANDOMNESS_DISCRIMINATOR: [u8; 8] = [10, 66, 229, 135, 220, 239, 217, 114];

pub fn build_randomness_account_data(seed_slot: u64, reveal_slot: u64, value: [u8; 32]) -> Vec<u8> {
    let mut data = RandomnessAccountData::zeroed();
    data.seed_slot = seed_slot;
    data.reveal_slot = reveal_slot;
    data.value = value;
    let mut bytes = Vec::with_capacity(8 + std::mem::size_of::<RandomnessAccountData>());
    bytes.extend_from_slice(&RANDOMNESS_DISCRIMINATOR);
    bytes.extend_from_slice(bytemuck::bytes_of(&data));
    bytes
}

pub fn write_randomness(
    svm: &mut LiteSVM,
    key: Pubkey,
    seed_slot: u64,
    reveal_slot: u64,
    value: [u8; 32],
) {
    write_randomness_with_owner(svm, key, SWITCHBOARD_PROGRAM_ID, seed_slot, reveal_slot, value);
}

pub fn write_randomness_with_owner(
    svm: &mut LiteSVM,
    key: Pubkey,
    owner: Pubkey,
    seed_slot: u64,
    reveal_slot: u64,
    value: [u8; 32],
) {
    let bytes = build_randomness_account_data(seed_slot, reveal_slot, value);
    let rent: Rent = svm.get_sysvar();
    let lamports = rent.minimum_balance(bytes.len());
    svm.set_account(
        key,
        Account {
            lamports,
            data: bytes,
            owner,
            executable: false,
            rent_epoch: 0,
        },
    )
    .expect("set randomness account");
}
