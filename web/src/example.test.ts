import { expect, it } from "vitest";
import { CALIBRATIONS, calibrationValues, EXAMPLE } from "./example.ts";
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
