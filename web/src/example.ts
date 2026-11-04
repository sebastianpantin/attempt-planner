import type { FormValues } from "./inputs.ts";

/**
 * Calibrated to OpenPowerlifting raw full-power meets by `crates/fit`: the midpoint is
 * the expected max by definition, and each lift's steepness and technical-miss chance,
 * with one day-form spread, match how often lifters make each attempt after a make
 * and after a miss.
 */
const RATIO_AT_HALF = 1;
const STEEPNESS = { squat: 53, bench: 90, deadlift: 106 };
const TECHNICAL_MISS_PERCENT = { squat: 0, bench: 3, deadlift: 1.5 };
const SPREAD_PERCENT = 4.5;

export const EXAMPLE: FormValues = {
  "squat-expectedMaxKg": 200,
  "squat-openerKg": 182.5,
  "squat-secondAfterMakeKg": 12.5,
  "squat-secondAfterMissKg": 0,
  "squat-thirdAfterMakeKg": 7.5,
  "squat-thirdAfterMissKg": 0,
  "squat-ratioAtHalf": RATIO_AT_HALF,
  "squat-steepness": STEEPNESS.squat,
  "squat-technicalMissPercent": TECHNICAL_MISS_PERCENT.squat,
  "bench-expectedMaxKg": 130,
  "bench-openerKg": 120,
  "bench-secondAfterMakeKg": 7.5,
  "bench-secondAfterMissKg": 0,
  "bench-thirdAfterMakeKg": 5,
  "bench-thirdAfterMissKg": 0,
  "bench-ratioAtHalf": RATIO_AT_HALF,
  "bench-steepness": STEEPNESS.bench,
  "bench-technicalMissPercent": TECHNICAL_MISS_PERCENT.bench,
  "deadlift-expectedMaxKg": 240,
  "deadlift-openerKg": 220,
  "deadlift-secondAfterMakeKg": 15,
  "deadlift-secondAfterMissKg": 0,
  "deadlift-thirdAfterMakeKg": 10,
  "deadlift-thirdAfterMissKg": 0,
  "deadlift-ratioAtHalf": RATIO_AT_HALF,
  "deadlift-steepness": STEEPNESS.deadlift,
  "deadlift-technicalMissPercent": TECHNICAL_MISS_PERCENT.deadlift,
  goalKg: 550,
  spreadPercent: SPREAD_PERCENT,
  correlation: 0.5,
  bombOutCapPercent: 5,
};
