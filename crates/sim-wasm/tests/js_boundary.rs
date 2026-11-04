#![cfg(target_arch = "wasm32")]

use attempt_sim_wasm::{PlanSearch, estimate_maxes, evaluate, single_changes, tradeoff};
use js_sys::JSON;
use tsify::{Ts, Tsify};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

const REQUEST: &str = r#"{
    "lifter": {
        "lifts": {
            "squat": { "expectedMaxKg": 200, "curve": { "ratioAtHalf": 1, "steepness": 1000 } },
            "bench": { "expectedMaxKg": 130, "curve": { "ratioAtHalf": 1, "steepness": 1000 } },
            "deadlift": { "expectedMaxKg": 240, "curve": { "ratioAtHalf": 1, "steepness": 1000 } }
        },
        "dayForm": { "spread": 0, "correlation": 0.5 }
    },
    "plans": {
        "squat": { "openerKg": 180, "second": { "afterMakeKg": 10, "afterMissKg": 0 }, "third": { "afterMakeKg": 15, "afterMissKg": 0 } },
        "bench": { "openerKg": 117.5, "second": { "afterMakeKg": 7.5, "afterMissKg": 0 }, "third": { "afterMakeKg": 7.5, "afterMissKg": 0 } },
        "deadlift": { "openerKg": 215, "second": { "afterMakeKg": 15, "afterMissKg": 0 }, "third": { "afterMakeKg": 5, "afterMissKg": 0 } }
    },
    "simulations": 200,
    "seed": 7
}"#;

fn parse<T: Tsify>(json: &str) -> Ts<T> {
    Ts::new_unchecked(JSON::parse(json).unwrap())
}

fn stringify<T: Tsify>(value: Ts<T>) -> String {
    JSON::stringify(&JsValue::from(value)).unwrap().into()
}

fn with_fields(fields: &str) -> String {
    REQUEST.replace(r#""seed": 7"#, &format!(r#""seed": 7, {fields}"#))
}

fn call(json: &str) -> String {
    stringify(evaluate(parse(json)).unwrap())
}

#[wasm_bindgen_test]
fn returns_an_evaluation_with_camel_case_fields() {
    let json = call(REQUEST);
    assert!(
        json.starts_with(r#"{"kind":"evaluated","evaluation":{"#),
        "{json}"
    );
    assert!(json.contains(r#""expectedTotalKg":550"#), "{json}");
    assert!(
        json.contains(
            r#""bench":{"makeProbability":1,"medianBestKg":125,"strengthMissRate":0.3333333333333333,"technicalMissRate":0,"attemptMakeProbabilities":[1,1,0],"technicalBombOutProbability":0,"paths":[{"made":[true,true,false],"bestKg":125,"share":1}"#
        ),
        "{json}"
    );
}

#[wasm_bindgen_test]
fn leaves_out_goal_figures_without_a_goal() {
    let json = call(REQUEST);
    assert!(!json.contains("goalProbability"), "{json}");
    let with_goal = call(&with_fields(r#""goalKg": 550"#));
    assert!(with_goal.contains(r#""goalProbability":1"#), "{with_goal}");
}

#[wasm_bindgen_test]
fn returns_problems_for_an_invalid_plan() {
    let json = call(&REQUEST.replace(r#""openerKg": 180"#, r#""openerKg": 181"#));
    assert_eq!(
        json,
        r#"{"kind":"invalid","problems":[{"lift":"squat","field":"opener","code":"notLoadable","message":"attempt 1 is not a multiple of 2.5 kg"}]}"#
    );
}

#[wasm_bindgen_test]
fn throws_on_input_of_the_wrong_shape() {
    assert!(evaluate(parse(r#"{"plans": 3}"#)).is_err());
}

#[wasm_bindgen_test]
fn sweeps_the_plan_family_with_camel_case_fields() {
    let json = stringify(tradeoff(parse(&with_fields(r#""goalKg": 550"#))).unwrap());
    assert!(
        json.starts_with(
            r#"{"kind":"swept","points":[{"shape":{"openerPercent":85,"secondPercent":93,"thirdPercent":97},"plans":{"squat":{"openerKg":170,"second":{"afterMakeKg":15,"afterMissKg":0}"#
        ),
        "{json}"
    );
    assert!(json.contains(r#""onFrontier":true"#), "{json}");
    assert!(json.contains(r#""bestForGoal":"#), "{json}");
}

#[wasm_bindgen_test]
fn the_sweep_reads_the_bomb_out_cap() {
    let json = stringify(tradeoff(parse(&with_fields(r#""bombOutCap": 3"#))).unwrap());
    assert!(json.contains(r#""field":"bombOutCap""#), "{json}");
}

#[wasm_bindgen_test]
fn a_search_steps_until_done() {
    let request = with_fields(r#""goalKg": 560, "bombOutCap": 0.05"#);
    let mut search = PlanSearch::new(parse(&request)).unwrap();
    let json = loop {
        let json = stringify(search.step().unwrap());
        if json.contains(r#""done":true"#) {
            break json;
        }
    };
    assert!(json.starts_with(r#"{"round":"#), "{json}");
    assert!(json.contains(r#""plans":{"squat":{"openerKg":"#), "{json}");
    assert!(json.contains(r#""goalProbability":"#), "{json}");
}

#[wasm_bindgen_test]
fn a_search_with_an_invalid_cap_throws_its_problem() {
    assert!(PlanSearch::new(parse(&with_fields(r#""bombOutCap": 3"#))).is_err());
}

const PAST_MEETS: &str = r#"{
    "curves": {
        "squat": { "ratioAtHalf": 1, "steepness": 60 },
        "bench": { "ratioAtHalf": 1, "steepness": 60 },
        "deadlift": { "ratioAtHalf": 1, "steepness": 60 }
    },
    "typicalOpener": { "squat": 0.9, "bench": 0.9, "deadlift": 0.9 },
    "dayFormSpread": 0.03,
    "meets": [{
        "day": 19784,
        "attempts": {
            "squat": [{ "kg": 180, "made": true }, { "kg": 190, "made": true }, { "kg": 197.5, "made": false }],
            "bench": [],
            "deadlift": []
        }
    }],
    "onDay": 19800
}"#;

#[wasm_bindgen_test]
fn estimates_maxes_from_past_meets() {
    let json = stringify(estimate_maxes(parse(PAST_MEETS)).unwrap());
    assert!(
        json.starts_with(r#"{"lifts":{"squat":{"expectedMaxKg":"#),
        "{json}"
    );
    assert!(!json.contains(r#""bench""#), "{json}");
    assert!(json.contains(r#""yearlyProgress":"#), "{json}");
}

#[wasm_bindgen_test]
fn estimating_from_a_negative_weight_throws() {
    let json = PAST_MEETS.replace(r#""kg": 180"#, r#""kg": -180"#);
    assert!(estimate_maxes(parse(&json)).is_err());
}

#[wasm_bindgen_test]
fn lists_single_changes_with_camel_case_fields() {
    let json = stringify(single_changes(parse(REQUEST)).unwrap());
    assert!(
        json.starts_with(r#"{"kind":"changed","current":{"plans":"#),
        "{json}"
    );
    assert!(json.contains(r#""setting":""#), "{json}");
    assert!(json.contains(r#""weightKg":"#), "{json}");
    assert!(json.contains(r#""improves":"#), "{json}");
    assert!(
        !json.contains(r#""setting":"secondAfterMiss""#),
        "every second attempt is made, so a jump after a miss changes nothing: {json}"
    );
}

#[wasm_bindgen_test]
fn an_evaluation_lists_every_total() {
    let json = call(REQUEST);
    assert!(
        json.contains(r#""totals":[{"totalKg":550,"share":1}]"#),
        "{json}"
    );
}
