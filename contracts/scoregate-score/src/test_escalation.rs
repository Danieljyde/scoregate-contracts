#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    Address, Env, Vec,
};

use crate::{Error, ScoreGateScoreContract, ScoreGateScoreContractClient};

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
fn escalation_threshold_defaults_to_five() {
    let (_env, client, _admin, _service) = setup();

    assert_eq!(client.get_escalation_threshold(), 5);
}

#[test]
fn breach_count_defaults_to_zero() {
    let (env, client, _admin, _service) = setup();

    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    assert_eq!(client.get_breach_count(&wallet, &pair), 0);
}

#[test]
fn breach_count_increments_after_consecutive_breaches() {
    let (env, client, _admin, _service) = setup();

    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    client.set_risk_threshold(&Vec::new(&env), &80);

    client.submit_score(
        &Vec::new(&env),
        &wallet,
        &pair,
        &90,
        &true,
        &true,
        &1,
        &95,
        &1,
        &None,
    );

    assert_eq!(client.get_breach_count(&wallet, &pair), 1);

    env.ledger().with_mut(|l| l.timestamp += 3601);

    client.submit_score(
        &Vec::new(&env),
        &wallet,
        &pair,
        &95,
        &true,
        &true,
        &2,
        &95,
        &1,
        &None,
    );

    assert_eq!(client.get_breach_count(&wallet, &pair), 2);
}

#[test]
fn breach_count_resets_after_non_breach_submission() {
    let (env, client, _admin, _service) = setup();

    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    client.set_risk_threshold(&Vec::new(&env), &80);

    client.submit_score(
        &Vec::new(&env),
        &wallet,
        &pair,
        &90,
        &true,
        &true,
        &1,
        &95,
        &1,
        &None,
    );

    assert_eq!(client.get_breach_count(&wallet, &pair), 1);

    env.ledger().with_mut(|l| l.timestamp += 3601);

    client.submit_score(
        &Vec::new(&env),
        &wallet,
        &pair,
        &40,
        &false,
        &false,
        &2,
        &95,
        &1,
        &None,
    );

    assert_eq!(client.get_breach_count(&wallet, &pair), 0);
}

// ── Multisig authorization for set_escalation_threshold ───────────────────────

#[test]
fn test_set_escalation_threshold_multisig_enforced() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    client.add_admin_signer(&Vec::new(&env), &s1);
    client.add_admin_signer(&Vec::new(&env), &s2);
    client.set_admin_threshold(&Vec::new(&env), &2);

    // Single-key authorization fails when threshold = 2
    let single_signer = Vec::from_array(&env, [s1.clone()]);
    let err = client.try_set_escalation_threshold(&single_signer, &7);
    assert_eq!(err, Err(Ok(Error::InsufficientAdminSigners)));

    // Empty signers fails when threshold = 2
    let empty_signers = Vec::new(&env);
    let err_empty = client.try_set_escalation_threshold(&empty_signers, &7);
    assert_eq!(err_empty, Err(Ok(Error::InsufficientAdminSigners)));

    // Quorum of 2 signers succeeds
    let signers = Vec::from_array(&env, [s1.clone(), s2.clone()]);
    client.set_escalation_threshold(&signers, &7);
    assert_eq!(client.get_escalation_threshold(), 7);
}

#[test]
fn test_set_escalation_threshold_multisig_unauthorized_signer_fails() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let rogue = Address::generate(&env);
    client.add_admin_signer(&Vec::new(&env), &s1);
    client.add_admin_signer(&Vec::new(&env), &s2);
    client.set_admin_threshold(&Vec::new(&env), &2);

    let signers = Vec::from_array(&env, [s1.clone(), rogue]);
    let err = client.try_set_escalation_threshold(&signers, &7);
    assert_eq!(err, Err(Ok(Error::AdminSignerNotInSet)));
}
