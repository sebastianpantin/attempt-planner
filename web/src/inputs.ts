import type {
  EvaluateRequest,
  InputField,
  Lift,
  LiftPlanKg,
  PerLift,
  Problem,
  SearchRequest,
  TradeoffRequest,
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
  "technicalMissPercent",
] as const;

const MEET_FIELDS = [
  "goalKg",
  "spreadPercent",
  "maxUncertaintyPercent",
  "correlation",
  "bombOutCapPercent",
] as const;

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

/** The lifter, goal and bomb-out cap for the tradeoff sweep, always at the quick count. */
export function buildTradeoffRequest(values: FormValues): TradeoffRequest {
  const read = (lift: Lift, field: LiftField) => values[inputName(lift, field)];
  return {
    lifter: {
      lifts: perLift((lift) => ({
        expectedMaxKg: read(lift, "expectedMaxKg"),
        curve: {
          ratioAtHalf: read(lift, "ratioAtHalf"),
          steepness: read(lift, "steepness"),
          technicalMissChance: read(lift, "technicalMissPercent") / 100,
        },
      })),
      dayForm: {
        spread: values.spreadPercent / 100,
        maxUncertainty: values.maxUncertaintyPercent / 100,
        correlation: values.correlation,
      },
    },
    ...(Number.isNaN(values.goalKg) ? {} : { goalKg: values.goalKg }),
    ...(Number.isNaN(values.bombOutCapPercent)
      ? {}
      : { bombOutCap: values.bombOutCapPercent / 100 }),
    simulations: QUICK_SIMULATIONS,
    seed: SEED,
  };
}

export function buildRequest(
  values: FormValues,
  precise: boolean,
): EvaluateRequest {
  const read = (lift: Lift, field: LiftField) => values[inputName(lift, field)];
  const { bombOutCap, ...sweep } = buildTradeoffRequest(values);
  return {
    ...sweep,
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
    simulations: precise ? PRECISE_SIMULATIONS : QUICK_SIMULATIONS,
  };
}

/** The lifter, goal and current plans for a search, with the bomb-out cap in percent. */
export function buildSearchRequest(values: FormValues): SearchRequest {
  return {
    ...buildTradeoffRequest(values),
    plans: buildRequest(values, false).plans,
  };
}

/** The plan inputs, with their values, that make `buildRequest` carry these plans. */
export function planValues(plans: PerLift<LiftPlanKg>): [InputName, number][] {
  return LIFTS.flatMap((lift) => {
    const plan = plans[lift];
    const values: [LiftField, number][] = [
      ["openerKg", plan.openerKg],
      ["secondAfterMakeKg", plan.second.afterMakeKg],
      ["secondAfterMissKg", plan.second.afterMissKg],
      ["thirdAfterMakeKg", plan.third.afterMakeKg],
      ["thirdAfterMissKg", plan.third.afterMissKg],
    ];
    return values.map(([field, kg]) => [inputName(lift, field), kg]);
  });
}

const PROBLEM_LIFT_FIELDS: Partial<Record<InputField, LiftField[]>> = {
  expectedMax: ["expectedMaxKg"],
  curve: ["ratioAtHalf", "steepness", "technicalMissPercent"],
  opener: ["openerKg"],
  second: ["secondAfterMakeKg", "secondAfterMissKg"],
  third: ["thirdAfterMakeKg", "thirdAfterMissKg"],
};

const PROBLEM_MEET_FIELDS: Partial<Record<InputField, MeetField[]>> = {
  goal: ["goalKg"],
  dayForm: ["spreadPercent", "maxUncertaintyPercent", "correlation"],
  bombOutCap: ["bombOutCapPercent"],
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
  bombOutCap: "Bomb-out cap",
};

/** Messages restated in the page's percent units where the engine speaks in fractions. */
const PERCENT_MESSAGES: Partial<Record<Problem["code"], string>> = {
  invalidBombOutCap: "must be from 0 to 100%",
  invalidCurve:
    "needs a positive midpoint and steepness, and a technical-miss chance from 0 to below 100%",
  invalidDayForm:
    "spreads must be at least 0% and combine to at most 50%, and correlation must be from 0 to 1",
};

export const describeProblem = ({ field, code, message }: Problem) =>
  `${FIELD_LABELS[field]}: ${PERCENT_MESSAGES[code] ?? message}`;
