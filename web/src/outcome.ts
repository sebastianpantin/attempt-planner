import type { Percentiles, StandardErrors } from "../pkg/attempt_sim_wasm.js";
import { percent } from "./format.ts";

const upToOneDecimal = new Intl.NumberFormat("en", {
  maximumFractionDigits: 1,
});

/** A weight without a needless ".0": "182.5", "195". */
export const weight = (kg: number) => upToOneDecimal.format(kg);
export const shortKg = (kg: number) => `${weight(kg)} kg`;

/** The rarest chance put as a count of meets. */
const RAREST = 1000;

/** A chance as a count of meets: "7 of 10", "1 of 25", "24 of 25". */
export function chanceInWords(probability: number): string {
  if (probability < 1 / RAREST) return `fewer than 1 of ${weight(RAREST)}`;
  if (probability > 1 - 1 / RAREST) return "almost all";
  if (probability < 0.1) return `1 of ${Math.round(1 / probability)}`;
  if (probability > 0.9) {
    const meets = Math.round(1 / (1 - probability));
    return `${meets - 1} of ${meets}`;
  }
  return `${Math.round(probability * 10)} of 10`;
}

export interface Phrase {
  text: string;
  strong?: boolean;
}

function goalPhrases(probability: number, goalKg: number): Phrase[] {
  const goal = shortKg(goalKg);
  if (probability === 0)
    return [{ text: `No simulated meet reaches ${goal}.` }];
  if (probability === 1) {
    return [{ text: `Every simulated meet reaches ${goal} or more.` }];
  }
  return [
    { text: "In " },
    { text: chanceInWords(probability), strong: true },
    { text: ` simulated meets you total ${goal} or more.` },
  ];
}

function bombOutPhrases(probability: number): Phrase[] {
  if (probability === 0) return [{ text: "No simulated meet bombs out." }];
  if (probability === 1) return [{ text: "Every simulated meet bombs out." }];
  return [
    { text: "You bomb out in " },
    { text: chanceInWords(probability), strong: true },
    { text: "." },
  ];
}

/** The outcome in plain words, with the counts to stress marked strong. */
export function outcomeSentence(
  figures: StandardErrors,
  goalKg?: number,
): Phrase[] {
  const lead =
    figures.goalProbability === undefined || goalKg === undefined
      ? [
          { text: "On average you total " },
          { text: shortKg(figures.expectedTotalKg), strong: true },
          { text: ", counting a bomb-out as 0." },
        ]
      : goalPhrases(figures.goalProbability, goalKg);
  return [
    ...lead,
    { text: " " },
    ...bombOutPhrases(figures.bombOutProbability),
  ];
}

/** The ring: the goal chance with a goal, else the average total filled by the meets without a bomb-out. */
export function ringFigure(figures: StandardErrors, goalKg?: number) {
  if (figures.goalProbability !== undefined && goalKg !== undefined) {
    return {
      value: percent(figures.goalProbability),
      label: `reach ${shortKg(goalKg)}`,
      share: figures.goalProbability,
    };
  }
  return {
    value: shortKg(figures.expectedTotalKg),
    label: "average total",
    share: 1 - figures.bombOutProbability,
  };
}

/** The bottom bar's figures, each a value and what it means. */
export function barFigures(
  figures: StandardErrors,
  goalKg?: number,
): [string, string][] {
  const lead: [string, string] =
    figures.goalProbability !== undefined && goalKg !== undefined
      ? [percent(figures.goalProbability), `reach ${shortKg(goalKg)}`]
      : [shortKg(figures.expectedTotalKg), "average"];
  return [lead, [percent(figures.bombOutProbability), "bomb-out"]];
}

/** Where the likely totals, the median and the goal sit along the range bar, in percent of its width. */
export function rangeLayout(totals: Percentiles, goalKg?: number) {
  const low = Math.min(totals.p10, goalKg ?? Infinity);
  const high = Math.max(totals.p90, goalKg ?? -Infinity);
  const margin = high > low ? (high - low) / 4 : 1;
  const start = low - margin;
  const width = high + margin - start;
  const at = (kg: number) => ((kg - start) / width) * 100;
  return {
    fromPercent: at(totals.p10),
    toPercent: at(totals.p90),
    medianPercent: at(totals.p50),
    goalPercent: goalKg === undefined ? undefined : at(goalKg),
  };
}
