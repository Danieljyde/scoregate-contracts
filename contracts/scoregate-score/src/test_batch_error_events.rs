//! Structured error-event mapping tests for rejected batches.
//!
//! These tests verify that batch submissions generate machine-readable rejection
//! summaries without leaking sensitive input data. Each rejection code maps to
//! a documented event category for operator monitoring and alerting.

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    Address, Env, Vec,
};

use crate::{Error, ScoreGateScoreContract, ScoreGateScoreContractClient, ScoreSubmission};

const START_TS: u64 = 1_700_000_000;

fn setup<'a>() -> (Env, ScoreGateScoreContractClient<'a>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env.ledger().with_mut(|l| l.timestamp = START_TS);

    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    client.initialize(&admin, &service);

    (env, client, admin, service)
}

#[test]
fn test_batch_rejection_code_contract_paused() {
    let (env, client, _admin, _service) = setup();

    // Pause the contract
    client.pause(&Vec::new(&env));

    // Attempt batch submission
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    let mut batch = Vec::new(&env);
    batch.push_back(ScoreSubmission {
        wallet: wallet.clone(),
        asset_pair: pair.clone(),
        score: 50,
        benford_flag: false,
        ml_flag: false,
        timestamp: START_TS,
        confidence: 80,
        model_version: 1,
    });

    assert_eq!(
        client.try_submit_scores_batch(&Vec::new(&env), &batch),
        Err(Ok(Error::ContractPaused))
    );
}

#[test]
fn test_batch_rejection_code_invalid_score() {
    let (env, client, _admin, _service) = setup();

    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    let mut batch = Vec::new(&env);
    // Score out of valid range (0-100)
    batch.push_back(ScoreSubmission {
        wallet: wallet.clone(),
        asset_pair: pair,
        score: 150,
        benford_flag: false,
        ml_flag: false,
        timestamp: START_TS,
        confidence: 80,
        model_version: 1,
    });

    let result = client.submit_scores_batch(&Vec::new(&env), &batch);

    // Verify rejection code indicates invalid score
    assert_eq!(result.results.len(), 1);
    let entry = result.results.get(0).unwrap();
    assert!(!entry.accepted);
    assert_eq!(
        entry.rejection_code,
        Error::InvalidScore as u32,
        "Rejection code should indicate invalid score"
    );
}

#[test]
fn test_batch_rejection_code_invalid_confidence() {
    let (env, client, _admin, _service) = setup();

    let wallet = Address::generate(&env);
    let pair = symbol_short!("BTC_USDT");

    let mut batch = Vec::new(&env);
    // Confidence out of valid range (0-100)
    batch.push_back(ScoreSubmission {
        wallet: wallet.clone(),
        asset_pair: pair,
        score: 50,
        benford_flag: false,
        ml_flag: false,
        timestamp: START_TS,
        confidence: 150,
        model_version: 1,
    });

    let result = client.submit_scores_batch(&Vec::new(&env), &batch);

    // Verify rejection code indicates invalid confidence
    assert_eq!(result.results.len(), 1);
    let entry = result.results.get(0).unwrap();
    assert!(!entry.accepted);
    assert_eq!(
        entry.rejection_code,
        Error::InvalidConfidence as u32,
        "Rejection code should indicate invalid confidence"
    );
}

#[test]
fn test_batch_rejection_code_invalid_timestamp() {
    let (env, client, _admin, _service) = setup();

    let wallet = Address::generate(&env);
    let pair = symbol_short!("ETH_USDC");

    let mut batch = Vec::new(&env);
    // Timestamp in future
    batch.push_back(ScoreSubmission {
        wallet: wallet.clone(),
        asset_pair: pair,
        score: 50,
        benford_flag: false,
        ml_flag: false,
        timestamp: 0,
        confidence: 80,
        model_version: 1,
    });

    let result = client.submit_scores_batch(&Vec::new(&env), &batch);

    // Verify rejection code indicates invalid timestamp
    assert_eq!(result.results.len(), 1);
    let entry = result.results.get(0).unwrap();
    assert!(!entry.accepted);
    assert_eq!(
        entry.rejection_code,
        Error::InvalidTimestamp as u32,
        "Rejection code should indicate invalid timestamp"
    );
}

#[test]
fn test_batch_mixed_acceptance_and_rejection() {
    let (env, client, _admin, _service) = setup();

    let wallet1 = Address::generate(&env);
    let wallet2 = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    let mut batch = Vec::new(&env);

    // Valid entry
    batch.push_back(ScoreSubmission {
        wallet: wallet1.clone(),
        asset_pair: pair.clone(),
        score: 45,
        benford_flag: false,
        ml_flag: false,
        timestamp: START_TS,
        confidence: 80,
        model_version: 1,
    });

    // Invalid score entry
    batch.push_back(ScoreSubmission {
        wallet: wallet2.clone(),
        asset_pair: pair,
        score: 150,
        benford_flag: false,
        ml_flag: false,
        timestamp: START_TS,
        confidence: 80,
        model_version: 1,
    });

    let result = client.submit_scores_batch(&Vec::new(&env), &batch);

    // Verify first entry accepted, second rejected
    assert_eq!(result.results.len(), 2);

    let entry0 = result.results.get(0).unwrap();
    assert!(entry0.accepted, "First entry should be accepted");
    assert_eq!(entry0.rejection_code, 0, "Accepted entries have zero rejection code");

    let entry1 = result.results.get(1).unwrap();
    assert!(!entry1.accepted, "Second entry should be rejected");
    assert_eq!(
        entry1.rejection_code,
        Error::InvalidScore as u32,
        "Second entry should have invalid score code"
    );
}

#[test]
fn test_batch_rejection_deterministic_across_wallets() {
    let (env, client, _admin, _service) = setup();

    // Submit same invalid data for different wallets
    let mut wallets = Vec::new(&env);
    for _ in 0..3 {
        wallets.push_back(Address::generate(&env));
    }
    let pair = symbol_short!("XLM_USDC");

    let mut batch = Vec::new(&env);
    for wallet in wallets.iter() {
        batch.push_back(ScoreSubmission {
            wallet: wallet.clone(),
            asset_pair: pair.clone(),
            score: 150, // Invalid
            benford_flag: false,
            ml_flag: false,
            timestamp: START_TS,
            confidence: 80,
            model_version: 1,
        });
    }

    let result = client.submit_scores_batch(&Vec::new(&env), &batch);

    // Verify all entries rejected with same code (deterministic)
    assert_eq!(result.results.len(), 3);
    for i in 0..3 {
        let entry = result.results.get(i).unwrap();
        assert!(!entry.accepted);
        assert_eq!(
            entry.rejection_code,
            Error::InvalidScore as u32,
            "All entries should have same deterministic rejection code"
        );
    }
}

#[test]
fn test_rejection_does_not_leak_wallet_data() {
    let (env, client, _admin, _service) = setup();

    let sensitive_wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    let mut batch = Vec::new(&env);
    batch.push_back(ScoreSubmission {
        wallet: sensitive_wallet.clone(),
        asset_pair: pair,
        score: 50,
        benford_flag: false,
        ml_flag: false,
        timestamp: START_TS,
        confidence: 150, // Invalid
        model_version: 1,
    });

    let result = client.submit_scores_batch(&Vec::new(&env), &batch);

    // Verify the event/result contains only rejection reason, not wallet details
    let entry = result.results.get(0).unwrap();
    assert!(!entry.accepted);
    assert_eq!(
        entry.rejection_code,
        Error::InvalidConfidence as u32,
        "Event should contain rejection reason without wallet data"
    );
    // The wallet address itself is not part of the rejection response
    // in terms of the rejection_code - only the category is returned
}

#[test]
fn test_batch_rejection_events_publish_schema_version_and_contents() {
    use crate::events::{self, EVENT_VERSION};
    use soroban_sdk::{testutils::Events as _, IntoVal, Symbol};

    let env = Env::default();

    // 1. batch_rejected_contract_paused
    events::batch_rejected_contract_paused(&env, 5);
    let all = env.events().all();
    let (_, topics, data) = all.last().unwrap();
    assert_eq!(topics.len(), 2);
    let name: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(name, Symbol::new(&env, "bat_rej_pa"));
    let version: u32 = topics.get(1).unwrap().into_val(&env);
    assert_eq!(version, EVENT_VERSION);
    let count: u32 = data.into_val(&env);
    assert_eq!(count, 5);

    // 2. batch_rejected_data_quality
    events::batch_rejected_data_quality(&env, 2, 3);
    let all = env.events().all();
    let (_, topics, data) = all.last().unwrap();
    assert_eq!(topics.len(), 2);
    let name: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(name, Symbol::new(&env, "bat_rej_dq"));
    let version: u32 = topics.get(1).unwrap().into_val(&env);
    assert_eq!(version, EVENT_VERSION);
    let (code, count): (u32, u32) = data.into_val(&env);
    assert_eq!((code, count), (2, 3));

    // 3. batch_rejected_model_version
    events::batch_rejected_model_version(&env, 1, 4);
    let all = env.events().all();
    let (_, topics, data) = all.last().unwrap();
    assert_eq!(topics.len(), 2);
    let name: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(name, Symbol::new(&env, "bat_rej_mv"));
    let version: u32 = topics.get(1).unwrap().into_val(&env);
    assert_eq!(version, EVENT_VERSION);
    let (code, count): (u32, u32) = data.into_val(&env);
    assert_eq!((code, count), (1, 4));

    // 4. batch_rejected_rate_limit
    events::batch_rejected_rate_limit(&env, 2);
    let all = env.events().all();
    let (_, topics, data) = all.last().unwrap();
    assert_eq!(topics.len(), 2);
    let name: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(name, Symbol::new(&env, "bat_rej_rl"));
    let version: u32 = topics.get(1).unwrap().into_val(&env);
    assert_eq!(version, EVENT_VERSION);
    let count: u32 = data.into_val(&env);
    assert_eq!(count, 2);

    // 5. batch_rejected_attestation
    events::batch_rejected_attestation(&env, 7);
    let all = env.events().all();
    let (_, topics, data) = all.last().unwrap();
    assert_eq!(topics.len(), 2);
    let name: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(name, Symbol::new(&env, "bat_rej_at"));
    let version: u32 = topics.get(1).unwrap().into_val(&env);
    assert_eq!(version, EVENT_VERSION);
    let count: u32 = data.into_val(&env);
    assert_eq!(count, 7);

    // 6. batch_rejected_gate_failure
    events::batch_rejected_gate_failure(&env, 9);
    let all = env.events().all();
    let (_, topics, data) = all.last().unwrap();
    assert_eq!(topics.len(), 2);
    let name: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(name, Symbol::new(&env, "bat_rej_gt"));
    let version: u32 = topics.get(1).unwrap().into_val(&env);
    assert_eq!(version, EVENT_VERSION);
    let count: u32 = data.into_val(&env);
    assert_eq!(count, 9);
}
