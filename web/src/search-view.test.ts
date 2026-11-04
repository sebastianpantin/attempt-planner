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

it("says when no plan keeps within the cap, and that the plan found is the safest", () => {
  expect(describeSearch(progress({ done: true, round: 3 }), 0)).toBe(
    "No plan keeps the bomb-out chance within 0%. This is the safest plan found: 78.1% chance of reaching the goal, 553.2 kg average total, 4.8% bomb-out chance.",
  );
});

it("reports a plan within the cap as found", () => {
  expect(describeSearch(progress({ done: true, round: 1 }), 0.05)).toBe(
    "Found in 1 round and filled in: 78.1% chance of reaching the goal, 553.2 kg average total, 4.8% bomb-out chance.",
  );
});

it("warns when a search with any risk allowed finds a plan that bombs out often", () => {
  expect(
    describeSearch(
      progress({ done: true, round: 2, bombOutProbability: 0.155 }),
    ),
  ).toMatch(
    /15\.5% bomb-out chance\. With any risk allowed, this plan bombs out often\./,
  );
  expect(
    describeSearch(
      progress({ done: true, round: 2, bombOutProbability: 0.155 }),
      0.2,
    ),
  ).not.toMatch(/With any risk allowed/);
});
