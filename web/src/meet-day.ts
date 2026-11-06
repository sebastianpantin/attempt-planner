import type {
  Lift,
  LiftPlanKg,
  NextAttempt,
  PerLift,
  ReplanRequest,
  ReplanResponse,
  SearchRequest,
  TakenAttempt,
} from "../pkg/attempt_sim_wasm.js";
import { BAR_KG, PLATE_STEP_KG } from "./attempt-scaling.ts";
import * as format from "./format.ts";
import { LIFT_NAMES } from "./format.ts";
import { LIFTS, perLift } from "./inputs.ts";

export const ATTEMPTS_PER_LIFT = 3;
export const ATTEMPTS = LIFTS.length * ATTEMPTS_PER_LIFT;
const HEAVIEST_KG = 1000;

/** The plan, lifter and goal the meet started from. Later edits to the plan leave it alone. */
export type MeetSetup = Omit<ReplanRequest, "taken">;

export interface Meet {
  setup: MeetSetup;
  /** Every attempt taken so far, in meet order. */
  taken: TakenAttempt[];
}

export const STORAGE_KEY = "attempt-planner:meet-day";

/** A meet with nothing taken yet, from a search request for the plan as it stands. */
export function startMeet({ familyBest: _, ...setup }: SearchRequest): Meet {
  return { setup, taken: [] };
}

/** The goal, bomb-out limit and openers the meet starts from. */
export function describeSetup({
  goalKg,
  bombOutCap,
  plans,
}: MeetSetup): string {
  const goal =
    goalKg === undefined
      ? "No goal, so the highest average total"
      : `Goal ${format.kg(goalKg)}`;
  const cap =
    bombOutCap === undefined
      ? "any bomb-out risk"
      : `bomb-out risk up to ${format.finePercent(bombOutCap)}`;
  const openers = LIFTS.map(
    (lift) =>
      `${LIFT_NAMES[lift].short.toLowerCase()} ${format.weight(plans[lift].openerKg)}`,
  );
  return `${goal}, ${cap}. Openers: ${format.listOf(openers)} kg.`;
}

/**
 * Whether every number in `value` is finite. An empty planner input reads as `NaN`, which
 * a saved meet would store as `null` and the engine could no longer read after a reload.
 */
export function isComplete(value: unknown): boolean {
  if (typeof value === "number") return Number.isFinite(value);
  if (typeof value !== "object" || value === null) return true;
  return Object.values(value).every(isComplete);
}

export const replanRequest = ({ setup, taken }: Meet): ReplanRequest => ({
  ...setup,
  taken,
});

export interface Slot {
  lift: Lift;
  /** From 1. */
  attempt: number;
}

/** The lift and attempt at `index` in meet order, `undefined` past the last attempt. */
export function slotOf(index: number): Slot | undefined {
  const lift = LIFTS[Math.floor(index / ATTEMPTS_PER_LIFT)];
  return lift && { lift, attempt: (index % ATTEMPTS_PER_LIFT) + 1 };
}

export const slotName = ({ lift, attempt }: Slot) =>
  `${LIFT_NAMES[lift].full} attempt ${attempt}`;

/** Each lift's attempts so far. */
export const byLift = (taken: TakenAttempt[]): PerLift<TakenAttempt[]> =>
  perLift((lift) => {
    const from = LIFTS.indexOf(lift) * ATTEMPTS_PER_LIFT;
    return taken.slice(from, from + ATTEMPTS_PER_LIFT);
  });

/**
 * The lightest load the next attempt may take: the bar for a lift's first attempt, then
 * no lighter than the attempt before, and heavier after a make.
 */
export function lightestNextKg(taken: TakenAttempt[]): number {
  const before = taken.at(-1);
  if (!before || taken.length % ATTEMPTS_PER_LIFT === 0) return BAR_KG;
  return before.weightKg + (before.made ? PLATE_STEP_KG : 0);
}

/**
 * Why `kg` cannot be the next attempt's load, or `undefined` when it can. The engine
 * checks the same rules, but a load is checked here before it is saved, so a slip on the
 * stepper never becomes part of the meet.
 */
export function loadProblem(
  kg: number,
  taken: TakenAttempt[],
): string | undefined {
  if (!Number.isFinite(kg)) return "Enter the load in kg.";
  if (Math.round(kg * 1000) % (PLATE_STEP_KG * 1000) !== 0) {
    return `Loads go up in ${PLATE_STEP_KG} kg steps.`;
  }
  if (kg > HEAVIEST_KG) return `At most ${format.kg(HEAVIEST_KG)}.`;
  const lightest = lightestNextKg(taken);
  if (kg >= lightest) return undefined;
  if (lightest === BAR_KG) return `At least the ${format.kg(BAR_KG)} bar.`;
  const before = taken.at(-1)!;
  return before.made
    ? `At least ${format.kg(lightest)}: heavier than the attempt you made.`
    : `At least ${format.kg(lightest)}: no lighter than the attempt before.`;
}

export const take = (meet: Meet, attempt: TakenAttempt): Meet => ({
  ...meet,
  taken: [...meet.taken, attempt],
});

export const undo = (meet: Meet): Meet => ({
  ...meet,
  taken: meet.taken.slice(0, -1),
});

/** One attempt as a lifter reads it back: "Squat attempt 2: 190 kg, made". */
export const describeTaken = (index: number, attempt: TakenAttempt) =>
  `${slotName(slotOf(index)!)}: ${format.kg(attempt.weightKg)}, ${attempt.made ? "made" : "missed"}`;

/** The attempts after the next one, made or missed, in the replanned plans. */
export function afterNext(
  plans: PerLift<LiftPlanKg>,
  next: NextAttempt,
): string {
  const { lift, attempt, weightKg } = next;
  const nextLift = LIFTS[LIFTS.indexOf(lift) + 1];
  if (attempt === ATTEMPTS_PER_LIFT) {
    return nextLift
      ? `Then the ${LIFT_NAMES[nextLift].full.toLowerCase()} opens at ${format.kg(plans[nextLift].openerKg)}.`
      : "Your last attempt.";
  }
  const jumps = attempt === 1 ? plans[lift].second : plans[lift].third;
  const made = format.kg(weightKg + jumps.afterMakeKg);
  const missed = format.kg(weightKg + jumps.afterMissKg);
  return `If made, then ${made}. If missed, then ${missed}.`;
}

/** Lifts with all three attempts missed. */
export const bombedOut = (taken: TakenAttempt[]): Lift[] => {
  const lifts = byLift(taken);
  return LIFTS.filter(
    (lift) =>
      lifts[lift].length === ATTEMPTS_PER_LIFT &&
      lifts[lift].every((a) => !a.made),
  );
};

/** Where the figures come from, in a lifter's words. */
export function basis(
  replanned: Extract<ReplanResponse, { kind: "replanned" }>,
  taken: number,
): string {
  const meets = new Intl.NumberFormat("en").format(replanned.meets);
  if (taken === 0) return `From ${meets} simulated meets.`;
  return replanned.redrawn
    ? `Few simulated meets went like yours, so ${meets} were drawn to match it.`
    : `From the ${meets} simulated meets that went like yours so far.`;
}

const isTaken = (value: unknown): value is TakenAttempt =>
  typeof value === "object" &&
  value !== null &&
  typeof (value as TakenAttempt).weightKg === "number" &&
  typeof (value as TakenAttempt).made === "boolean";

/** The meet saved in `storage`, or `undefined` when there is none or it cannot be read. */
export function loadMeet(storage: Pick<Storage, "getItem">): Meet | undefined {
  try {
    const saved: unknown = JSON.parse(storage.getItem(STORAGE_KEY) ?? "null");
    if (typeof saved !== "object" || saved === null) return undefined;
    const { setup, taken } = saved as Partial<Meet>;
    const readable =
      typeof setup === "object" &&
      setup !== null &&
      Array.isArray(taken) &&
      taken.length <= ATTEMPTS &&
      taken.every(isTaken);
    return readable ? { setup, taken } : undefined;
  } catch {
    return undefined;
  }
}

/** Saves `meet`, or forgets the saved one when `undefined`. */
export function saveMeet(
  storage: Pick<Storage, "setItem" | "removeItem">,
  meet: Meet | undefined,
) {
  if (meet) storage.setItem(STORAGE_KEY, JSON.stringify(meet));
  else storage.removeItem(STORAGE_KEY);
}
