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

/// Estimates each lift's expected max from the attempts at past meets.
///
/// Throws when the input has the wrong shape or any problem, with every problem's message.
#[wasm_bindgen(js_name = estimateMaxes)]
pub fn estimate_maxes(
    request: Ts<attempt_sim::past_meets::MaxEstimateInput>,
) -> Result<Ts<attempt_sim::past_meets::MaxEstimate>, JsError> {
    let estimate =
        attempt_sim::past_meets::estimate_maxes(&request.to_rust()?).map_err(|problems| {
            let messages: Vec<_> = problems.iter().map(ToString::to_string).collect();
            JsError::new(&messages.join("; "))
        })?;
    Ok(estimate.into_ts()?)
}

/// A plan search the caller steps through, so it can report progress and stop early.
#[wasm_bindgen]
pub struct PlanSearch(attempt_sim::search::Search);

#[wasm_bindgen]
impl PlanSearch {
    /// Sets up a search. Throws when the input has the wrong shape or any problem, with
    /// every problem's message.
    #[wasm_bindgen(constructor)]
    pub fn new(request: Ts<dto::SearchRequest>) -> Result<PlanSearch, JsError> {
        dto::start_search(&request.to_rust()?)
            .map(PlanSearch)
            .map_err(|problems| {
                let messages: Vec<_> = problems.into_iter().map(|p| p.message).collect();
                JsError::new(&messages.join("; "))
            })
    }

    /// Tries the next setting and returns the best plans so far.
    pub fn step(&mut self) -> Result<Ts<dto::SearchProgress>, JsError> {
        Ok(dto::SearchProgress::from(self.0.step()).into_ts()?)
    }
}
