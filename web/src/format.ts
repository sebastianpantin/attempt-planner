import type { StandardErrors } from "../pkg/attempt_sim_wasm.js";

const oneDecimal = new Intl.NumberFormat("en", { maximumFractionDigits: 1 });
const percentFormat = (digits: number) =>
  new Intl.NumberFormat("en", {
    style: "percent",
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
const onePercentDecimal = percentFormat(1);
const twoPercentDecimals = percentFormat(2);

export const kg = (value: number) => `${oneDecimal.format(value)} kg`;

/** One decimal, or two under 1% so a small chance never reads as 0.0%. Exactly 0 is 0%. */
export const percent = (probability: number) =>
  probability === 0
    ? "0%"
    : (probability < 0.01 ? twoPercentDecimals : onePercentDecimal).format(
        probability,
      );

const upToTwoPercentDecimals = new Intl.NumberFormat("en", {
  style: "percent",
  maximumFractionDigits: 2,
});

/** Up to two decimals, for axis ticks like 0.5% and 1%. */
export const finePercent = (probability: number) =>
  upToTwoPercentDecimals.format(probability);

/** One quiet line with the Monte Carlo standard errors of the headline figures. */
export function simulationError(errors: StandardErrors, canNarrow: boolean) {
  const parts = [
    `±${kg(errors.expectedTotalKg)} on the total`,
    ...(errors.goalProbability === undefined
      ? []
      : [`±${percent(errors.goalProbability)} on the goal chance`]),
    `±${percent(errors.bombOutProbability)} on the bomb-out chance`,
  ];
  const listed = `${parts.slice(0, -1).join(", ")} and ${parts.at(-1)}`;
  return `Simulation error: about ${listed}.${canNarrow ? " Tick Precise to narrow it." : ""}`;
}
