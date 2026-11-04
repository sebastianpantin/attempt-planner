import type { Evaluation } from "../pkg/attempt_sim_wasm.js";
import * as format from "./format.ts";

/** The headline figures for the compact results bar on narrow screens. */
export function resultsSummary(
  evaluation: Pick<
    Evaluation,
    "expectedTotalKg" | "goalProbability" | "bombOutProbability"
  >,
): string[] {
  return [
    `Average ${format.kg(evaluation.expectedTotalKg)}`,
    ...(evaluation.goalProbability === undefined
      ? []
      : [`Goal ${format.percent(evaluation.goalProbability)}`]),
    `Bomb-out ${format.percent(evaluation.bombOutProbability)}`,
  ];
}
