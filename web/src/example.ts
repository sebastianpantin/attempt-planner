import type { FormValues } from "./inputs.ts";

/**
 * The make-rate curve calibrated to OpenPowerlifting raw full-power meets by
 * `crates/fit`: the midpoint is the expected max by definition, and each lift's
 * steepness matches how often lifters make their 1st, 2nd and 3rd attempts.
 */
const RATIO_AT_HALF = 1;
const STEEPNESS = { squat: 19, bench: 59, deadlift: 49 };

export const EXAMPLE: FormValues = {
  "squat-expectedMaxKg": 200,
  "squat-openerKg": 182.5,
  "squat-secondAfterMakeKg": 12.5,
  "squat-secondAfterMissKg": 0,
  "squat-thirdAfterMakeKg": 7.5,
  "squat-thirdAfterMissKg": 0,
  "squat-ratioAtHalf": RATIO_AT_HALF,
  "squat-steepness": STEEPNESS.squat,
  "bench-expectedMaxKg": 130,
  "bench-openerKg": 120,
  "bench-secondAfterMakeKg": 7.5,
  "bench-secondAfterMissKg": 0,
  "bench-thirdAfterMakeKg": 5,
  "bench-thirdAfterMissKg": 0,
  "bench-ratioAtHalf": RATIO_AT_HALF,
  "bench-steepness": STEEPNESS.bench,
  "deadlift-expectedMaxKg": 240,
  "deadlift-openerKg": 220,
  "deadlift-secondAfterMakeKg": 15,
  "deadlift-secondAfterMissKg": 0,
  "deadlift-thirdAfterMakeKg": 10,
  "deadlift-thirdAfterMissKg": 0,
  "deadlift-ratioAtHalf": RATIO_AT_HALF,
  "deadlift-steepness": STEEPNESS.deadlift,
  goalKg: 550,
  spreadPercent: 3,
  correlation: 0.5,
  bombOutCapPercent: 5,
};
