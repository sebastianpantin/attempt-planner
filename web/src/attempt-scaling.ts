import type { Lift, LiftPlanKg, PerLift } from "../pkg/attempt_sim_wasm.js";
import * as format from "./format.ts";
import {
  type FormValues,
  type InputName,
  inputName,
  isRepeatJump,
  JUMP_FIELDS,
  type LiftField,
  LIFTS,
} from "./inputs.ts";

const BAR_KG = 20;
const PLATE_STEP_KG = 2.5;

/** The opener and the four jumps of a lift's plan. */
export const PLAN_FIELDS = [
  "openerKg",
  ...JUMP_FIELDS,
] as const satisfies LiftField[];
export type PlanField = (typeof PLAN_FIELDS)[number];

/** A plan as shares of the expected max, so it can follow the max as it changes. */
export type PlanShape = Record<PlanField, number>;

export const isPlanField = (field: string): field is PlanField =>
  (PLAN_FIELDS as readonly string[]).includes(field);

/** A max the engine accepts: a weight of at least the bar. */
export const isUsableMax = (kg: number) => Number.isFinite(kg) && kg >= BAR_KG;

/** A plan for someone new to it: the typical opener, then about 5½% and 4% more, repeating after a miss. */
export const defaultShape = (typicalOpener: number): PlanShape => ({
  openerKg: typicalOpener,
  secondAfterMakeKg: 0.055,
  secondAfterMissKg: 0,
  thirdAfterMakeKg: 0.04,
  thirdAfterMissKg: 0,
});

export const defaultShapes = (
  typicalOpener: PerLift<number>,
): Record<Lift, PlanShape> =>
  Object.fromEntries(
    LIFTS.map((lift) => [lift, defaultShape(typicalOpener[lift])]),
  ) as Record<Lift, PlanShape>;

const sameShape = (a: PlanShape, b: PlanShape) =>
  PLAN_FIELDS.every((field) => a[field] === b[field]);

/** The lifts whose shape is still the default for openers `from`, with the default for `to`. */
export function regroupedShapes(
  shapes: Record<Lift, PlanShape>,
  from: PerLift<number>,
  to: PerLift<number>,
): [Lift, PlanShape][] {
  const before = defaultShapes(from);
  const after = defaultShapes(to);
  return LIFTS.filter((lift) => sameShape(shapes[lift], before[lift])).map(
    (lift) => [lift, after[lift]],
  );
}

/** The lift's plan as shares of its max, or `undefined` while the max or a weight is missing. */
export function shapeOf(values: FormValues, lift: Lift): PlanShape | undefined {
  const maxKg = values[inputName(lift, "expectedMaxKg")];
  if (!isUsableMax(maxKg)) return undefined;
  const entries = PLAN_FIELDS.map((field) => [
    field,
    values[inputName(lift, field)] / maxKg,
  ]);
  return entries.every(([, share]) => Number.isFinite(share))
    ? (Object.fromEntries(entries) as PlanShape)
    : undefined;
}

const toPlate = (kg: number) => Math.round(kg / PLATE_STEP_KG) * PLATE_STEP_KG;

/**
 * The plan inputs for `shape` at a max of `maxKg`, each on a 2.5 kg step and valid: the
 * opener at least the bar, a jump after a make at least one step, and one after a miss at
 * least 0.
 */
export function scaledPlan(
  shape: PlanShape,
  lift: Lift,
  maxKg: number,
): [InputName, number][] {
  const least = (field: PlanField) =>
    field === "openerKg" ? BAR_KG : isRepeatJump(field) ? 0 : PLATE_STEP_KG;
  return PLAN_FIELDS.map((field) => [
    inputName(lift, field),
    Math.max(least(field), toPlate(shape[field] * maxKg)),
  ]);
}

type AttemptName = "Opener" | "2nd attempt" | "3rd attempt";

/** Each attempt whose heaviest planned weight is above the expected max, with that weight. */
export function attemptsAboveMax(
  { openerKg, second, third }: LiftPlanKg,
  maxKg: number,
): { attempt: AttemptName; kg: number }[] {
  const secondKg = openerKg + Math.max(second.afterMakeKg, second.afterMissKg);
  const heaviest: { attempt: AttemptName; kg: number }[] = [
    { attempt: "Opener", kg: openerKg },
    { attempt: "2nd attempt", kg: secondKg },
    {
      attempt: "3rd attempt",
      kg: secondKg + Math.max(third.afterMakeKg, third.afterMissKg),
    },
  ];
  if (!isUsableMax(maxKg) || !heaviest.every(({ kg }) => Number.isFinite(kg))) {
    return [];
  }
  return heaviest.filter(({ kg }) => kg > maxKg);
}

/** Warnings for attempts above the expected max; an opener above it is missed more often than made. */
export function aboveMaxWarnings(plan: LiftPlanKg, maxKg: number): string[] {
  return attemptsAboveMax(plan, maxKg).map(({ attempt, kg }) =>
    attempt === "Opener"
      ? `Opener ${format.kg(kg)} is above your ${format.kg(maxKg)} max, so you'd miss it more often than make it. Lower it, or press Find my best plan.`
      : `${attempt} ${format.kg(kg)} is above your ${format.kg(maxKg)} max, so it's made less than half the time.`,
  );
}
