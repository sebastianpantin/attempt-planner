import type {
  InputField,
  Lift,
  Problem,
  ProblemCode,
} from "../pkg/attempt_sim_wasm.js";
import { LIFT_NAMES, listOf } from "./format.ts";
import {
  type FormValues,
  type InputName,
  inputName,
  type LiftField,
  LIFTS,
  type MeetField,
} from "./inputs.ts";

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

const outside = (value: number, low: number, high: number) =>
  Number.isNaN(value) || value < low || value > high;

/** Whether an input on its own breaks the rule behind the problem's code. */
function atFault(code: ProblemCode, name: InputName, value: number): boolean {
  if (code === "invalidWeight") return Number.isNaN(value);
  if (code === "lighter") return value < 0;
  if (code === "noIncreaseAfterMake") return name.endsWith("AfterMakeKg");
  if (code === "notLoadable") return value % 2.5 !== 0;
  if (name === "correlation") return outside(value, 0, 1);
  if (name.endsWith("technicalMissPercent")) return outside(value, 0, 99.999);
  if (name.endsWith("ratioAtHalf") || name.endsWith("steepness")) {
    return Number.isNaN(value) || value <= 0;
  }
  return Number.isNaN(value) || value < 0;
}

/**
 * The inputs to mark for a problem the engine found: of the inputs it names, those that
 * break the rule on their own, or all of them when only their mix does.
 */
export function inputsFor(problem: Problem, values: FormValues): InputName[] {
  const { lift, field, code } = problem;
  const names = lift
    ? (PROBLEM_LIFT_FIELDS[field] ?? []).map((f) => inputName(lift, f))
    : (PROBLEM_MEET_FIELDS[field] ?? []);
  const faulty = names.filter((name) => atFault(code, name, values[name]));
  return faulty.length > 0 ? faulty : names;
}

const SUBJECTS: Record<InputField, string> = {
  expectedMax: "max",
  curve: "make rate",
  opener: "opener",
  second: "2nd attempt",
  third: "3rd attempt",
  dayForm: "Day form",
  simulations: "Simulations",
  goal: "Goal total",
  bombOutCap: "Bomb-out limit",
};

const PAGE_MESSAGES: Partial<Record<ProblemCode, string>> = {
  invalidWeight: "enter a weight in kg",
  expectedMaxBelowBar: "enter at least 20 kg, the weight of the bar",
  belowBar: "must be at least 20 kg, the weight of the bar",
  tooHeavy: "must be at most 1,000 kg",
  notLoadable: "use a multiple of 2.5 kg",
  lighter: "a jump after a miss can't be negative; 0 repeats the weight",
  noIncreaseAfterMake: "the jump after a make must be at least 2.5 kg",
  invalidCurve:
    "the midpoint and steepness must be above 0, and the technical-miss chance from 0% to below 100%",
  invalidDayForm:
    "the spread and the uncertainty must be 0% or more and together at most 50%, and the correlation from 0 to 1",
  invalidBombOutCap: "must be from 0 to 100%",
};

/** A problem in the page's words, naming the lift and the field. */
export function describeProblem({ lift, field, code, message }: Problem) {
  const subject = lift
    ? `${LIFT_NAMES[lift].full} ${SUBJECTS[field]}`
    : SUBJECTS[field];
  const advice =
    field === "goal" && code === "invalidWeight"
      ? "enter a weight in kg, or leave it empty for no goal"
      : (PAGE_MESSAGES[code] ?? message);
  return `${subject}: ${advice}.`;
}

/** The list a problem goes in, next to its inputs. */
export function problemListId({ lift, field }: Problem): string {
  if (field === "expectedMax") return `${lift}-max-problems`;
  if (field === "curve") return "curve-problems";
  if (field === "dayForm") return "day-form-problems";
  return `${lift ?? "meet"}-problems`;
}

/** Lifts whose max is still empty, so their results wait for it. */
export const liftsWithoutMax = (values: FormValues): Lift[] =>
  LIFTS.filter((lift) =>
    Number.isNaN(values[inputName(lift, "expectedMaxKg")]),
  );

const WAITING_FIELDS = new Set<InputField>([
  "expectedMax",
  "opener",
  "second",
  "third",
]);

/** The problems to show: none yet for a lift whose max is still empty, as a prompt asks for it instead. */
export function shownProblems(
  problems: Problem[],
  values: FormValues,
): Problem[] {
  const waiting = new Set(liftsWithoutMax(values));
  return problems.filter(
    ({ lift, field }) =>
      !(lift && waiting.has(lift) && WAITING_FIELDS.has(field)),
  );
}

/** Asks for the maxes still empty, or nothing once all are in. */
export function maxesPrompt(values: FormValues): string {
  const missing = liftsWithoutMax(values);
  if (missing.length === 0) return "";
  const names = missing.map((lift) => LIFT_NAMES[lift].full.toLowerCase());
  return `Enter your ${listOf(names)} ${missing.length === 1 ? "max" : "maxes"} to see results.`;
}
