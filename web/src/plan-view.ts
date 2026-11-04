import type { LiftPlanKg, Problem } from "../pkg/attempt_sim_wasm.js";
import { attemptTree } from "./attempt-tree.ts";
import { weight } from "./outcome.ts";

const wholePercent = new Intl.NumberFormat("en", {
  style: "percent",
  maximumFractionDigits: 0,
});

const path = (...weights: number[]) => weights.map(weight).join(" → ");

export interface Ladder {
  /** The 2nd and 3rd attempts if every attempt before is made. */
  secondKg?: number;
  thirdKg?: number;
  shareOfMax?: string;
  /** The weights if every attempt is made, for a collapsed lift. */
  line?: string;
  paths: [string, string][];
}

/** What a lift's attempt cards show, leaving out what a missing weight makes unknown. */
export function ladder(plan: LiftPlanKg, expectedMaxKg: number): Ladder {
  const share = plan.openerKg / expectedMaxKg;
  const shareOfMax =
    Number.isFinite(share) && share > 0
      ? `${wholePercent.format(share)} of your max`
      : undefined;
  const tree = attemptTree(plan);
  const made = tree?.afterMake;
  const missed = tree?.afterMiss;
  if (!tree || !made?.afterMake || !made.afterMiss || !missed?.afterMake) {
    return { ...(shareOfMax && { shareOfMax }), paths: [] };
  }
  const opener = tree.weightKg;
  const line = path(opener, made.weightKg, made.afterMake.weightKg);
  return {
    secondKg: made.weightKg,
    thirdKg: made.afterMake.weightKg,
    ...(shareOfMax && { shareOfMax }),
    line,
    paths: [
      ["All made", line],
      [
        "Miss the opener",
        path(opener, missed.weightKg, missed.afterMake.weightKg),
      ],
      ["Miss the 2nd", path(opener, made.weightKg, made.afterMiss.weightKg)],
    ],
  };
}

/** The cap the page asks for: the slider's value, or none with no limit. */
export const bombOutCapPercent = (slider: number, noLimit: boolean) =>
  noLimit ? Number.NaN : slider;

export const bombOutCapLabel = (slider: number, noLimit: boolean) =>
  noLimit ? "No limit" : `${weight(slider)}%`;

/** The list a problem goes in: the maxes, its lift's attempts, or the meet. */
export const problemListId = ({ lift, field }: Problem) =>
  field === "expectedMax" ? "maxes-problems" : `${lift ?? "meet"}-problems`;
