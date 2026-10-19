#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    Address, Env, Vec,
};

use crate::{InterpolationMethod, ScoreGateScoreContract, ScoreGateScoreContractClient};

const START_TS: u64 = 1_700_000_000;

fn setup<'a>() -> (Env, ScoreGateScoreContractClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);
    let id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    client.initialize(&admin, &service);
    (env, client)
}

// ── Issue #187: get_interpolated_score honours the configured method ─────────

/// Submits a score at an explicit history timestamp while advancing the
/// ledger past the cooldown (mirrors `submit` with a custom `ts`).
fn submit_at(
    env: &Env,
    client: &ScoreGateScoreContractClient<'_>,
    wallet: &Address,
    pair: &soroban_sdk::Symbol,
    score: u32,
    ts: u64,
) {
    env.ledger().with_mut(|l| l.timestamp += 3_601);
    client.submit_score(&Vec::new(env), wallet, pair, &score, &false, &false, &ts, &80, &1, &None);
}

/// With `InterpolationMethod::CubicSpline` configured, `get_interpolated_score`
/// delegates to `cubic_spline_interpolate`; `Linear` keeps decay-curve
/// interpolation. Expected values are the exact fixed-point results of both
/// algorithms for the first segment of `[0, 100, 100]` at `t₀ + 30 s`:
/// linear → 50, Catmull-Rom (clamped end tangents t1 = 100, t2 = 50) → 56.
#[test]
fn test_get_interpolated_score_honours_configured_method() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    // Linear is the default method.
    assert_eq!(client.get_interpolation_method(), InterpolationMethod::Linear);

    submit_at(&env, &client, &wallet, &pair, 0, START_TS);
    submit_at(&env, &client, &wallet, &pair, 100, START_TS + 60);
    submit_at(&env, &client, &wallet, &pair, 100, START_TS + 120);

    // Linear (default): midpoint = 0 + (100 − 0) · ½ = 50.
    assert_eq!(client.get_interpolated_score(&wallet, &pair, &(START_TS + 30)), 50);

    // Configure CubicSpline — the identical query now runs through
    // cubic_spline_interpolate.
    client.set_interpolation_method(&InterpolationMethod::CubicSpline);
    assert_eq!(client.get_interpolation_method(), InterpolationMethod::CubicSpline);
    assert_eq!(client.get_interpolated_score(&wallet, &pair, &(START_TS + 30)), 56);

    // Exact history timestamps and out-of-range timestamps stay exact under
    // the spline (boundary short-circuits precede the dispatch).
    assert_eq!(client.get_interpolated_score(&wallet, &pair, &START_TS), 0);
    assert_eq!(client.get_interpolated_score(&wallet, &pair, &(START_TS + 60)), 100);
    assert_eq!(client.get_interpolated_score(&wallet, &pair, &(START_TS.saturating_sub(60))), 0);
    assert_eq!(client.get_interpolated_score(&wallet, &pair, &(START_TS + 360)), 100);

    // Switching back to Linear restores curve interpolation.
    client.set_interpolation_method(&InterpolationMethod::Linear);
    assert_eq!(client.get_interpolated_score(&wallet, &pair, &(START_TS + 30)), 50);
}

/// With `CubicSpline` configured but only two history points, neighbour
/// tangents cannot be estimated — the dispatch falls back to curve
/// interpolation (midpoint = 50, not a spline value).
#[test]
fn test_cubic_spline_falls_back_with_insufficient_history() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    client.set_interpolation_method(&InterpolationMethod::CubicSpline);
    submit_at(&env, &client, &wallet, &pair, 0, START_TS);
    submit_at(&env, &client, &wallet, &pair, 100, START_TS + 60);

    assert_eq!(client.get_interpolated_score(&wallet, &pair, &(START_TS + 30)), 50);
}
