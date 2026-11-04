import type { Percentiles, StandardErrors } from "../pkg/attempt_sim_wasm.js";
import * as format from "./format.ts";

/** The rarest chance put as a count of meets. */
const RAREST = 500;

/** A count of meets rounded to a friendly figure: 7, 15, 35, 750. */
function friendlyCount(meets: number): number {
  const step = meets < 20 ? 1 : meets < 100 ? 5 : 50;
  return Math.round(meets / step) * step;
}

/** A chance as a count of meets: "7 of 10", "1 in 25", "24 of 25". */
export function chanceInWords(probability: number): string {
  if (probability < 1 / RAREST)
    return `fewer than 1 in ${format.weight(RAREST)}`;
  if (probability > 1 - 1 / RAREST) return "almost all";
  if (probability < 0.1) {
    return `1 in ${format.weight(friendlyCount(1 / probability))}`;
  }
  if (probability > 0.9) {
    const meets = Math.round(1 / (1 - probability));
    return `${meets - 1} of ${meets}`;
  }
  return `${Math.round(probability * 10)} of 10`;
}

/** A simulated total, where 0 kg can only be a bomb-out. */
export const totalKg = (kg: number) => (kg === 0 ? "bomb-out" : format.kg(kg));

/** The headline figures of an evaluation the outcome card reads. */
export type Figures = StandardErrors & { totalPercentilesKg: Percentiles };

export interface Phrase {
  text: string;
  strong?: boolean;
}

const capitalized = (text: string) =>
  text.charAt(0).toUpperCase() + text.slice(1);

/** A count of meets to stress, then what happens in them. */
const countedPhrases = (probability: number, what: string): Phrase[] => [
  { text: capitalized(chanceInWords(probability)), strong: true },
  { text: ` simulated meets ${what}.` },
];

function goalPhrases({ kg, chance: probability }: KnownGoal): Phrase[] {
  const goal = format.kg(kg);
  if (probability === 0)
    return [{ text: `No simulated meet reaches ${goal}.` }];
  if (probability === 1) {
    return [{ text: `Every simulated meet reaches ${goal} or more.` }];
  }
  return countedPhrases(probability, `reach ${goal} or more`);
}

function bombOutPhrases(probability: number): Phrase[] {
  if (probability === 0) return [{ text: "No simulated meet bombs out." }];
  if (probability === 1) return [{ text: "Every simulated meet bombs out." }];
  return countedPhrases(probability, "end in a bomb-out");
}

export interface KnownGoal {
  kg: number;
  chance: number;
}

/** The goal and its chance when both are known, which makes the goal the headline. */
export const knownGoal = (
  figures: StandardErrors,
  goalKg?: number,
): KnownGoal | undefined =>
  figures.goalProbability === undefined || goalKg === undefined
    ? undefined
    : { kg: goalKg, chance: figures.goalProbability };

/** The typical total in words: the median, which bomb-outs do not drag down unless they are most meets. */
function typicalPhrases({ p50 }: Percentiles): Phrase[] {
  if (p50 === 0)
    return [{ text: "Half the simulated meets or more end in a bomb-out." }];
  return [
    { text: "Half the simulated meets total " },
    { text: format.kg(p50), strong: true },
    { text: " or more." },
  ];
}

/** The outcome in plain words, with the counts to stress marked strong. */
export function outcomeSentence(figures: Figures, goalKg?: number): Phrase[] {
  const goal = knownGoal(figures, goalKg);
  const lead = goal
    ? goalPhrases(goal)
    : typicalPhrases(figures.totalPercentilesKg);
  return [
    ...lead,
    { text: " " },
    ...bombOutPhrases(figures.bombOutProbability),
  ];
}

/** The ring: the goal chance with a goal, else the typical total filled by the meets without a bomb-out. */
export function ringFigure(figures: Figures, goalKg?: number) {
  const goal = knownGoal(figures, goalKg);
  if (goal) {
    return {
      value: format.percent(goal.chance),
      label: `reach ${format.kg(goal.kg)}`,
      share: goal.chance,
    };
  }
  return {
    value: totalKg(figures.totalPercentilesKg.p50),
    label: "typical total",
    share: 1 - figures.bombOutProbability,
  };
}

/** The bottom bar's figures, each a value and what it means. */
export function barFigures(
  figures: Figures,
  goalKg?: number,
): [string, string][] {
  const goal = knownGoal(figures, goalKg);
  const lead: [string, string] = goal
    ? [format.percent(goal.chance), `reach ${format.kg(goal.kg)}`]
    : [totalKg(figures.totalPercentilesKg.p50), "typical total"];
  return [lead, [format.percent(figures.bombOutProbability), "bomb-out"]];
}

/** The average total said plainly: bomb-outs count as 0 kg, and the average of the meets with a total. */
export function averageLine(
  expectedTotalKg: number,
  withoutBombOutKg: number | undefined,
): string {
  const posted =
    withoutBombOutKg === undefined
      ? "every meet bombed out"
      : `${format.kg(withoutBombOutKg)} in the meets where you post a total`;
  return `Average total: ${format.kg(expectedTotalKg)} counting a bomb-out as 0 kg; ${posted}.`;
}

/** The likely-totals labels, naming a bomb-out rather than 0 kg, with a note when bomb-outs reach into the range. */
export function rangeLabels({ p10, p50, p90 }: Percentiles) {
  const note =
    p50 === 0
      ? "Half the meets or more end in a bomb-out."
      : p10 === 0
        ? "1 in 10 meets or more ends in a bomb-out."
        : "";
  return {
    low: totalKg(p10),
    median: `median ${p50 === 0 ? "bomb-out" : format.weight(p50)}`,
    high: totalKg(p90),
    note,
  };
}

/**
 * Where the likely totals, the median and the goal sit along the range bar, in percent of
 * its width. The scale takes in the totals posted, so a bomb-out sits at the left edge
 * rather than stretching it to 0 kg.
 */
export function rangeLayout(totals: Percentiles, goalKg?: number) {
  const posted = [totals.p10, totals.p50, totals.p90, goalKg ?? 0].filter(
    (kg) => kg > 0,
  );
  const low = Math.min(...posted);
  const high = Math.max(...posted);
  const margin = high > low ? (high - low) / 4 : 1;
  const start = low - margin;
  const width = high + margin - start;
  const at = (kg: number) =>
    kg > 0 && posted.length > 0 ? ((kg - start) / width) * 100 : 0;
  return {
    fromPercent: at(totals.p10),
    toPercent: at(totals.p90),
    medianPercent: at(totals.p50),
    goalPercent: goalKg === undefined ? undefined : at(goalKg),
  };
}

/** Above this bomb-out chance a plan gets a warning. */
export const HIGH_RISK = 0.1;

/** A warning for a plan that bombs out often, or nothing. */
export const riskWarning = (bombOutProbability: number) =>
  bombOutProbability > HIGH_RISK
    ? `High risk: ${format.percent(bombOutProbability)} of meets end in a bomb-out. Set a lower bomb-out limit in step 1 and press Find my best plan for a safer plan.`
    : "";
