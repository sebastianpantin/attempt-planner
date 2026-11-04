use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

pub mod dto;

/// Simulates the meet for the given lifter and plans.
///
/// Returns the evaluation, or every problem with the input. Throws only when the input
/// does not have the expected shape.
#[wasm_bindgen]
pub fn evaluate(request: Ts<dto::EvaluateRequest>) -> Result<Ts<dto::EvaluateResponse>, JsError> {
    Ok(dto::evaluate(&request.to_rust()?).into_ts()?)
}

/// Sweeps the plan family for the given lifter on the same simulated meets.
///
/// Returns every plan that fits the lifter's maxes, or every problem with the input.
/// Throws only when the input does not have the expected shape.
#[wasm_bindgen]
pub fn tradeoff(request: Ts<dto::TradeoffRequest>) -> Result<Ts<dto::TradeoffResponse>, JsError> {
    Ok(dto::tradeoff(&request.to_rust()?).into_ts()?)
}
