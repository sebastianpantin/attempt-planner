import type {
  Lift,
  LiftEstimate,
  MaxEstimate,
  MaxEstimateInput,
  PastAttempt,
  PastMeet,
} from "../pkg/attempt_sim_wasm.js";
import { kg, LIFT_NAMES, percent } from "./format.ts";
import {
  type FormValues,
  type InputName,
  inputName,
  LIFTS,
  perLift,
} from "./inputs.ts";

const MS_PER_DAY = 24 * 60 * 60 * 1000;
const ATTEMPT_COLUMNS: Record<Lift, string> = {
  squat: "Squat",
  bench: "Bench",
  deadlift: "Deadlift",
};

export interface ParsedMeets {
  meets: PastMeet[];
  /** Lines or rows that were not a raw meet with at least one attempt. */
  skipped: number;
}

/** Days since 1970-01-01 for a `YYYY-MM-DD` date. */
export function dayNumber(date: string): number | undefined {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date.trim());
  if (!match) return undefined;
  const [year, month, day] = [match[1], match[2], match[3]].map(Number) as [
    number,
    number,
    number,
  ];
  const time = Date.UTC(year, month - 1, day);
  const valid = new Date(time).getUTCDate() === day && month <= 12;
  return valid ? time / MS_PER_DAY : undefined;
}

export const today = (now = new Date()) =>
  Math.floor(now.getTime() / MS_PER_DAY);

/** An attempt as OpenPowerlifting records it: missed negative, not taken 0 or blank. */
function attempt(text: string): PastAttempt | undefined | "invalid" {
  const kg = Number(text);
  if (text.trim() === "" || kg === 0) return undefined;
  return Number.isFinite(kg) ? { kg: Math.abs(kg), made: kg > 0 } : "invalid";
}

function meet(
  date: string,
  attemptsOf: (lift: Lift) => string[],
): PastMeet | undefined {
  const day = dayNumber(date);
  if (day === undefined) return undefined;
  const attempts = {} as PastMeet["attempts"];
  for (const lift of LIFTS) {
    const parsed = attemptsOf(lift).map(attempt);
    if (parsed.includes("invalid")) return undefined;
    attempts[lift] = parsed.filter(
      (a): a is PastAttempt => !!a && a !== "invalid",
    );
  }
  const any = LIFTS.some((lift) => attempts[lift].length > 0);
  return any ? { day, attempts } : undefined;
}

function csvRows(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    if (quoted) {
      if (char === '"' && text[i + 1] === '"') field += text[++i];
      else if (char === '"') quoted = false;
      else field += char;
    } else if (char === '"') quoted = true;
    else if (char === ",") {
      row.push(field);
      field = "";
    } else if (char === "\n" || char === "\r") {
      if (char === "\r" && text[i + 1] === "\n") i++;
      row.push(field);
      if (row.some((cell) => cell.trim() !== "")) rows.push(row);
      row = [];
      field = "";
    } else field += char;
  }
  row.push(field);
  if (row.some((cell) => cell.trim() !== "")) rows.push(row);
  return rows;
}

function fromCsv(text: string): ParsedMeets {
  const [header = [], ...rows] = csvRows(text);
  const column = (name: string) => header.indexOf(name);
  const date = column("Date");
  const equipment = column("Equipment");
  const meets: PastMeet[] = [];
  let skipped = 0;
  for (const row of rows) {
    const raw = equipment < 0 || row[equipment] === "Raw";
    const parsed =
      raw &&
      meet(row[date] ?? "", (lift) =>
        [1, 2, 3].map(
          (n) => row[column(`${ATTEMPT_COLUMNS[lift]}${n}Kg`)] ?? "",
        ),
      );
    if (parsed) meets.push(parsed);
    else skipped++;
  }
  return { meets, skipped };
}

function fromLines(text: string): ParsedMeets {
  const meets: PastMeet[] = [];
  let skipped = 0;
  for (const line of text.split(/\r?\n/)) {
    if (line.trim() === "") continue;
    const [date = "", ...numbers] = line.trim().split(/[\s,;]+/);
    const parsed =
      numbers.length === 9 &&
      meet(date, (lift) => {
        const start = LIFTS.indexOf(lift) * 3;
        return numbers.slice(start, start + 3);
      });
    if (parsed) meets.push(parsed);
    else skipped++;
  }
  return { meets, skipped };
}

/**
 * Reads past meets from an OpenPowerlifting lifter CSV, or from lines of a date and nine
 * attempts: squat, bench and deadlift, a miss negative and an attempt not taken 0.
 */
export function parsePastMeets(text: string): ParsedMeets {
  const firstLine = text.trimStart().split(/\r?\n/, 1)[0] ?? "";
  return firstLine.includes("Squat1Kg") ? fromCsv(text) : fromLines(text);
}

export function buildEstimateRequest(
  values: FormValues,
  meets: PastMeet[],
  onDay: number,
  typicalOpener: MaxEstimateInput["typicalOpener"],
): MaxEstimateInput {
  return {
    curves: perLift((lift) => ({
      ratioAtHalf: values[inputName(lift, "ratioAtHalf")],
      steepness: values[inputName(lift, "steepness")],
      technicalMissChance:
        values[inputName(lift, "technicalMissPercent")] / 100,
    })),
    typicalOpener,
    dayFormSpread: values.spreadPercent / 100,
    meets,
    onDay,
  };
}

/** Expected maxes go to the nearest half kilogram, percentages to a tenth. */
const toHalf = (value: number) => Math.round(value * 2) / 2;
const toTenth = (value: number) => Math.round(value * 10) / 10;

/**
 * The inputs an estimate fills in: each estimated lift's expected max, and the
 * uncertainty of the expected max as the average over the estimated lifts.
 */
export function estimateValues(estimate: MaxEstimate): [InputName, number][] {
  const found = LIFTS.flatMap((lift) => {
    const liftEstimate = estimate.lifts[lift];
    return liftEstimate ? [{ lift, ...liftEstimate }] : [];
  });
  const values: [InputName, number][] = found.map(({ lift, expectedMaxKg }) => [
    inputName(lift, "expectedMaxKg"),
    toHalf(expectedMaxKg),
  ]);
  if (found.length > 0) {
    const mean =
      found.reduce((sum, { uncertainty }) => sum + uncertainty, 0) /
      found.length;
    values.push(["maxUncertaintyPercent", Math.max(0.5, toTenth(mean * 100))]);
  }
  return values;
}

/** Each lift's heaviest made attempt in the meets, `undefined` with none made. */
export function bestMade(meets: PastMeet[]): Record<Lift, number | undefined> {
  return perLift((lift) => {
    const made = meets.flatMap((meet) =>
      meet.attempts[lift].filter((a) => a.made).map((a) => a.kg),
    );
    return made.length > 0 ? Math.max(...made) : undefined;
  });
}

/** A lift's estimate next to its best made lift, saying when the estimate is above it. */
export function estimateLine(
  lift: Lift,
  { expectedMaxKg, uncertainty, meets }: LiftEstimate,
  bestKg: number | undefined,
): string {
  const name = LIFT_NAMES[lift].full;
  const estimate = `${name}: ${kg(toHalf(expectedMaxKg))} ± ${percent(uncertainty)}, from ${meets} ${meets === 1 ? "meet" : "meets"}.`;
  if (bestKg === undefined) {
    return `${estimate} No ${name.toLowerCase()} made in these meets.`;
  }
  const above = toHalf(expectedMaxKg) - bestKg;
  return above > 0
    ? `${estimate} Best made: ${kg(bestKg)}, so the estimate is ${kg(above)} above anything you made.`
    : `${estimate} Best made: ${kg(bestKg)}.`;
}
