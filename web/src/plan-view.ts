import type { Lift, LiftPlanKg } from "../pkg/attempt_sim_wasm.js";
import { attemptTree } from "./attempt-tree.ts";
import { jump, weight, wholePercent } from "./format.ts";
import {
  type FormValues,
  type InputName,
  inputName,
  isRepeatJump,
  JUMP_FIELDS,
  type LiftField,
} from "./inputs.ts";

const path = (...weights: number[]) => weights.map(weight).join(" → ");

export interface Ladder {
  /** The 2nd and 3rd attempts if every attempt before is made, "–" while unknown. */
  secondKg: string;
  thirdKg: string;
  shareOfMax: string;
  /** The weights if every attempt is made, for a collapsed lift. */
  line: string;
  paths: [string, string][];
}

/** What a lift's attempt cards show, leaving out what a missing weight makes unknown. */
export function ladder(plan: LiftPlanKg, expectedMaxKg: number): Ladder {
  const share = plan.openerKg / expectedMaxKg;
  const shareOfMax =
    Number.isFinite(share) && share > 0
      ? `${wholePercent(share)} of your max`
      : "";
  const tree = attemptTree(plan);
  const made = tree?.afterMake;
  const missed = tree?.afterMiss;
  if (!tree || !made?.afterMake || !made.afterMiss || !missed?.afterMake) {
    return { secondKg: "–", thirdKg: "–", shareOfMax, line: "", paths: [] };
  }
  const opener = tree.weightKg;
  const allMade = path(opener, made.weightKg, made.afterMake.weightKg);
  return {
    secondKg: weight(made.weightKg),
    thirdKg: weight(made.afterMake.weightKg),
    shareOfMax,
    line: allMade,
    paths: [
      ["All made", allMade],
      [
        "Miss the opener",
        path(opener, missed.weightKg, missed.afterMake.weightKg),
      ],
      ["Miss the 2nd", path(opener, made.weightKg, made.afterMiss.weightKg)],
    ],
  };
}

/** What a screen reader says for a plan input's value, as the jumps show signed. */
export function valueText(field: LiftField, kg: number): string {
  if (Number.isNaN(kg)) return "empty";
  if (field === "openerKg") return `${weight(kg)} kg`;
  if (kg === 0) {
    return isRepeatJump(field) ? "repeat the same weight" : "no change";
  }
  return `${weight(Math.abs(kg))} kg ${kg > 0 ? "more" : "less"}`;
}

/** Each jump input of a lift with the signed jump shown over it, none while it is empty. */
export const jumpLabels = (
  values: FormValues,
  lift: Lift,
): [InputName, string | undefined][] =>
  JUMP_FIELDS.map((field) => {
    const name = inputName(lift, field);
    const kg = values[name];
    return [name, Number.isNaN(kg) ? undefined : jump(kg, isRepeatJump(field))];
  });

const steps = (from: number, to: number, by: number) =>
  Array.from(
    { length: Math.round((to - from) / by) + 1 },
    (_, index) => from + index * by,
  );

/** The caps in percent the slider steps through, finest at the low end where most caps are. */
export const BOMB_OUT_CAPS = [
  ...steps(0, 10, 0.5),
  ...steps(11, 20, 1),
  ...steps(22.5, 30, 2.5),
  ...steps(35, 100, 5),
];

const capAt = (position: number) =>
  BOMB_OUT_CAPS[
    Math.min(Math.max(Math.round(position), 0), BOMB_OUT_CAPS.length - 1)
  ]!;

/** The slider position whose cap is nearest `percent`. */
export const sliderPosition = (percent: number) =>
  BOMB_OUT_CAPS.reduce(
    (best, cap, index) =>
      Math.abs(cap - percent) < Math.abs(BOMB_OUT_CAPS[best]! - percent)
        ? index
        : best,
    0,
  );

/** The cap the page asks for: the slider's cap, or none when any risk goes. */
export const bombOutCapPercent = (position: number, anyRisk: boolean) =>
  anyRisk ? Number.NaN : capAt(position);

export const bombOutCapLabel = (position: number, anyRisk: boolean) =>
  anyRisk ? "Any risk" : `${weight(capAt(position))}%`;
