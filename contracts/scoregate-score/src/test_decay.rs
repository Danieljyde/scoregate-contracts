use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    Address, Env, Vec,
};

use crate::{Error, ScoreGateScoreContract, ScoreGateScoreContractClient};

const START_TS: u64 = 1_700_000_000;

fn setup<'a>() -> (Env, ScoreGateScoreContractClient<'a>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);
    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    client.initialize(&admin, &service);
    (env, client, admin, service)
}

fn submit_pair(
    env: &Env,
    client: &ScoreGateScoreContractClient,
    wallet: &Address,
    pair: &soroban_sdk::Symbol,
    score: u32,
    ts: u64,
) {
    client.submit_score(&Vec::new(env), wallet, pair, &score, &false, &false, &ts, &90, &1, &None);
}

#[test]
fn test_get_decay_rate_defaults_to_zero() {
    let (_env, client, _, _) = setup();
    let (num, den) = client.get_decay_rate();
    assert_eq!(num, 0);
    assert_eq!(den, 1);
}

#[test]
fn test_set_decay_rate_valid_accepted() {
    let (_env, client, _, _) = setup();
    client.set_decay_rate(&1, &1000);
    let (num, den) = client.get_decay_rate();
    assert_eq!(num, 1);
    assert_eq!(den, 1000);
}

#[test]
fn test_set_decay_rate_above_max_rejected() {
    let (_env, client, _, _) = setup();
    // λ = 2/1 = 2.0 exceeds MAX_DECAY_LAMBDA (numerator > denominator)
    assert_eq!(client.try_set_decay_rate(&2, &1), Err(Ok(Error::InvalidDecayRate)));
}

#[test]
fn test_set_decay_rate_zero_denominator_rejected() {
    let (_env, client, _, _) = setup();
    assert_eq!(client.try_set_decay_rate(&1, &0), Err(Ok(Error::InvalidDecayRate)));
}

#[test]
fn test_set_decay_rate_blocked_when_paused() {
    let (env, client, _, _) = setup();
    client.pause(&Vec::new(&env));
    assert_eq!(client.try_set_decay_rate(&1, &1000), Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_set_decay_rate_max_boundary_accepted() {
    let (_env, client, _, _) = setup();
    // MAX_DECAY_LAMBDA = 0.01 = 1/100
    client.set_decay_rate(&1, &100);
    let (num, den) = client.get_decay_rate();
    assert_eq!(num, 1);
    assert_eq!(den, 100);
}

#[test]
fn test_set_decay_rate_below_max_boundary_accepted() {
    let (_env, client, _, _) = setup();
    // λ = 0.001 < 0.01
    client.set_decay_rate(&1, &1000);
    let (num, den) = client.get_decay_rate();
    assert_eq!(num, 1);
    assert_eq!(den, 1000);
}

#[test]
fn test_decay_rate_zero_reproduces_static_average() {
    let (env, client, _, _) = setup();
    let wallet = Address::generate(&env);
    let pair1 = symbol_short!("XLMU");
    let pair2 = symbol_short!("XLMB");

    let (num, _) = client.get_decay_rate();
    assert_eq!(num, 0);

    client.set_pair_weight(&Vec::new(&env), &pair1, &1);
    client.set_pair_weight(&Vec::new(&env), &pair2, &2);

    submit_pair(&env, &client, &wallet, &pair1, 30, 100);
    submit_pair(&env, &client, &wallet, &pair2, 60, 200);

    let aggregate = client.get_aggregate_score(&wallet);
    // Expected: (30*1 + 60*2) / (1 + 2) = 150 / 3 = 50
    assert_eq!(aggregate.aggregate_score, 50);
    assert!(!aggregate.decay_lambda_applied);
}

#[test]
fn test_decay_rate_nonzero_reduces_old_scores() {
    let (env, client, _, _) = setup();
    let wallet = Address::generate(&env);
    let pair_a = symbol_short!("XLMA");
    let pair_b = symbol_short!("XLMB");

    client.set_pair_weight(&Vec::new(&env), &pair_a, &1);
    client.set_pair_weight(&Vec::new(&env), &pair_b, &1);
    client.set_decay_rate(&1, &1000);

    let current_ts = env.ledger().timestamp();
    let score_a_ts = current_ts.saturating_sub(100);

    submit_pair(&env, &client, &wallet, &pair_a, 100, score_a_ts);

    env.ledger().with_mut(|l| l.timestamp = current_ts + 50);

    let score_b_ts = env.ledger().timestamp();
    submit_pair(&env, &client, &wallet, &pair_b, 50, score_b_ts);

    let aggregate = client.get_aggregate_score(&wallet);
    // With decay, older score A should be weighted less → aggregate closer to 50 than 75
    assert!(aggregate.aggregate_score < 75);
    assert!(aggregate.decay_lambda_applied);
}

#[test]
fn test_aggregate_decay_lambda_applied_false_when_zero() {
    let (env, client, _, _) = setup();
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLMU");

    let (num, _) = client.get_decay_rate();
    assert_eq!(num, 0);

    submit_pair(&env, &client, &wallet, &pair, 50, 100);

    let aggregate = client.get_aggregate_score(&wallet);
    assert!(!aggregate.decay_lambda_applied);
}

#[test]
fn test_aggregate_decay_lambda_applied_true_when_nonzero() {
    let (env, client, _, _) = setup();
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLMU");

    client.set_decay_rate(&1, &1000);
    submit_pair(&env, &client, &wallet, &pair, 50, START_TS);

    let aggregate = client.get_aggregate_score(&wallet);
    assert!(aggregate.decay_lambda_applied);
}

#[test]
fn test_decay_applies_consistently_across_pairs() {
    let (env, client, _, _) = setup();
    let wallet = Address::generate(&env);
    let pairs = [symbol_short!("PXL1"), symbol_short!("PXL2"), symbol_short!("PXL3")];

    client.set_decay_rate(&1, &500);

    let ts = env.ledger().timestamp();
    for pair in &pairs {
        client.set_pair_weight(&Vec::new(&env), pair, &1);
        submit_pair(&env, &client, &wallet, pair, 60, ts);
    }

    let aggregate = client.get_aggregate_score(&wallet);
    // All pairs same age → same decay factor → aggregate = 60
    assert_eq!(aggregate.aggregate_score, 60);
    assert_eq!(aggregate.pair_count, 3);
}

#[test]
fn test_set_decay_rate_not_initialized_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, crate::ScoreGateScoreContract);
    let client = crate::ScoreGateScoreContractClient::new(&env, &contract_id);
    // Contract not yet initialized — must return NotInitialized
    assert_eq!(client.try_set_decay_rate(&1, &1000), Err(Ok(Error::NotInitialized)));
}

#[test]
#[should_panic]
fn test_set_decay_rate_non_admin_rejected() {
    let env = Env::default();
    // No mock_all_auths — auth must be provided explicitly
    let contract_id = env.register_contract(None, crate::ScoreGateScoreContract);
    let client = crate::ScoreGateScoreContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &service);
    env.set_auths(&[]);
    // Calling without admin auth must panic
    client.set_decay_rate(&1, &1000);
}

// ── Issue #188: decay / interpolation math on arbitrarily large elapsed times ──

/// Ten years of elapsed time in seconds (365-day years), the horizon span
/// required by the issue #188 acceptance criteria.
const TEN_YEARS: u64 = 10 * 365 * 24 * 3600; // 315_360_000 s

/// `decay_fixed` must clamp the age to `MAX_DECAY_HORIZON_SECS`: the factor
/// evaluated exactly at the horizon equals the factor evaluated far beyond
/// it (saturation), while ages below the horizon keep decaying. This also
/// pins the u128 saturating intermediates — a 10-year age used to overflow
/// the u64 scaling chain for sufficiently large numerators.
#[test]
fn test_decay_fixed_saturates_at_max_decay_horizon() {
    // λ = 1/100_000_000 (slow decay — the factor is still > 0 at the horizon,
    // so saturation is observable instead of collapsing to zero).
    let num = 1u64;
    let den = 100_000_000u64;

    let at_horizon =
        ScoreGateScoreContract::decay_fixed(crate::constants::MAX_DECAY_HORIZON_SECS, num, den);
    let ten_years = ScoreGateScoreContract::decay_fixed(TEN_YEARS, num, den);
    let five_days = ScoreGateScoreContract::decay_fixed(5 * 86_400, num, den);

    // Older than the horizon → saturated to the value at the horizon
    // (the neutral baseline beyond the decay horizon), never an overflow.
    assert_eq!(at_horizon, ten_years);
    // Below the horizon decay is still progressing.
    assert!(five_days > at_horizon);
    assert!(at_horizon < crate::constants::DECAY_FIXED_POINT_SCALE);
    assert!(five_days <= crate::constants::DECAY_FIXED_POINT_SCALE);

    // λ = 0 short-circuits regardless of age.
    assert_eq!(
        ScoreGateScoreContract::decay_fixed(TEN_YEARS, 0, 1),
        crate::constants::DECAY_FIXED_POINT_SCALE
    );

    // Even an absurd age (far beyond 10 years) saturates without aborting.
    assert_eq!(ScoreGateScoreContract::decay_fixed(u64::MAX, num, den), ten_years);
}

/// A score ten years old — well past `MAX_DECAY_HORIZON_SECS` — evaluates to
/// the neutral score floor (zero effective contribution) instead of aborting.
#[test]
fn test_effective_score_10_year_old_saturates_to_floor() {
    let (env, client, _, _) = setup();
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLMU");

    client.set_decay_rate(&1, &1000);
    submit_pair(&env, &client, &wallet, &pair, 80, START_TS);

    // Ten years later — no overflow, fully decayed to the floor.
    env.ledger().with_mut(|l| l.timestamp = START_TS + TEN_YEARS);
    let eff = client.get_effective_score(&wallet, &pair);
    assert_eq!(eff.original_score, 80);
    assert_eq!(eff.effective_score, 0, "scores beyond the decay horizon reach the neutral floor");

    // One day past the horizon evaluates to the same saturated floor.
    env.ledger()
        .with_mut(|l| l.timestamp = START_TS + crate::constants::MAX_DECAY_HORIZON_SECS + 86_400);
    let eff_at_horizon = client.get_effective_score(&wallet, &pair);
    assert_eq!(eff_at_horizon.effective_score, eff.effective_score);
}

/// In the weighted aggregate a 10-year-old score contributes zero weight
/// (its decay factor saturated to the floor) while a fresh score dominates.
#[test]
fn test_aggregate_10_year_old_score_weight_saturates_to_floor() {
    let (env, client, _, _) = setup();
    let wallet = Address::generate(&env);
    let old_pair = symbol_short!("OLDP");
    let new_pair = symbol_short!("NEWP");

    client.set_pair_weight(&Vec::new(&env), &old_pair, &1);
    client.set_pair_weight(&Vec::new(&env), &new_pair, &1);
    client.set_decay_rate(&1, &1000);

    submit_pair(&env, &client, &wallet, &old_pair, 90, START_TS);

    env.ledger().with_mut(|l| l.timestamp = START_TS + TEN_YEARS);
    submit_pair(&env, &client, &wallet, &new_pair, 40, START_TS + TEN_YEARS);

    let aggregate = client.get_aggregate_score(&wallet);
    assert!(aggregate.decay_lambda_applied);
    assert_eq!(aggregate.pair_count, 2);
    // The old pair's decayed weight hit the floor, so only the fresh score
    // contributes: (0 + 40·1) / (0 + 1) = 40.
    assert_eq!(aggregate.aggregate_score, 40);
}

/// Interpolating between history entries ten years apart must not overflow:
/// endpoints stay exact and the midpoint is the smooth interpolated value.
#[test]
fn test_interpolation_across_10_year_gap_does_not_overflow() {
    let (env, client, _, _) = setup();
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLMU");

    submit_pair(&env, &client, &wallet, &pair, 20, START_TS);
    env.ledger().with_mut(|l| l.timestamp = START_TS + TEN_YEARS);
    submit_pair(&env, &client, &wallet, &pair, 60, START_TS + TEN_YEARS);

    // Exact endpoints.
    assert_eq!(client.get_interpolated_score(&wallet, &pair, &START_TS), 20);
    assert_eq!(client.get_interpolated_score(&wallet, &pair, &(START_TS + TEN_YEARS)), 60);
    // Midpoint of a 10-year segment: 20 + (60−20)·½ = 40.
    assert_eq!(client.get_interpolated_score(&wallet, &pair, &(START_TS + TEN_YEARS / 2)), 40);
    // Far outside the history range — boundary values, no overflow.
    assert_eq!(
        client.get_interpolated_score(&wallet, &pair, &(START_TS.saturating_sub(TEN_YEARS))),
        20
    );
    assert_eq!(client.get_interpolated_score(&wallet, &pair, &(START_TS + 2 * TEN_YEARS)), 60);
}
