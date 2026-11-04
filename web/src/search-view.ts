import type { SearchProgress } from "../pkg/attempt_sim_wasm.js";
import * as format from "./format.ts";

/** The search's progress in words; `bombOutCap` is the highest acceptable bomb-out chance. */
export function describeSearch(
  progress: SearchProgress,
  bombOutCap?: number,
): string {
  const figures = [
    ...(progress.goalProbability === undefined
      ? []
      : [
          `${format.percent(progress.goalProbability)} chance of reaching the goal`,
        ]),
    `${format.kg(progress.expectedTotalKg)} average total`,
    `${format.percent(progress.bombOutProbability)} bomb-out chance`,
  ].join(", ");
  if (!progress.done)
    return `Round ${progress.round}, best so far: ${figures}.`;
  if (bombOutCap !== undefined && progress.bombOutProbability > bombOutCap) {
    return `No plan keeps the bomb-out chance within ${format.finePercent(bombOutCap)}. This is the safest plan found: ${figures}.`;
  }
  const rounds = progress.round === 1 ? "1 round" : `${progress.round} rounds`;
  return `Found in ${rounds} and filled in: ${figures}.`;
}
