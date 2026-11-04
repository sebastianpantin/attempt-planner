import type { FormValues } from "./inputs.ts";

/**
 * Calibrated to raw full-power IPF meets in OpenPowerlifting by `crates/fit`: the
 * midpoint is the expected max by definition, and each lift's steepness and
 * technical-miss chance, with one spread of the day max, match how often lifters make
 * each attempt after a make and after a miss. The day-to-day part of that spread and
 * the correlation between lifts come from lifters' consecutive meets.
 */
const RATIO_AT_HALF = 1;
const STEEPNESS = { squat: 56, bench: 94, deadlift: 109 };
const TECHNICAL_MISS_PERCENT = { squat: 0, bench: 3, deadlift: 1 };
const SPREAD_PERCENT = 3;
/** The fitted typical opener, as a share of the expected max. */
export const TYPICAL_OPENER = { squat: 0.89, bench: 0.91, deadlift: 0.89 };
const MAX_UNCERTAINTY_PERCENT = 3;

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
  maxUncertaintyPercent: MAX_UNCERTAINTY_PERCENT,
  correlation: 0.4,
  bombOutCapPercent: 5,
};
