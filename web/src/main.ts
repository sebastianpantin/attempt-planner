import type {
  EvaluateRequest,
  Evaluation,
  Lift,
  MaxEstimate,
  Problem,
  TradeoffPoint,
  TradeoffResponse,
} from "../pkg/attempt_sim_wasm.js";
import { type AttemptNode, attemptTree } from "./attempt-tree.ts";
import { describePoint, drawTradeoff } from "./chart.ts";
import type { Failed } from "./engine.ts";
import {
  CALIBRATIONS,
  describeKept,
  EXAMPLE,
  type Sex,
  switchCalibration,
} from "./example.ts";
import * as format from "./format.ts";
import {
  buildRequest,
  buildSearchRequest,
  buildTradeoffRequest,
  describeProblem,
  type FormValues,
  INPUT_NAMES,
  type InputName,
  inputName,
  inputsFor,
  type LiftField,
  LIFTS,
  planValues,
  PRECISE_SIMULATIONS,
} from "./inputs.ts";
import {
  buildEstimateRequest,
  estimateValues,
  parsePastMeets,
  today,
} from "./past-meets.ts";
import { startSearch } from "./search-runner.ts";
import { describeSearch } from "./search-view.ts";
import { simulationWorker } from "./simulation.ts";
import { highlightedPoints, shapeLabel } from "./tradeoff-view.ts";

const LIFT_NAMES: Record<Lift, string> = {
  squat: "Squat",
  bench: "Bench press",
  deadlift: "Deadlift",
};
const ORDINALS = ["1st", "2nd", "3rd"];
const TYPING_PAUSE_MS = 250;
const count = new Intl.NumberFormat("en");

const byId = (id: string) => document.getElementById(id)!;
const form = byId("plan") as HTMLFormElement;
const input = (name: InputName | "precise") =>
  form.elements.namedItem(name) as HTMLInputElement;

function liftFieldset(lift: Lift): string {
  const name = LIFT_NAMES[lift];
  const jump = (field: LiftField, label: string) =>
    `+ <input name="${inputName(lift, field)}" type="number" min="0" step="2.5"
         aria-label="${name} ${label} (kg)" /> kg`;
  return `
    <fieldset class="lift">
      <legend>${name}</legend>
      <label>Expected max (kg)
        <input name="${inputName(lift, "expectedMaxKg")}" type="number" min="20" step="2.5" />
      </label>
      <label>Opener (kg)
        <input name="${inputName(lift, "openerKg")}" type="number" min="20" step="2.5" />
      </label>
      <table class="jumps">
        <thead>
          <tr><td></td><th scope="col">After a make</th><th scope="col">After a miss</th></tr>
        </thead>
        <tbody>
          <tr>
            <th scope="row">2nd attempt</th>
            <td>${jump("secondAfterMakeKg", "2nd attempt jump after a make")}</td>
            <td>${jump("secondAfterMissKg", "2nd attempt jump after a miss")}</td>
          </tr>
          <tr>
            <th scope="row">3rd attempt</th>
            <td>${jump("thirdAfterMakeKg", "3rd attempt jump after a make")}</td>
            <td>${jump("thirdAfterMissKg", "3rd attempt jump after a miss")}</td>
          </tr>
        </tbody>
      </table>
      <ul class="tree" id="${lift}-tree" aria-label="${name} attempts"></ul>
      <ul class="problems" id="${lift}-problems"></ul>
    </fieldset>`;
}

function curveRow(lift: Lift): string {
  const name = LIFT_NAMES[lift];
  return `
    <tr>
      <th scope="row">${name}</th>
      <td><input name="${inputName(lift, "ratioAtHalf")}" type="number" min="0" step="0.01"
            aria-label="${name} make-rate midpoint" /></td>
      <td><input name="${inputName(lift, "steepness")}" type="number" min="0" step="1"
            aria-label="${name} make-rate steepness" /></td>
      <td><input name="${inputName(lift, "technicalMissPercent")}" type="number" min="0"
            max="99" step="0.5" aria-label="${name} technical-miss chance (%)" /></td>
    </tr>`;
}

byId("lifts").innerHTML = LIFTS.map(liftFieldset).join("");
byId("curves").innerHTML = `
  <table>
    <thead><tr><td></td><th scope="col">Midpoint</th><th scope="col">Steepness</th>
      <th scope="col">Technical miss (%)</th></tr></thead>
    <tbody>${LIFTS.map(curveRow).join("")}</tbody>
  </table>`;

for (const name of INPUT_NAMES) input(name).valueAsNumber = EXAMPLE[name];

const readValues = () =>
  Object.fromEntries(
    INPUT_NAMES.map((name) => [name, input(name).valueAsNumber]),
  ) as FormValues;

function listItem(text: string): HTMLLIElement {
  const item = document.createElement("li");
  item.textContent = text;
  return item;
}

const OUTCOMES = {
  afterMake: { mark: "✓ ", text: "after a make: " },
  afterMiss: { mark: "✗ ", text: "after a miss: " },
} as const;

function treeItem(
  node: AttemptNode,
  attempt = 0,
  outcome?: keyof typeof OUTCOMES,
): HTMLLIElement {
  const item = listItem(`${ORDINALS[attempt]} ${format.kg(node.weightKg)}`);
  if (outcome) {
    const mark = document.createElement("span");
    mark.ariaHidden = "true";
    mark.textContent = OUTCOMES[outcome].mark;
    const text = document.createElement("span");
    text.className = "visually-hidden";
    text.textContent = OUTCOMES[outcome].text;
    item.prepend(mark, text);
  }
  const children = (["afterMake", "afterMiss"] as const).flatMap((branch) => {
    const child = node[branch];
    return child ? [treeItem(child, attempt + 1, branch)] : [];
  });
  if (children.length > 0) {
    const list = document.createElement("ul");
    list.append(...children);
    item.append(list);
  }
  return item;
}

function showTrees(request: EvaluateRequest) {
  for (const lift of LIFTS) {
    const tree = attemptTree(request.plans[lift]);
    byId(`${lift}-tree`).replaceChildren(...(tree ? [treeItem(tree)] : []));
  }
}

function markProblems(problems: Problem[]) {
  for (const name of INPUT_NAMES) {
    input(name).removeAttribute("aria-invalid");
    input(name).removeAttribute("aria-describedby");
  }
  for (const list of form.querySelectorAll("ul.problems")) {
    list.replaceChildren();
  }
  for (const [index, problem] of problems.entries()) {
    const item = listItem(describeProblem(problem));
    item.id = `problem-${index}`;
    for (const name of inputsFor(problem)) {
      const marked = input(name);
      marked.setAttribute("aria-invalid", "true");
      const described = marked.getAttribute("aria-describedby");
      marked.setAttribute(
        "aria-describedby",
        described ? `${described} ${item.id}` : item.id,
      );
      const advanced = marked.closest("details");
      if (advanced) advanced.open = true;
    }
    byId(`${problem.lift ?? "meet"}-problems`).append(item);
  }
}

function showEvaluation(
  evaluation: Evaluation | undefined,
  simulations: number,
) {
  byId("evaluation").hidden = !evaluation;
  if (!evaluation) return;
  const { standardErrors: errors, totalPercentilesKg: totals } = evaluation;
  const goalRows: [string, string][] =
    evaluation.goalProbability === undefined
      ? []
      : [
          [
            "Chance of reaching the goal",
            format.percent(evaluation.goalProbability),
          ],
        ];
  const rows: [string, string][] = [
    [
      "Average total, counting a bomb-out as 0",
      format.kg(evaluation.expectedTotalKg),
    ],
    ...goalRows,
    ["Chance of bombing out", format.percent(evaluation.bombOutProbability)],
    [
      "Average total without a bomb-out",
      evaluation.expectedTotalWithoutBombOutKg === undefined
        ? "every meet bombed out"
        : format.kg(evaluation.expectedTotalWithoutBombOutKg),
    ],
    [
      "Total at the 10th, 50th and 90th percentile",
      [totals.p10, totals.p50, totals.p90].map(format.kg).join(" / "),
    ],
  ];
  byId("summary").replaceChildren(
    ...rows.flatMap(([term, value]) => {
      const dt = document.createElement("dt");
      const dd = document.createElement("dd");
      dt.textContent = term;
      dd.textContent = value;
      return [dt, dd];
    }),
  );
  byId("simulation-error").textContent = format.simulationError(
    errors,
    simulations < PRECISE_SIMULATIONS,
  );
  byId("lift-results").replaceChildren(
    ...LIFTS.map((lift) => {
      const {
        makeProbability,
        medianBestKg,
        strengthMissRate,
        technicalMissRate,
      } = evaluation.lifts[lift];
      const row = document.createElement("tr");
      const heading = document.createElement("th");
      heading.scope = "row";
      heading.textContent = LIFT_NAMES[lift];
      row.append(heading);
      row.insertCell().textContent = format.percent(makeProbability);
      row.insertCell().textContent =
        medianBestKg === undefined ? "–" : format.kg(medianBestKg);
      row.insertCell().textContent = format.percent(strengthMissRate);
      row.insertCell().textContent = format.percent(technicalMissRate);
      return row;
    }),
  );
}

const status = (id: string, text: string) => {
  byId(id).textContent = text;
};

let shownEvaluation: Evaluation | undefined;
let shownTradeoff: Extract<TradeoffResponse, { kind: "swept" }> | undefined;

function usePlans(plans: TradeoffPoint["plans"]) {
  for (const [name, kg] of planValues(plans)) {
    input(name).valueAsNumber = kg;
  }
  update();
}

function showTradeoff() {
  const view = byId("tradeoff-view");
  view.hidden = !shownTradeoff?.points.length;
  if (!shownTradeoff?.points.length) return;
  const { points, bestForGoal } = shownTradeoff;
  drawTradeoff(
    byId("tradeoff-chart") as unknown as SVGSVGElement,
    {
      points,
      bestForGoal,
      current: shownEvaluation && {
        bombOutProbability: shownEvaluation.bombOutProbability,
        expectedTotalKg: shownEvaluation.expectedTotalKg,
      },
    },
    (point) => usePlans(point.plans),
  );
  byId("tradeoff-legend").hidden = bestForGoal === undefined;
  byId("tradeoff-plans").replaceChildren(
    ...highlightedPoints(points, bestForGoal).map(({ point, bestForGoal }) => {
      const row = document.createElement("tr");
      const heading = document.createElement("th");
      heading.scope = "row";
      heading.textContent = shapeLabel(point.shape);
      if (bestForGoal) heading.append(" (best for the goal)");
      row.append(heading);
      row.insertCell().textContent = format.percent(point.bombOutProbability);
      row.insertCell().textContent = format.kg(point.expectedTotalKg);
      row.insertCell().textContent =
        point.goalProbability === undefined
          ? "–"
          : format.percent(point.goalProbability);
      const use = document.createElement("button");
      use.type = "button";
      use.textContent = "Use";
      use.setAttribute("aria-label", `Use ${describePoint(point)}`);
      use.addEventListener("click", () => usePlans(point.plans));
      row.insertCell().append(use);
      return row;
    }),
  );
}

function report(
  id: string,
  reply: { kind: "invalid" } | Failed | { kind: string },
  done: () => string,
  invalid: string,
  failedAs: string,
) {
  if (reply.kind === "invalid") status(id, invalid);
  else if ("message" in reply) status(id, `${failedAs}: ${reply.message}`);
  else status(id, done());
}

const submitEvaluation = simulationWorker("evaluate", {
  started: (request) =>
    status("status", `Simulating ${count.format(request.simulations)} meets…`),
  finished(reply, request) {
    byId("results").removeAttribute("aria-busy");
    markProblems(reply.kind === "invalid" ? reply.problems : []);
    shownEvaluation = reply.kind === "evaluated" ? reply.evaluation : undefined;
    showEvaluation(shownEvaluation, request.simulations);
    showTradeoff();
    report(
      "status",
      reply,
      () => `${count.format(request.simulations)} simulated meets.`,
      "Fix the marked inputs to see results.",
      "The simulation failed",
    );
  },
});

const submitTradeoff = simulationWorker("tradeoff", {
  started: () => status("tradeoff-status", "Comparing plans…"),
  finished(reply, request) {
    byId("tradeoff").removeAttribute("aria-busy");
    shownTradeoff = reply.kind === "swept" ? reply : undefined;
    showTradeoff();
    report(
      "tradeoff-status",
      reply,
      () =>
        shownTradeoff?.points.length
          ? `${count.format(shownTradeoff.points.length)} plans, ${count.format(request.simulations)} meets each.`
          : "No plan in the family fits these maxes.",
      "Fix the marked inputs to see the tradeoff.",
      "The comparison failed",
    );
  },
});

let sweptKey = "";
function simulate(values: FormValues) {
  submitEvaluation(buildRequest(values, input("precise").checked));
  const sweep = buildTradeoffRequest(values);
  const key = JSON.stringify(sweep);
  if (key !== sweptKey) {
    sweptKey = key;
    byId("tradeoff").setAttribute("aria-busy", "true");
    submitTradeoff(sweep);
  }
}

let typingPause: ReturnType<typeof setTimeout> | undefined;
function update() {
  const values = readValues();
  showTrees(buildRequest(values, false));
  byId("results").setAttribute("aria-busy", "true");
  status("status", "Updating…");
  clearTimeout(typingPause);
  typingPause = setTimeout(() => simulate(values), TYPING_PAUSE_MS);
}

const findButton = byId("find-plan") as HTMLButtonElement;
const cancelButton = byId("cancel-search") as HTMLButtonElement;
let stopSearch: (() => void) | undefined;

function searchEnded(message: string) {
  stopSearch = undefined;
  findButton.disabled = false;
  if (document.activeElement === cancelButton) findButton.focus();
  cancelButton.hidden = true;
  status("search-status", message);
}

findButton.addEventListener("click", () => {
  findButton.disabled = true;
  cancelButton.hidden = false;
  cancelButton.focus();
  status("search-status", "Searching…");
  stopSearch = startSearch(buildSearchRequest(readValues()), {
    progress(progress) {
      if (!progress.done) {
        status("search-status", describeSearch(progress));
        return;
      }
      usePlans(progress.plans);
      searchEnded(describeSearch(progress));
    },
    failed: (message) => searchEnded(`The search failed: ${message}`),
  });
});

cancelButton.addEventListener("click", () => {
  stopSearch?.();
  searchEnded("Search cancelled. The plan is unchanged.");
});

const sexSelect = form.elements.namedItem("sex") as HTMLSelectElement;
const sex = () => sexSelect.value as Sex;
let calibratedSex: Sex = "any";

const pastMeets = byId("past-meets") as HTMLTextAreaElement;
const estimateButton = byId("estimate-maxes") as HTMLButtonElement;
const useEstimateButton = byId("use-estimate") as HTMLButtonElement;
let estimatedValues: [InputName, number][] = [];
let skippedMeets = 0;

function showEstimate(estimate: MaxEstimate | undefined) {
  estimatedValues = estimate ? estimateValues(estimate) : [];
  useEstimateButton.hidden = estimatedValues.length === 0;
  byId("estimate-results").replaceChildren(
    ...(estimate
      ? LIFTS.flatMap((lift) => {
          const found = estimate.lifts[lift];
          return found
            ? [
                listItem(
                  `${LIFT_NAMES[lift]}: ${format.kg(found.expectedMaxKg)} ± ${format.percent(found.uncertainty)}, from ${count.format(found.meets)} ${found.meets === 1 ? "meet" : "meets"}`,
                ),
              ]
            : [];
        })
      : []),
  );
}

const submitEstimate = simulationWorker("estimate", {
  started: () => status("estimate-status", "Estimating…"),
  finished(reply) {
    estimateButton.disabled = false;
    if ("message" in reply) {
      showEstimate(undefined);
      status("estimate-status", `The estimate failed: ${reply.message}`);
      return;
    }
    const { estimate } = reply;
    showEstimate(estimate);
    const skipped =
      skippedMeets > 0
        ? ` Skipped ${count.format(skippedMeets)} ${skippedMeets === 1 ? "line" : "lines"} that were not a raw meet with attempts.`
        : "";
    status(
      "estimate-status",
      `Expected maxes today, getting ${format.percent(Math.abs(estimate.yearlyProgress))} ${estimate.yearlyProgress < 0 ? "weaker" : "stronger"} a year.${skipped}`,
    );
  },
});

estimateButton.addEventListener("click", () => {
  const { meets, skipped } = parsePastMeets(pastMeets.value);
  skippedMeets = skipped;
  if (meets.length === 0) {
    showEstimate(undefined);
    status(
      "estimate-status",
      "No raw meet with attempts found. Check the format above.",
    );
    return;
  }
  estimateButton.disabled = true;
  submitEstimate(
    buildEstimateRequest(
      readValues(),
      meets,
      today(),
      CALIBRATIONS[sex()].typicalOpener,
    ),
  );
});

useEstimateButton.addEventListener("click", () => {
  for (const [name, value] of estimatedValues)
    input(name).valueAsNumber = value;
  update();
});

form.addEventListener("input", (event) => {
  if (event.target === pastMeets) return;
  if (event.target === sexSelect) {
    const { changes, kept } = switchCalibration(
      readValues(),
      calibratedSex,
      sex(),
    );
    calibratedSex = sex();
    for (const [name, value] of changes) input(name).valueAsNumber = value;
    status("sex-status", describeKept(kept));
  }
  if (stopSearch) {
    stopSearch();
    searchEnded("Search cancelled because the inputs changed.");
  }
  update();
});
form.addEventListener("submit", (event) => event.preventDefault());

const initial = readValues();
showTrees(buildRequest(initial, false));
simulate(initial);
