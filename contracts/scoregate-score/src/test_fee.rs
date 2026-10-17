#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
    Address, Env, Vec,
};

use crate::{
    Error, ScoreGateScoreContract, ScoreGateScoreContractClient,
};

fn setup<'a>() -> (Env, ScoreGateScoreContractClient<'a>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    client.initialize(&admin, &service);

    (env, client, admin, service)
}

#[test]
fn test_set_fee_token_insufficient_signers_rejected() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);

    client.add_admin_signer(&Vec::new(&env), &s1);
    client.add_admin_signer(&Vec::new(&env), &s2);
    client.add_admin_signer(&Vec::new(&env), &s3);
    client.set_admin_threshold(&Vec::new(&env), &2);

    let fee_token = Address::generate(&env);

    // 0 signers when threshold is 2 -> InsufficientAdminSigners
    let res = client.try_set_fee_token(&Vec::new(&env), &fee_token);
    assert_eq!(res, Err(Ok(Error::InsufficientAdminSigners)));

    // 1 signer when threshold is 2 -> InsufficientAdminSigners
    let mut signers_1 = Vec::new(&env);
    signers_1.push_back(s1.clone());
    let res = client.try_set_fee_token(&signers_1, &fee_token);
    assert_eq!(res, Err(Ok(Error::InsufficientAdminSigners)));

    // Fee token was not set
    assert_eq!(client.get_fee_token(), None);
}

#[test]
fn test_set_fee_token_multisig_quorum_succeeds() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);

    client.add_admin_signer(&Vec::new(&env), &s1);
    client.add_admin_signer(&Vec::new(&env), &s2);
    client.add_admin_signer(&Vec::new(&env), &s3);
    client.set_admin_threshold(&Vec::new(&env), &2);

    let fee_token = Address::generate(&env);

    let mut signers_2 = Vec::new(&env);
    signers_2.push_back(s1);
    signers_2.push_back(s2);

    let res = client.try_set_fee_token(&signers_2, &fee_token);
    assert!(res.is_ok());
    assert_eq!(client.get_fee_token(), Some(fee_token));
}

#[test]
fn test_set_fee_token_legacy_single_admin_mode() {
    let (env, client, _admin, _service) = setup();
    let fee_token = Address::generate(&env);

    // In legacy single-admin mode (threshold = 0, no signers configured),
    // require_admin_auth requires legacy admin authorization.
    let res = client.try_set_fee_token(&Vec::new(&env), &fee_token);
    assert!(res.is_ok());
    assert_eq!(client.get_fee_token(), Some(fee_token));
}

#[test]
fn test_set_fee_token_signer_not_in_set_rejected() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let rogue = Address::generate(&env);

    client.add_admin_signer(&Vec::new(&env), &s1);
    client.set_admin_threshold(&Vec::new(&env), &1);

    let fee_token = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(rogue);

    let res = client.try_set_fee_token(&signers, &fee_token);
    assert_eq!(res, Err(Ok(Error::AdminSignerNotInSet)));
    assert_eq!(client.get_fee_token(), None);
}

#[test]
fn test_set_fee_token_uninitialized_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);
    let fee_token = Address::generate(&env);

    let res = client.try_set_fee_token(&Vec::new(&env), &fee_token);
    assert_eq!(res, Err(Ok(Error::NotInitialized)));
}
