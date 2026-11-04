use attempt_sim::rules::{Attempt, BAR, PLATE_STEP, Weight, check_progression};
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

#[wasm_bindgen(js_name = barKg)]
pub fn bar_kg() -> f64 {
    BAR.kg()
}

#[wasm_bindgen(js_name = plateStepKg)]
pub fn plate_step_kg() -> f64 {
    PLATE_STEP.kg()
}

/// Checks one lift's attempts against the attempt rules.
///
/// Returns `undefined` when the attempts are allowed, otherwise the rule they break.
/// Throws when the input itself is malformed.
#[wasm_bindgen(js_name = checkAttempts)]
pub fn check_attempts(weights_kg: &[f64], made: &[u8]) -> Result<Option<String>, JsError> {
    let attempts = to_attempts(weights_kg, made).map_err(|message| JsError::new(&message))?;
    Ok(check_progression(&attempts).err().map(|v| v.to_string()))
}

fn to_attempts(weights_kg: &[f64], made: &[u8]) -> Result<Vec<Attempt>, String> {
    if weights_kg.len() != made.len() {
        return Err("weights and results differ in length".into());
    }
    weights_kg
        .iter()
        .zip(made)
        .map(|(&kg, &made)| {
            let weight = Weight::from_kg(kg).ok_or_else(|| format!("{kg} is not a weight"))?;
            Ok(Attempt {
                weight,
                made: made != 0,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_attempts_convert() {
        let attempts = to_attempts(&[200.0, 210.0], &[1, 0]).unwrap();
        assert_eq!(check_progression(&attempts), Ok(()));
        assert!(!attempts[1].made);
    }

    #[test]
    fn mismatched_lengths_are_rejected() {
        assert!(to_attempts(&[200.0], &[]).is_err());
    }

    #[test]
    fn negative_weights_are_rejected() {
        assert_eq!(
            to_attempts(&[-5.0], &[1]).unwrap_err(),
            "-5 is not a weight"
        );
    }

    #[test]
    fn bar_weight_is_exported_in_kg() {
        assert_eq!(bar_kg(), 20.0);
    }

    #[test]
    fn plate_step_is_exported_in_kg() {
        assert_eq!(plate_step_kg(), 2.5);
    }
}
