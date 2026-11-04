import type { PerLift } from "../pkg/attempt_sim_wasm.js";
import { listOf } from "./format.ts";
import {
  type FormValues,
  INPUT_NAMES,
  type InputName,
  inputName,
  LIFTS,
} from "./inputs.ts";

export type Sex = "any" | "men" | "women";

export interface Calibration {
  steepness: PerLift<number>;
  technicalMissPercent: PerLift<number>;
  spreadPercent: number;
  maxUncertaintyPercent: number;
  correlation: number;
  /** The typical opener, as a share of the expected max. */
  typicalOpener: PerLift<number>;
}

/**
 * Calibrated to raw full-power IPF meets in OpenPowerlifting by `crates/fit`, for all
 * lifters and for each sex: the midpoint is the expected max by definition, and each
 * lift's steepness and technical-miss chance, with one spread of the day max, match how
 * often lifters make each attempt after a make and after a miss. The day-to-day part of
 * that spread and the correlation between lifts come from lifters' consecutive meets.
 */
export const CALIBRATIONS: Record<Sex, Calibration> = {
  any: {
    steepness: { squat: 56, bench: 94, deadlift: 109 },
    technicalMissPercent: { squat: 0, bench: 3, deadlift: 1 },
    spreadPercent: 3,
    maxUncertaintyPercent: 3,
    correlation: 0.4,
    typicalOpener: { squat: 0.89, bench: 0.91, deadlift: 0.89 },
  },
  men: {
    steepness: { squat: 58, bench: 99, deadlift: 116 },
    technicalMissPercent: { squat: 0, bench: 3, deadlift: 1 },
    spreadPercent: 2.9,
    maxUncertaintyPercent: 3,
    correlation: 0.42,
    typicalOpener: { squat: 0.89, bench: 0.91, deadlift: 0.89 },
  },
  women: {
    steepness: { squat: 51, bench: 77, deadlift: 93 },
    technicalMissPercent: { squat: 0, bench: 2, deadlift: 1 },
    spreadPercent: 3.2,
    maxUncertaintyPercent: 3.4,
    correlation: 0.36,
    typicalOpener: { squat: 0.88, bench: 0.9, deadlift: 0.87 },
  },
};

/** The inputs a calibration sets. */
export function calibrationValues(sex: Sex): [InputName, number][] {
  const calibration = CALIBRATIONS[sex];
  return [
    ...LIFTS.flatMap((lift): [InputName, number][] => [
      [inputName(lift, "ratioAtHalf"), 1],
      [inputName(lift, "steepness"), calibration.steepness[lift]],
      [
        inputName(lift, "technicalMissPercent"),
        calibration.technicalMissPercent[lift],
      ],
    ]),
    ["spreadPercent", calibration.spreadPercent],
    ["maxUncertaintyPercent", calibration.maxUncertaintyPercent],
    ["correlation", calibration.correlation],
  ];
}

/**
 * The inputs to set when the lifter group changes from `from` to `to`: those still at
 * `from`'s value, or empty. `kept` lists the values the lifter changed that now differ
 * from the new group's.
 */
export function switchCalibration(
  values: FormValues,
  from: Sex,
  to: Sex,
): { changes: [InputName, number][]; kept: InputName[] } {
  const previous = new Map(calibrationValues(from));
  const changes: [InputName, number][] = [];
  const kept: InputName[] = [];
  for (const [name, value] of calibrationValues(to)) {
    const current = values[name];
    if (current === previous.get(name) || Number.isNaN(current)) {
      changes.push([name, value]);
    } else if (current !== value) {
      kept.push(name);
    }
  }
  return { changes, kept };
}

const CALIBRATED_NAMES = Object.fromEntries([
  ...LIFTS.flatMap((lift) => [
    [inputName(lift, "ratioAtHalf"), `${lift} midpoint`],
    [inputName(lift, "steepness"), `${lift} steepness`],
    [inputName(lift, "technicalMissPercent"), `${lift} technical-miss chance`],
  ]),
  ["spreadPercent", "day-to-day spread"],
  ["maxUncertaintyPercent", "uncertainty of the expected max"],
  ["correlation", "correlation"],
]) as Record<InputName, string>;

/** Tells the lifter which of their own values a group switch left alone. */
export function describeKept(kept: InputName[]): string {
  if (kept.length === 0) return "";
  return `Kept your own ${kept.length === 1 ? "value" : "values"} for ${listOf(kept.map((name) => CALIBRATED_NAMES[name]))}.`;
}

export const EXAMPLE: FormValues = {
  "squat-expectedMaxKg": 200,
  "squat-openerKg": 182.5,
  "squat-secondAfterMakeKg": 12.5,
  "squat-secondAfterMissKg": 0,
  "squat-thirdAfterMakeKg": 7.5,
  "squat-thirdAfterMissKg": 0,
  "bench-expectedMaxKg": 130,
  "bench-openerKg": 120,
  "bench-secondAfterMakeKg": 7.5,
  "bench-secondAfterMissKg": 0,
  "bench-thirdAfterMakeKg": 5,
  "bench-thirdAfterMissKg": 0,
  "deadlift-expectedMaxKg": 240,
  "deadlift-openerKg": 220,
  "deadlift-secondAfterMakeKg": 15,
  "deadlift-secondAfterMissKg": 0,
  "deadlift-thirdAfterMakeKg": 10,
  "deadlift-thirdAfterMissKg": 0,
  goalKg: 550,
  bombOutCapPercent: 5,
  ...Object.fromEntries(calibrationValues("any")),
} as FormValues;

/** The page's first values: the model and a 5% bomb-out limit, with the lifter's own numbers empty. */
export const START: FormValues = {
  ...(Object.fromEntries(
    INPUT_NAMES.map((name) => [name, Number.NaN]),
  ) as FormValues),
  bombOutCapPercent: EXAMPLE.bombOutCapPercent,
  ...Object.fromEntries(calibrationValues("any")),
};
