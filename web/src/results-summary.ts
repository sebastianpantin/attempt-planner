import type { StandardErrors } from "../pkg/attempt_sim_wasm.js";
import { type Headline, headlines } from "./format.ts";

const SUMMARY_LABELS: Record<Headline, string> = {
  total: "Average",
  goal: "Goal",
  bombOut: "Bomb-out",
};

/** The headline figures for the compact results bar. */
export const resultsSummary = (evaluation: StandardErrors) =>
  headlines(evaluation).map(
    ([headline, value]) => `${SUMMARY_LABELS[headline]} ${value}`,
  );
