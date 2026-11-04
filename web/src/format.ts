import type { Lift, StandardErrors } from "../pkg/attempt_sim_wasm.js";
import { PRECISE_SIMULATIONS } from "./inputs.ts";

const percentFormat = (
  minimumFractionDigits: number,
  maximumFractionDigits: number,
) =>
  new Intl.NumberFormat("en", {
    style: "percent",
    minimumFractionDigits,
    maximumFractionDigits,
  });
const onePercentDecimal = percentFormat(1, 1);
const upToTwoPercentDecimals = percentFormat(0, 2);
const noPercentDecimals = percentFormat(0, 0);
const upToOneDecimal = new Intl.NumberFormat("en", {
  maximumFractionDigits: 1,
});
const conjunction = new Intl.ListFormat("en", { type: "conjunction" });

/** The smallest chance one decimal can show. */
const SMALLEST_SHOWN = 0.0005;

/** Each lift's full name, and a short one for tight labels such as tabs. */
export const LIFT_NAMES: Record<Lift, { full: string; short: string }> = {
  squat: { full: "Squat", short: "Squat" },
  bench: { full: "Bench press", short: "Bench" },
  deadlift: { full: "Deadlift", short: "Deadlift" },
};

/** A weight without a needless ".0": "182.5", "195". */
export const weight = (kg: number) => upToOneDecimal.format(kg);

/** A weight in kg with at most one decimal: "195 kg", "202.5 kg". */
export const kg = (value: number) => `${weight(value)} kg`;

/** A jump with its sign, "+12.5"; with `zeroRepeats`, a jump of 0 reads "repeat". */
export function jump(kg: number, zeroRepeats: boolean): string {
  if (kg === 0 && zeroRepeats) return "repeat";
  return kg > 0 ? `+${weight(kg)}` : weight(kg);
}

export const wholePercent = (probability: number) =>
  noPercentDecimals.format(probability);

/** One decimal, with a chance too small to show reading "under 0.1%" rather than 0.0%. */
export const percent = (probability: number) =>
  probability > 0 && probability < SMALLEST_SHOWN
    ? "under 0.1%"
    : onePercentDecimal.format(probability);

/** Up to two decimals, for axis ticks like 0.5% and 1%. */
export const finePercent = (probability: number) =>
  upToTwoPercentDecimals.format(probability);

/** Round chances for scales and axes, from none to certain. */
export const ROUND_CHANCES = [
  0, 0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.15, 0.2, 0.3, 0.5, 1,
];

/** The first round chance at or above `chance`, ignoring rounding error. */
export const roundChanceUp = (chance: number) =>
  ROUND_CHANCES.find((round) => round >= chance - 1e-9) ?? 1;

/** "a, b and c". */
export const listOf = (items: string[]) => conjunction.format(items);

export type Headline = "total" | "goal" | "bombOut";

/** The headline figures in order, the goal chance only when there is a goal. */
export function headlines(figures: StandardErrors): [Headline, string][] {
  return [
    ["total", kg(figures.expectedTotalKg)],
    ...(figures.goalProbability === undefined
      ? []
      : [["goal", percent(figures.goalProbability)] as [Headline, string]]),
    ["bombOut", percent(figures.bombOutProbability)],
  ];
}

const ERROR_LABELS: Record<Headline, string> = {
  total: "on the total",
  goal: "on the goal chance",
  bombOut: "on the bomb-out chance",
};

/** One quiet line with the Monte Carlo standard errors of the headline figures. */
export function simulationError(errors: StandardErrors, simulations: number) {
  const atLeastShown = (error: number) => Math.max(error, SMALLEST_SHOWN);
  const parts = headlines({
    expectedTotalKg: errors.expectedTotalKg,
    bombOutProbability: atLeastShown(errors.bombOutProbability),
    ...(errors.goalProbability === undefined
      ? {}
      : { goalProbability: atLeastShown(errors.goalProbability) }),
  }).map(([headline, value]) => `±${value} ${ERROR_LABELS[headline]}`);
  const narrow =
    simulations < PRECISE_SIMULATIONS ? " Tick Precise to narrow it." : "";
  return `Simulation error: about ${listOf(parts)}.${narrow}`;
}
