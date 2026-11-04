import { describe, expect, it } from "vitest";
import {
  CALIBRATIONS,
  calibrationValues,
  describeKept,
  EXAMPLE,
  switchCalibration,
} from "./example.ts";
import { INPUT_NAMES } from "./inputs.ts";

it("sets every curve and day-form input for a group", () => {
  const values = calibrationValues("women");
  expect(values).toHaveLength(3 * 3 + 3);
  expect(values.every(([name]) => INPUT_NAMES.includes(name))).toBe(true);
  expect(values).toContainEqual(["bench-steepness", 77]);
  expect(values).toContainEqual(["correlation", 0.36]);
});

it("starts from the calibration for any lifter", () => {
  expect(EXAMPLE["deadlift-steepness"]).toBe(
    CALIBRATIONS.any.steepness.deadlift,
  );
  expect(EXAMPLE.spreadPercent).toBe(CALIBRATIONS.any.spreadPercent);
});

describe("switchCalibration", () => {
  it("moves every input still at the old group's value to the new group", () => {
    const { changes, kept } = switchCalibration(EXAMPLE, "any", "women");
    expect(changes).toEqual(calibrationValues("women"));
    expect(kept).toEqual([]);
  });

  it("keeps the values the lifter changed", () => {
    const values = { ...EXAMPLE, "bench-steepness": 80, correlation: 0.5 };
    const { changes, kept } = switchCalibration(values, "any", "men");
    expect(changes.map(([name]) => name)).not.toContain("bench-steepness");
    expect(changes.map(([name]) => name)).not.toContain("correlation");
    expect(changes).toContainEqual(["squat-steepness", 58]);
    expect(kept).toEqual(["bench-steepness", "correlation"]);
  });

  it("does not report a changed value that already matches the new group", () => {
    const values = { ...EXAMPLE, "bench-steepness": 99 };
    expect(switchCalibration(values, "any", "men").kept).toEqual([]);
  });

  it("fills an empty input", () => {
    const values = { ...EXAMPLE, spreadPercent: NaN };
    const { changes, kept } = switchCalibration(values, "any", "women");
    expect(changes).toContainEqual(["spreadPercent", 3.2]);
    expect(kept).toEqual([]);
  });
});

describe("describeKept", () => {
  it("names the kept inputs", () => {
    expect(describeKept(["bench-steepness", "correlation"])).toBe(
      "Kept your own values for bench steepness and correlation.",
    );
    expect(describeKept(["squat-technicalMissPercent"])).toBe(
      "Kept your own value for squat technical-miss chance.",
    );
  });

  it("says nothing when every value followed the group", () => {
    expect(describeKept([])).toBe("");
  });
});
