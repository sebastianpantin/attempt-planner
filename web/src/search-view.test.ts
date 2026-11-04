import { expect, it } from "vitest";
import type { SearchProgress } from "../pkg/attempt_sim_wasm.js";
import { describeSearch } from "./search-view.ts";

const progress = (overrides: Partial<SearchProgress>): SearchProgress => ({
  round: 2,
  done: false,
  plans: {} as SearchProgress["plans"],
  bombOutProbability: 0.0484,
  expectedTotalKg: 553.21,
  goalProbability: 0.7812,
  ...overrides,
});

it("describes the best plan so far with its round", () => {
  expect(describeSearch(progress({}))).toBe(
    "Round 2, best so far: 78.1% chance of reaching the goal, 553.2 kg average total, 4.8% bomb-out chance.",
  );
});

it("leaves out the goal without one", () => {
  expect(describeSearch(progress({ goalProbability: undefined }))).toBe(
    "Round 2, best so far: 553.2 kg average total, 4.8% bomb-out chance.",
  );
});

it("says when the search is done", () => {
  expect(describeSearch(progress({ done: true, round: 3 }))).toBe(
    "Found in 3 rounds and filled in: 78.1% chance of reaching the goal, 553.2 kg average total, 4.8% bomb-out chance.",
  );
});
