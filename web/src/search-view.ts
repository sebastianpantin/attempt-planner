import type { SearchProgress } from "../pkg/attempt_sim_wasm.js";
import * as format from "./format.ts";

export function describeSearch(progress: SearchProgress): string {
  const figures = [
    ...(progress.goalProbability === undefined
      ? []
      : [
          `${format.percent(progress.goalProbability)} chance of reaching the goal`,
        ]),
    `${format.kg(progress.expectedTotalKg)} average total`,
    `${format.percent(progress.bombOutProbability)} bomb-out chance`,
  ].join(", ");
  const rounds = progress.round === 1 ? "1 round" : `${progress.round} rounds`;
  return progress.done
    ? `Found in ${rounds} and filled in: ${figures}.`
    : `Round ${progress.round}, best so far: ${figures}.`;
}
