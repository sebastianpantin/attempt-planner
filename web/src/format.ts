import type { StandardErrors } from "../pkg/attempt_sim_wasm.js";
import { PRECISE_SIMULATIONS } from "./inputs.ts";

const oneDecimal = new Intl.NumberFormat("en", {
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
});
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
const conjunction = new Intl.ListFormat("en", { type: "conjunction" });

/** The smallest chance one decimal can show. */
const SMALLEST_SHOWN = 0.0005;

export const kg = (value: number) => `${oneDecimal.format(value)} kg`;

/** One decimal, with a chance too small to show reading "under 0.1%" rather than 0.0%. */
export const percent = (probability: number) =>
  probability > 0 && probability < SMALLEST_SHOWN
    ? "under 0.1%"
    : onePercentDecimal.format(probability);

/** Up to two decimals, for axis ticks like 0.5% and 1%. */
export const finePercent = (probability: number) =>
  upToTwoPercentDecimals.format(probability);

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
