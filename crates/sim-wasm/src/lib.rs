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
