import type {
  EvaluateRequest,
  InputField,
  Lift,
  PerLift,
  Problem,
} from "../pkg/attempt_sim_wasm.js";

export const LIFTS = ["squat", "bench", "deadlift"] as const satisfies Lift[];

export const QUICK_SIMULATIONS = 20_000;
export const PRECISE_SIMULATIONS = 1_000_000;
const SEED = 1;

const LIFT_FIELDS = [
  "expectedMaxKg",
  "openerKg",
  "secondAfterMakeKg",
  "secondAfterMissKg",
  "thirdAfterMakeKg",
  "thirdAfterMissKg",
  "ratioAtHalf",
  "steepness",
] as const;

const MEET_FIELDS = ["goalKg", "spreadPercent", "correlation"] as const;

export type LiftField = (typeof LIFT_FIELDS)[number];
export type MeetField = (typeof MEET_FIELDS)[number];
export type InputName = `${Lift}-${LiftField}` | MeetField;

/** Every number input on the page by name. An empty input reads as `NaN`. */
export type FormValues = Record<InputName, number>;

export const inputName = (lift: Lift, field: LiftField): InputName =>
  `${lift}-${field}`;

export const INPUT_NAMES: InputName[] = [
  ...LIFTS.flatMap((lift) =>
    LIFT_FIELDS.map((field) => inputName(lift, field)),
  ),
  ...MEET_FIELDS,
];

const perLift = <T>(value: (lift: Lift) => T): PerLift<T> => ({
  squat: value("squat"),
  bench: value("bench"),
  deadlift: value("deadlift"),
});

export function buildRequest(
  values: FormValues,
  precise: boolean,
): EvaluateRequest {
  const read = (lift: Lift, field: LiftField) => values[inputName(lift, field)];
  return {
    lifter: {
      lifts: perLift((lift) => ({
        expectedMaxKg: read(lift, "expectedMaxKg"),
        curve: {
          ratioAtHalf: read(lift, "ratioAtHalf"),
          steepness: read(lift, "steepness"),
        },
      })),
      dayForm: {
        spread: values.spreadPercent / 100,
        correlation: values.correlation,
      },
    },
    plans: perLift((lift) => ({
      openerKg: read(lift, "openerKg"),
      second: {
        afterMakeKg: read(lift, "secondAfterMakeKg"),
        afterMissKg: read(lift, "secondAfterMissKg"),
      },
      third: {
        afterMakeKg: read(lift, "thirdAfterMakeKg"),
        afterMissKg: read(lift, "thirdAfterMissKg"),
      },
    })),
    ...(Number.isNaN(values.goalKg) ? {} : { goalKg: values.goalKg }),
    simulations: precise ? PRECISE_SIMULATIONS : QUICK_SIMULATIONS,
    seed: SEED,
  };
}

const PROBLEM_LIFT_FIELDS: Partial<Record<InputField, LiftField[]>> = {
  expectedMax: ["expectedMaxKg"],
  curve: ["ratioAtHalf", "steepness"],
  opener: ["openerKg"],
  second: ["secondAfterMakeKg", "secondAfterMissKg"],
  third: ["thirdAfterMakeKg", "thirdAfterMissKg"],
};

const PROBLEM_MEET_FIELDS: Partial<Record<InputField, MeetField[]>> = {
  goal: ["goalKg"],
  dayForm: ["spreadPercent", "correlation"],
};

/** The inputs to mark for a problem the engine found. */
export function inputsFor({ lift, field }: Problem): InputName[] {
  if (lift) {
    return (PROBLEM_LIFT_FIELDS[field] ?? []).map((f) => inputName(lift, f));
  }
  return PROBLEM_MEET_FIELDS[field] ?? [];
}

const FIELD_LABELS: Record<InputField, string> = {
  expectedMax: "Expected max",
  curve: "Make rate",
  opener: "Opener",
  second: "2nd attempt",
  third: "3rd attempt",
  dayForm: "Day form",
  simulations: "Simulations",
  goal: "Goal",
};

export const describeProblem = ({ field, message }: Problem) =>
  `${FIELD_LABELS[field]}: ${message}`;
