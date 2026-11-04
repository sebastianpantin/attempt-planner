import type { FormValues } from "./inputs.ts";

/** Hand-picked until the make-rate curve is fitted to OpenPowerlifting results. */
const PLACEHOLDER_RATIO_AT_HALF = 1;
const PLACEHOLDER_STEEPNESS = 30;

export const EXAMPLE: FormValues = {
  "squat-expectedMaxKg": 200,
  "squat-openerKg": 182.5,
  "squat-secondAfterMakeKg": 12.5,
  "squat-secondAfterMissKg": 0,
  "squat-thirdAfterMakeKg": 7.5,
  "squat-thirdAfterMissKg": 0,
  "squat-ratioAtHalf": PLACEHOLDER_RATIO_AT_HALF,
  "squat-steepness": PLACEHOLDER_STEEPNESS,
  "bench-expectedMaxKg": 130,
  "bench-openerKg": 120,
  "bench-secondAfterMakeKg": 7.5,
  "bench-secondAfterMissKg": 0,
  "bench-thirdAfterMakeKg": 5,
  "bench-thirdAfterMissKg": 0,
  "bench-ratioAtHalf": PLACEHOLDER_RATIO_AT_HALF,
  "bench-steepness": PLACEHOLDER_STEEPNESS,
  "deadlift-expectedMaxKg": 240,
  "deadlift-openerKg": 220,
  "deadlift-secondAfterMakeKg": 15,
  "deadlift-secondAfterMissKg": 0,
  "deadlift-thirdAfterMakeKg": 10,
  "deadlift-thirdAfterMissKg": 0,
  "deadlift-ratioAtHalf": PLACEHOLDER_RATIO_AT_HALF,
  "deadlift-steepness": PLACEHOLDER_STEEPNESS,
  goalKg: 550,
  spreadPercent: 3,
  correlation: 0.5,
};
