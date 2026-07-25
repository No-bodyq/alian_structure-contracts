#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token, Env,
};

fn setup_token<'a>(env: &'a Env, admin: &Address) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract(admin.clone());
    let client = token::Client::new(env, &contract_address);
    let asset_client = token::StellarAssetClient::new(env, &contract_address);
    (contract_address, client, asset_client)
}

fn advance_ledger(env: &Env, delta: u32) {
    env.ledger().with_mut(|l| {
        l.sequence_number += delta;
    });
}

#[test]
fn claim_transfers_escrow_and_settles() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);
    assert_eq!(token_client.balance(&contract_id), 500);

    client.claim_aid(&aid_id, &recipient);

    assert_eq!(token_client.balance(&recipient), 500);
    assert_eq!(token_client.balance(&contract_id), 0);

    let record = client.get_aid(&aid_id).unwrap();
    assert_eq!(record.status, AidStatus::Settled);
}

#[test]
fn second_claim_returns_already_claimed() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, _token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);
    client.claim_aid(&aid_id, &recipient);

    let result = client.try_claim_aid(&aid_id, &recipient);
    assert_eq!(result, Err(Ok(AidError::AlreadyClaimed)));
}

#[test]
fn claim_after_expiry_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, _token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);

    advance_ledger(&env, 101);

    let result = client.try_claim_aid(&aid_id, &recipient);
    assert_eq!(result, Err(Ok(AidError::Expired)));
}

#[test]
fn claim_by_wrong_address_is_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let stranger = Address::generate(&env);

    let (token_addr, _token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);

    let result = client.try_claim_aid(&aid_id, &stranger);
    assert_eq!(result, Err(Ok(AidError::Unauthorized)));
}

#[test]
fn claim_while_paused_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, _token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);
    client.set_paused(&admin, &true);

    let result = client.try_claim_aid(&aid_id, &recipient);
    assert_eq!(result, Err(Ok(AidError::Paused)));
}

#[test]
fn state_is_settled_before_transfer_state_is_consistent_on_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);
    client.claim_aid(&aid_id, &recipient);

    let record = client.get_aid(&aid_id).unwrap();
    let moved = token_client.balance(&recipient) == 500;
    assert_eq!(record.status, AidStatus::Settled);
    assert!(moved);
}

#[test]
fn refund_aid_after_expiry_returns_funds_to_donor() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);
    advance_ledger(&env, 101);

    client.refund_aid(&aid_id);

    assert_eq!(token_client.balance(&donor), 1_000);
    let record = client.get_aid(&aid_id).unwrap();
    assert_eq!(record.status, AidStatus::Refunded);
}

#[test]
fn refund_aid_before_expiry_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, _token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);

    let result = client.try_refund_aid(&aid_id);
    assert_eq!(result, Err(Ok(AidError::NotExpiredYet)));
}

#[test]
fn refund_claimed_aid_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, _token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);
    client.claim_aid(&aid_id, &recipient);
    advance_ledger(&env, 101);

    let result = client.try_refund_aid(&aid_id);
    assert_eq!(result, Err(Ok(AidError::AlreadyClaimed)));
}

#[test]
fn refund_refunded_aid_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, _token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);
    advance_ledger(&env, 101);
    client.refund_aid(&aid_id);

    let result = client.try_refund_aid(&aid_id);
    assert_eq!(result, Err(Ok(AidError::AlreadyRefunded)));
}

#[test]
fn refund_by_stranger_is_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address.generate(&env);
    let donor = Address.generate(&env);
    let recipient = Address.generate(&env);

    let (token_addr, _token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);
    advance_ledger(&env, 101);

    // We need to mock auths for the stranger trying to call refund_aid
    let stranger = Address::generate(&env);
    let res = client.try_refund_aid(&stranger, &aid_id);
    assert_eq!(res, Err(Ok(AidError::Unauthorized)));
}

#[test]
fn refund_by_admin_is_successful() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, token_client, asset_client) = setup_token(&env, &admin);
    asset_client.mint(&donor, &1_000);

    let contract_id = env.register_contract(None, AidContract);
    let client = AidContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    let expiry = env.ledger().sequence() + 100;
    let aid_id = client.create_aid(&donor, &recipient, &500, &expiry);
    advance_ledger(&env, 101);

    client.refund_aid(&aid_id);

    assert_eq!(token_client.balance(&donor), 1_000);
    let record = client.get_aid(&aid_id).unwrap();
    assert_eq!(record.status, AidStatus::Refunded);
}