//! Roundtrip persistence tests for the gate-caller allowlist (issue #222).
//!
//! `set_gate_callers` writes with `GateDataKey::GateCallers` while
//! `get_gate_callers` used to read with `DataKeyC::GateCallers` — two
//! different storage keys — so the getter permanently returned an empty
//! `Vec`. These tests pin the setter/getter roundtrip.

use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

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
fn test_gate_callers_default_is_empty() {
    let (_env, client) = setup();
    assert!(client.get_gate_callers().is_empty());
}

#[test]
fn test_gate_callers_roundtrip_returns_exact_vector() {
    let (env, client) = setup();
    let caller_a = Address::generate(&env);
    let caller_b = Address::generate(&env);
    let mut callers = Vec::new(&env);
    callers.push_back(caller_a.clone());
    callers.push_back(caller_b.clone());

    client.set_gate_callers(&Vec::new(&env), &callers);

    let stored = client.get_gate_callers();
    assert_eq!(stored.len(), 2);
    // Exact vector, order preserved.
    assert_eq!(stored.get(0).unwrap(), caller_a);
    assert_eq!(stored.get(1).unwrap(), caller_b);
}

#[test]
fn test_gate_callers_roundtrip_overwrite_replaces_previous() {
    let (env, client) = setup();
    let first = Address::generate(&env);
    let second = Address::generate(&env);

    let mut v1 = Vec::new(&env);
    v1.push_back(first.clone());
    client.set_gate_callers(&Vec::new(&env), &v1);
    assert_eq!(client.get_gate_callers().len(), 1);

    let mut v2 = Vec::new(&env);
    v2.push_back(second.clone());
    client.set_gate_callers(&Vec::new(&env), &v2);

    let stored = client.get_gate_callers();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored.get(0).unwrap(), second);
}

#[test]
fn test_gate_callers_roundtrip_clear_with_empty_vec() {
    let (env, client) = setup();
    let caller = Address::generate(&env);
    let mut callers = Vec::new(&env);
    callers.push_back(caller);
    client.set_gate_callers(&Vec::new(&env), &callers);
    assert_eq!(client.get_gate_callers().len(), 1);

    client.set_gate_callers(&Vec::new(&env), &Vec::new(&env));
    assert!(client.get_gate_callers().is_empty());
}

#[test]
fn test_gate_callers_set_rejects_over_maximum() {
    let (env, client) = setup();
    let mut callers = Vec::new(&env);
    for _ in 0..=crate::constants::MAX_GATE_CALLERS {
        callers.push_back(Address::generate(&env));
    }
    assert_eq!(
        client.try_set_gate_callers(&Vec::new(&env), &callers),
        Err(Ok(Error::InvalidArgument))
    );
    // A rejected write must not disturb the previously stored list.
    assert!(client.get_gate_callers().is_empty());
}
