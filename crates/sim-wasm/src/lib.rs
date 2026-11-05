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
    let estimate = attempt_sim::past_meets::estimate_maxes(&request.to_rust()?)
        .map_err(|problems| joined_error(problems.iter().map(ToString::to_string)))?;
    Ok(estimate.into_ts()?)
}

/// Scores every one-step change to the request's plans on the same simulated meets.
///
/// Returns the changes, best first, or every problem with the input. Throws only when the
/// input does not have the expected shape.
#[wasm_bindgen(js_name = singleChanges)]
pub fn single_changes(
    request: Ts<dto::SearchRequest>,
) -> Result<Ts<dto::ChangesResponse>, JsError> {
    Ok(dto::single_changes(&request.to_rust()?).into_ts()?)
}

fn joined_error(messages: impl Iterator<Item = String>) -> JsError {
    JsError::new(&messages.collect::<Vec<_>>().join("; "))
}

/// A plan search the caller steps through, so it can report progress and stop early.
#[wasm_bindgen]
pub struct PlanSearch(attempt_sim::search::Search);

#[wasm_bindgen]
impl PlanSearch {
    /// Throws when the input has the wrong shape or any problem, with every problem's
    /// message.
    #[wasm_bindgen(constructor)]
    pub fn new(request: Ts<dto::SearchRequest>) -> Result<PlanSearch, JsError> {
        dto::start_search(&request.to_rust()?)
            .map(PlanSearch)
            .map_err(|problems| joined_error(problems.into_iter().map(|p| p.message)))
    }

    /// Tries the next setting and returns the best plans so far.
    pub fn step(&mut self) -> Result<Ts<dto::SearchProgress>, JsError> {
        Ok(dto::SearchProgress::from(self.0.step()).into_ts()?)
    }
}
