//! Tests for `set_consensus_config`.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{Error, ScoreGateScoreContract, ScoreGateScoreContractClient};

fn setup<'a>() -> (Env, ScoreGateScoreContractClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    client.initialize(&admin, &service);
    (env, client)
}

#[test]
fn test_set_consensus_config_happy_path() {
    let (_env, client) = setup();
    client.set_consensus_config(&3, &10);
    assert_eq!(client.get_consensus_config(), (3, 10));
}

#[test]
fn test_set_consensus_config_updates_both_atomically() {
    let (_env, client) = setup();
    client.set_consensus_config(&2, &5);
    assert_eq!(client.get_consensus_config(), (2, 5));
    // Override with new values — both change together.
    client.set_consensus_config(&5, &20);
    assert_eq!(client.get_consensus_config(), (5, 20));
}

#[test]
fn test_set_consensus_config_boundary_values() {
    let (_env, client) = setup();
    // k=1 and epsilon=0 are the lowest valid values.
    client.set_consensus_config(&1, &0);
    assert_eq!(client.get_consensus_config(), (1, 0));
    // epsilon=100 is the highest valid value.
    client.set_consensus_config(&1, &100);
    assert_eq!(client.get_consensus_config(), (1, 100));
}

#[test]
fn test_set_consensus_config_k_zero_rejected() {
    let (_env, client) = setup();
    let result = client.try_set_consensus_config(&0, &5);
    assert_eq!(result, Err(Ok(Error::InvalidConsensusConfig)));
}

#[test]
fn test_set_consensus_config_epsilon_over_100_rejected() {
    let (_env, client) = setup();
    let result = client.try_set_consensus_config(&2, &101);
    assert_eq!(result, Err(Ok(Error::InvalidConsensusConfig)));
}

#[test]
fn test_set_consensus_config_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);

    let result = client.try_set_consensus_config(&2, &5);
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
}

#[test]
fn test_set_consensus_config_non_admin_rejected() {
    let env = Env::default();
    // Do NOT mock_all_auths so that require_auth() enforces real authorization.
    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);

    // Initialize using a scoped mock just for the initialize call.
    env.mock_all_auths_allowing_non_root_auth();
    client.initialize(&admin, &service);
    env.set_auths(&[]);

    // Without any auth context, set_consensus_config must be rejected.
    let result = client.try_set_consensus_config(&2, &5);
    assert!(result.is_err());
}

#[test]
fn test_set_adaptive_epsilon_bounds_spread_enforced() {
    let (env, client) = setup();
    let admin_signers = soroban_sdk::Vec::new(&env);

    // Spread == 5 (min 5, max 10) is accepted (MIN_EPSILON_SPREAD = 5).
    let result = client.try_set_adaptive_epsilon_bounds(&admin_signers, &true, &5, &10);
    assert!(result.is_ok());
    assert_eq!(client.get_adaptive_epsilon_bounds(), (true, 5, 10));

    // Spread > 5 (min 5, max 20) is accepted.
    let result = client.try_set_adaptive_epsilon_bounds(&admin_signers, &true, &5, &20);
    assert!(result.is_ok());
    assert_eq!(client.get_adaptive_epsilon_bounds(), (true, 5, 20));

    // Equal bounds (min == max) must fail.
    let result = client.try_set_adaptive_epsilon_bounds(&admin_signers, &true, &10, &10);
    assert_eq!(result, Err(Ok(Error::InvalidArgument)));

    // Spread < 5 (min 10, max 14) must fail.
    let result = client.try_set_adaptive_epsilon_bounds(&admin_signers, &true, &10, &14);
    assert_eq!(result, Err(Ok(Error::InvalidArgument)));

    // Inverted bounds (min > max) must fail.
    let result = client.try_set_adaptive_epsilon_bounds(&admin_signers, &true, &15, &10);
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
}

