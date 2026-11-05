import { describe, expect, it } from "vitest";
import { CALIBRATIONS, EXAMPLE } from "./example.ts";
import {
  bestMade,
  buildEstimateRequest,
  estimateLine,
  dayNumber,
  estimateValues,
  parsePastMeets,
  today,
} from "./past-meets.ts";

const HEADER =
  "Name,Sex,Event,Equipment,Squat1Kg,Squat2Kg,Squat3Kg,Squat4Kg,Bench1Kg,Bench2Kg,Bench3Kg,Bench4Kg,Deadlift1Kg,Deadlift2Kg,Deadlift3Kg,Deadlift4Kg,TotalKg,Federation,Date,MeetName";

describe("parsePastMeets", () => {
  it("reads an OpenPowerlifting lifter CSV, raw meets only", () => {
    const csv = [
      HEADER,
      'Kim,F,SBD,Raw,180,190,-197.5,,120,125,-130,,220,235,245,,560,SSF,2025-03-15,"Open, Day 1"',
      "Kim,F,B,Raw,,,,,122.5,-127.5,127.5,,,,,,127.5,SSF,2025-06-01,Bench cup",
      "Kim,F,SBD,Single-ply,200,210,220,,130,135,140,,240,250,260,,620,SSF,2025-09-01,Gear",
      "Kim,F,SBD,Raw,,,,,,,,,,,,,560,SSF,2024-03-01,Best lifts only",
    ].join("\n");
    const { meets, skipped } = parsePastMeets(csv);
    expect(skipped).toBe(2);
    expect(meets).toEqual([
      {
        day: dayNumber("2025-03-15"),
        attempts: {
          squat: [
            { kg: 180, made: true },
            { kg: 190, made: true },
            { kg: 197.5, made: false },
          ],
          bench: [
            { kg: 120, made: true },
            { kg: 125, made: true },
            { kg: 130, made: false },
          ],
          deadlift: [
            { kg: 220, made: true },
            { kg: 235, made: true },
            { kg: 245, made: true },
          ],
        },
      },
      {
        day: dayNumber("2025-06-01"),
        attempts: {
          squat: [],
          bench: [
            { kg: 122.5, made: true },
            { kg: 127.5, made: false },
            { kg: 127.5, made: true },
          ],
          deadlift: [],
        },
      },
    ]);
  });

  it("reads a date and nine attempts per line", () => {
    const { meets, skipped } = parsePastMeets(
      "2025-03-15 180 190 -197.5 120 125 0 220, 235; 245\n\nnot a meet\n2025-02-30 1 2 3 4 5 6 7 8 9",
    );
    expect(skipped).toBe(2);
    expect(meets).toHaveLength(1);
    const [first] = meets;
    expect(first?.attempts.bench).toEqual([
      { kg: 120, made: true },
      { kg: 125, made: true },
    ]);
    expect(first?.attempts.deadlift[2]).toEqual({ kg: 245, made: true });
  });
});

it("counts days from 1970 and rejects impossible dates", () => {
  expect(dayNumber("1970-01-01")).toBe(0);
  expect(dayNumber("2024-03-02")).toBe(19784);
  expect(dayNumber("2024-02-30")).toBeUndefined();
  expect(dayNumber("2024-13-01")).toBeUndefined();
  expect(today(new Date("2024-03-02T23:00:00Z"))).toBe(19784);
});

it("asks for an estimate with the page's curves and day-form spread", () => {
  const request = buildEstimateRequest(
    EXAMPLE,
    [],
    100,
    CALIBRATIONS.any.typicalOpener,
  );
  expect(request.curves.bench).toEqual({
    ratioAtHalf: 1,
    steepness: EXAMPLE["bench-steepness"],
    technicalMissChance: EXAMPLE["bench-technicalMissPercent"] / 100,
  });
  expect(request.dayFormSpread).toBe(EXAMPLE.spreadPercent / 100);
  expect(request.onDay).toBe(100);
});

it("fills in the estimated maxes and their shared uncertainty", () => {
  const values = estimateValues({
    lifts: {
      squat: { expectedMaxKg: 201.3, uncertainty: 0.012, meets: 4 },
      bench: undefined,
      deadlift: { expectedMaxKg: 244.6, uncertainty: 0.03, meets: 4 },
    },
    yearlyProgress: 0.02,
    sharedUncertainty: 0.021,
  });
  expect(values).toEqual([
    ["squat-expectedMaxKg", 201.5],
    ["deadlift-expectedMaxKg", 244.5],
    ["maxUncertaintyPercent", 2.1],
  ]);
});

describe("bestMade", () => {
  it("finds each lift's heaviest made attempt, skipping misses", () => {
    const { meets } = parsePastMeets(
      [
        "2024-10-01 170 180 -185 110 115 120 200 210 -220",
        "2025-03-15 180 190 -197.5 120 125 -130 0 0 0",
      ].join("\n"),
    );
    expect(bestMade(meets)).toEqual({
      squat: 190,
      bench: 125,
      deadlift: 210,
    });
    expect(bestMade([]).squat).toBeUndefined();
  });
});

describe("estimateLine", () => {
  const found = { expectedMaxKg: 237.7, uncertainty: 0.031, meets: 3 };

  it("says how far an estimate is above the best made lift", () => {
    expect(estimateLine("squat", found, 210)).toBe(
      "Squat: 237.5 kg ± 3.1%, from 3 meets. Best made: 210 kg, so the estimate is 27.5 kg above anything you made.",
    );
  });

  it("only gives the best made lift when the estimate is not above it", () => {
    expect(
      estimateLine("bench", { ...found, expectedMaxKg: 130, meets: 1 }, 135),
    ).toBe("Bench press: 130 kg ± 3.1%, from 1 meet. Best made: 135 kg.");
  });

  it("says when no attempt was made", () => {
    expect(estimateLine("deadlift", found, undefined)).toMatch(
      /No deadlift made in these meets\.$/,
    );
  });
});
