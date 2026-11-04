import type {
  EvaluateRequest,
  Lift,
  LiftPlanKg,
  PerLift,
  Problem,
  SearchRequest,
  TradeoffRequest,
  TradeoffResponse,
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

/** The jumps to the 2nd and 3rd attempts, after a make and after a miss. */
export const JUMP_FIELDS = [
  "secondAfterMakeKg",
  "secondAfterMissKg",
  "thirdAfterMakeKg",
  "thirdAfterMissKg",
] as const satisfies LiftField[];

/** Jumps after a miss, where 0 repeats the missed weight. */
export const isRepeatJump = (field: LiftField) =>
  field === "secondAfterMissKg" || field === "thirdAfterMissKg";
export type MeetField = (typeof MEET_FIELDS)[number];
export type InputName = `${Lift}-${LiftField}` | MeetField;

/** Every number input on the page by name. An empty input reads as `NaN`. */
export type FormValues = Record<InputName, number>;

export const inputName = (lift: Lift, field: LiftField): InputName =>
  `${lift}-${field}`;

const LIFT_INPUTS = new Map<string, { lift: Lift; field: LiftField }>(
  LIFTS.flatMap((lift) =>
    LIFT_FIELDS.map((field) => [inputName(lift, field), { lift, field }]),
  ),
);

/** The lift and field of a lift input's name, or `undefined` for any other name. */
export const parseInputName = (name: string) => LIFT_INPUTS.get(name);

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

const read = (values: FormValues, lift: Lift, field: LiftField) =>
  values[inputName(lift, field)];

/** The lifter and the optional goal, shared by every request. */
function lifterAndGoal(
  values: FormValues,
): Pick<TradeoffRequest, "lifter" | "goalKg"> {
  return {
    lifter: {
      lifts: perLift((lift) => ({
        expectedMaxKg: read(values, lift, "expectedMaxKg"),
        curve: {
          ratioAtHalf: read(values, lift, "ratioAtHalf"),
          steepness: read(values, lift, "steepness"),
          technicalMissChance: read(values, lift, "technicalMissPercent") / 100,
        },
      })),
      dayForm: {
        spread: values.spreadPercent / 100,
        maxUncertainty: values.maxUncertaintyPercent / 100,
        correlation: values.correlation,
      },
    },
    ...(Number.isNaN(values.goalKg) ? {} : { goalKg: values.goalKg }),
  };
}

const plansOf = (values: FormValues): PerLift<LiftPlanKg> =>
  perLift((lift) => ({
    openerKg: read(values, lift, "openerKg"),
    second: {
      afterMakeKg: read(values, lift, "secondAfterMakeKg"),
      afterMissKg: read(values, lift, "secondAfterMissKg"),
    },
    third: {
      afterMakeKg: read(values, lift, "thirdAfterMakeKg"),
      afterMissKg: read(values, lift, "thirdAfterMissKg"),
    },
  }));

/** The lifter, goal and bomb-out cap for the tradeoff sweep, always at the quick count. */
export function buildTradeoffRequest(values: FormValues): TradeoffRequest {
  return {
    ...lifterAndGoal(values),
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
  return {
    ...lifterAndGoal(values),
    plans: plansOf(values),
    simulations: precise ? PRECISE_SIMULATIONS : QUICK_SIMULATIONS,
    seed: SEED,
  };
}

/** A key that is the same for two tradeoff requests exactly when they ask for the same sweep. */
export const sweepKey = (request: TradeoffRequest) => JSON.stringify(request);

/** A tradeoff sweep and the request it answered. */
export interface Swept {
  sweep: Extract<TradeoffResponse, { kind: "swept" }>;
  request: TradeoffRequest;
}

/**
 * The sweep's lifter, goal and bomb-out cap, with the current plans to start from. Carries
 * the best plans of `swept` when it answered the same sweep, so the search need not sweep
 * again.
 */
export function buildSearchRequest(
  values: FormValues,
  swept?: Swept,
): SearchRequest {
  const request = buildTradeoffRequest(values);
  const sameSweep = swept && sweepKey(swept.request) === sweepKey(request);
  const index = sameSweep ? swept.sweep.bestForSearch : undefined;
  const familyBest =
    index === undefined ? undefined : swept?.sweep.points[index]?.plans;
  return {
    ...request,
    plans: plansOf(values),
    ...(familyBest ? { familyBest } : {}),
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

/** The sweep's problems that only it reports, so they are not listed twice. */
export const sweepOnlyProblems = (problems: Problem[]) =>
  problems.filter((problem) => problem.field === "bombOutCap");
