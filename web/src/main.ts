import type {
  ChangesResponse,
  EvaluateRequest,
  Evaluation,
  Lift,
  MaxEstimate,
  Percentiles,
  Problem,
  SearchRequest,
  TradeoffPoint,
  TradeoffRequest,
  TradeoffResponse,
} from "../pkg/attempt_sim_wasm.js";
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
  sweepOnlyProblems,
} from "./inputs.ts";
import {
  buildEstimateRequest,
  estimateValues,
  parsePastMeets,
  today,
} from "./past-meets.ts";
import {
  barFigures,
  outcomeSentence,
  rangeLayout,
  ringFigure,
  shortKg,
  weight,
} from "./outcome.ts";
import {
  bombOutCapLabel,
  bombOutCapPercent,
  ladder,
  problemListId,
} from "./plan-view.ts";
import { renderAnalytics } from "./analytics/analytics.ts";
import { startSearch } from "./search-runner.ts";
import { describeSearch } from "./search-view.ts";
import { simulationWorker } from "./simulation.ts";
import {
  bestForGoalLabel,
  highlightedPoints,
  noGoalPickNote,
  shapeLabel,
} from "./tradeoff-view.ts";

const LIFT_NAMES: Record<Lift, string> = {
  squat: "Squat",
  bench: "Bench press",
  deadlift: "Deadlift",
};
const TYPING_PAUSE_MS = 250;
const count = new Intl.NumberFormat("en");
const CONTROL_NAMES = new Set<string>([
  ...INPUT_NAMES,
  "sex",
  "precise",
  "noBombOutCap",
]);

const byId = (id: string) => document.getElementById(id)!;
const form = byId("plan") as HTMLFormElement;
const summaryBar = byId("summary-bar");
const input = (name: InputName | "precise" | "noBombOutCap") =>
  form.elements.namedItem(name) as HTMLInputElement;

/** A number input between −/+ buttons; the buttons stay out of the tab order as arrow keys step the input. */
function stepper(lift: Lift, field: LiftField, label: string): string {
  const repeat = field.endsWith("AfterMissKg")
    ? '<span class="repeat" aria-hidden="true">repeat</span>'
    : "";
  return `
    <span class="stepper">
      <button type="button" tabindex="-1" data-step="-1" aria-label="${label}: 2.5 kg less">−</button>
      <span class="stepper-value">
        <input name="${inputName(lift, field)}" type="number" min="${field === "openerKg" ? 20 : 0}"
          step="2.5" aria-label="${label} (kg)" />${repeat}
      </span>
      <button type="button" tabindex="-1" data-step="1" aria-label="${label}: 2.5 kg more">+</button>
    </span>`;
}

function attemptCard(lift: Lift, attempt: "2nd" | "3rd"): string {
  const name = LIFT_NAMES[lift];
  const jump = (outcome: "Make" | "Miss") =>
    stepper(
      lift,
      `${attempt === "2nd" ? "second" : "third"}After${outcome}Kg`,
      `${name} ${attempt} attempt jump after a ${outcome.toLowerCase()}`,
    );
  return `
    <div class="attempt">
      <p class="n">${attempt}</p>
      <p class="kg"><span id="${lift}-${attempt}"></span> <small>kg if made</small></p>
      <div class="jump"><span class="tag make" aria-hidden="true">✓ made</span>${jump("Make")}</div>
      <div class="jump"><span class="tag miss" aria-hidden="true">✗ missed</span>${jump("Miss")}</div>
    </div>`;
}

function liftCard(lift: Lift, index: number): string {
  const name = LIFT_NAMES[lift];
  return `
    <details class="lift"${index === 0 ? " open" : ""}>
      <summary>
        <h3>${name}</h3>
        <span class="lift-line" id="${lift}-line"></span>
        <span class="pill" id="${lift}-make"></span>
      </summary>
      <div class="ladder">
        <div class="attempt opener">
          <p class="n">Opener</p>
          ${stepper(lift, "openerKg", `${name} opener`)}
          <p class="note" id="${lift}-share"></p>
        </div>
        ${attemptCard(lift, "2nd")}
        ${attemptCard(lift, "3rd")}
      </div>
      <p class="path" id="${lift}-paths"></p>
      <ul class="problems" id="${lift}-problems"></ul>
    </details>`;
}

const maxField = (lift: Lift) => `
  <label class="field">
    <span>${LIFT_NAMES[lift]}<span class="visually-hidden"> expected max</span></span>
    <span class="input big">
      <input name="${inputName(lift, "expectedMaxKg")}" type="number" min="20" step="2.5" />
      <span aria-hidden="true">kg</span>
    </span>
  </label>`;

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

byId("maxes").innerHTML = LIFTS.map(maxField).join("");
byId("lifts").innerHTML = LIFTS.map(liftCard).join("");
byId("curves").innerHTML = `
  <table>
    <thead><tr><td></td><th scope="col">Midpoint</th><th scope="col">Steepness</th>
      <th scope="col">Technical miss (%)</th></tr></thead>
    <tbody>${LIFTS.map(curveRow).join("")}</tbody>
  </table>`;

for (const name of INPUT_NAMES) input(name).valueAsNumber = EXAMPLE[name];

const bombOutCapSlider = input("bombOutCapPercent");
const noBombOutCap = input("noBombOutCap");

const readValues = () =>
  ({
    ...Object.fromEntries(
      INPUT_NAMES.map((name) => [name, input(name).valueAsNumber]),
    ),
    bombOutCapPercent: bombOutCapPercent(
      bombOutCapSlider.valueAsNumber,
      noBombOutCap.checked,
    ),
  }) as FormValues;

function showBombOutCap() {
  bombOutCapSlider.disabled = noBombOutCap.checked;
  const label = bombOutCapLabel(
    bombOutCapSlider.valueAsNumber,
    noBombOutCap.checked,
  );
  byId("bomb-out-cap-value").textContent = label;
  bombOutCapSlider.setAttribute("aria-valuetext", label);
}

function listItem(text: string): HTMLLIElement {
  const item = document.createElement("li");
  item.textContent = text;
  return item;
}

function strong(text: string): HTMLElement {
  const element = document.createElement("b");
  element.textContent = text;
  return element;
}

const REPEAT_FIELDS = ["secondAfterMissKg", "thirdAfterMissKg"] as const;

function showPlans(values: FormValues) {
  const { plans } = buildRequest(values, false);
  for (const lift of LIFTS) {
    const view = ladder(plans[lift], values[inputName(lift, "expectedMaxKg")]);
    const shown = (kg: number | undefined) =>
      kg === undefined ? "–" : weight(kg);
    byId(`${lift}-2nd`).textContent = shown(view.secondKg);
    byId(`${lift}-3rd`).textContent = shown(view.thirdKg);
    byId(`${lift}-share`).textContent = view.shareOfMax ?? "";
    byId(`${lift}-line`).textContent = view.line ? `· ${view.line}` : "";
    byId(`${lift}-paths`).replaceChildren(
      ...view.paths.map(([label, weights]) => {
        const item = document.createElement("span");
        item.append(`${label}: `, strong(weights));
        return item;
      }),
    );
    for (const field of REPEAT_FIELDS) {
      const jump = input(inputName(lift, field));
      jump
        .closest(".stepper")!
        .toggleAttribute("data-repeat", jump.valueAsNumber === 0);
    }
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
      const collapsed = marked.closest("details");
      if (collapsed) collapsed.open = true;
    }
    byId(problemListId(problem)).append(item);
  }
}

function showEvaluation(
  evaluation: Evaluation | undefined,
  request: EvaluateRequest,
) {
  byId("evaluation").hidden = !evaluation;
  byId("evaluation-details").hidden = !evaluation;
  updateSummaryBar();
  for (const lift of LIFTS) {
    byId(`${lift}-make`).textContent = evaluation
      ? `${format.percent(evaluation.lifts[lift].makeProbability)} at least one make`
      : "";
  }
  if (!evaluation) return;
  const { goalKg } = request;
  const ring = ringFigure(evaluation, goalKg);
  byId("ring").style.setProperty("--share", String(ring.share));
  byId("ring-value").textContent = ring.value;
  byId("ring-label").textContent = ring.label;
  byId("outcome-sentence").replaceChildren(
    ...outcomeSentence(evaluation, goalKg).map(({ text, strong: stressed }) =>
      stressed ? strong(text) : text,
    ),
  );
  byId("average-total").textContent = format.kg(evaluation.expectedTotalKg);
  byId("bomb-out").textContent = format.percent(evaluation.bombOutProbability);
  showRange(evaluation.totalPercentilesKg, goalKg);
  byId("summary-bar-figures").replaceChildren(
    ...barFigures(evaluation, goalKg).map(([value, meaning]) => {
      const figure = document.createElement("span");
      figure.append(strong(value), ` ${meaning}`);
      return figure;
    }),
  );
  byId("without-bomb-out").textContent = `Average total without a bomb-out: ${
    evaluation.expectedTotalWithoutBombOutKg === undefined
      ? "every meet bombed out"
      : format.kg(evaluation.expectedTotalWithoutBombOutKg)
  }.`;
  byId("simulation-error").textContent = format.simulationError(
    evaluation.standardErrors,
    request.simulations,
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

function showRange(totals: Percentiles, goalKg: number | undefined) {
  const layout = rangeLayout(totals, goalKg);
  const fill = byId("range-fill").style;
  fill.left = `${layout.fromPercent}%`;
  fill.right = `${100 - layout.toPercent}%`;
  byId("range-median").style.left = `${layout.medianPercent}%`;
  const goal = byId("range-goal");
  goal.hidden = layout.goalPercent === undefined;
  goal.style.left = `${layout.goalPercent ?? 0}%`;
  byId("range-low").textContent = shortKg(totals.p10);
  byId("range-median-label").textContent = `median ${weight(totals.p50)}`;
  byId("range-high").textContent = shortKg(totals.p90);
}

const status = (id: string, text: string) => {
  byId(id).textContent = text;
};

let shownEvaluation: Evaluation | undefined;
let evaluatedGoalKg: number | undefined;
/** Single changes to the plans shown, cleared while newer plans are being simulated. */
let shownChanges:
  | {
      reply: Extract<ChangesResponse, { kind: "changed" }>;
      request: SearchRequest;
    }
  | undefined;

function showAnalytics() {
  const section = byId("analytics");
  section.hidden = !shownEvaluation;
  if (!shownEvaluation) return;
  renderAnalytics(byId("analytics-panels"), {
    evaluation: shownEvaluation,
    goalKg: evaluatedGoalKg,
    ...(shownChanges && {
      changes: shownChanges.reply,
      bombOutCap: shownChanges.request.bombOutCap,
    }),
    onApply: usePlans,
  });
}
let resultsInView = false;
/** The summary bar stands in for the results while they are out of sight. */
const updateSummaryBar = () => {
  summaryBar.hidden = !shownEvaluation || resultsInView;
};
new IntersectionObserver(([entry]) => {
  resultsInView = entry?.isIntersecting ?? false;
  updateSummaryBar();
}).observe(byId("results"));
/** Problems from the evaluation, and those only the sweep reports. */
const problems: { evaluation: Problem[]; sweep: Problem[] } = {
  evaluation: [],
  sweep: [],
};
const showProblems = () =>
  markProblems([...problems.evaluation, ...problems.sweep]);
let shownTradeoff:
  | {
      sweep: Extract<TradeoffResponse, { kind: "swept" }>;
      request: TradeoffRequest;
    }
  | undefined;

function usePlans(plans: TradeoffPoint["plans"]) {
  for (const [name, kg] of planValues(plans)) {
    input(name).valueAsNumber = kg;
  }
  update();
}

function showTradeoff() {
  const view = byId("tradeoff-view");
  view.hidden = !shownTradeoff?.sweep.points.length;
  if (!shownTradeoff?.sweep.points.length) return;
  const { points, bestForGoal } = shownTradeoff.sweep;
  const { bombOutCap, goalKg } = shownTradeoff.request;
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
  byId("tradeoff-note").textContent = noGoalPickNote(
    goalKg,
    bombOutCap,
    bestForGoal,
  );
  const bestLabel = bestForGoalLabel(bombOutCap);
  byId("tradeoff-best-label").textContent = bestLabel;
  byId("tradeoff-plans").replaceChildren(
    ...highlightedPoints(points, bestForGoal).map(({ point, bestForGoal }) => {
      const row = document.createElement("tr");
      const heading = document.createElement("th");
      heading.scope = "row";
      heading.textContent = shapeLabel(point.shape);
      if (bestForGoal) heading.append(` (${bestLabel})`);
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
    problems.evaluation = reply.kind === "invalid" ? reply.problems : [];
    showProblems();
    shownEvaluation = reply.kind === "evaluated" ? reply.evaluation : undefined;
    evaluatedGoalKg = request.goalKg;
    showEvaluation(shownEvaluation, request);
    showAnalytics();
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
    shownTradeoff =
      reply.kind === "swept" ? { sweep: reply, request } : undefined;
    problems.sweep =
      reply.kind === "invalid" ? sweepOnlyProblems(reply.problems) : [];
    showProblems();
    showTradeoff();
    report(
      "tradeoff-status",
      reply,
      () =>
        shownTradeoff?.sweep.points.length
          ? `${count.format(shownTradeoff.sweep.points.length)} plans, ${count.format(request.simulations)} meets each.`
          : "No plan in the family fits these maxes.",
      "Fix the marked inputs to see the tradeoff.",
      "The comparison failed",
    );
  },
});

const submitChanges = simulationWorker("changes", {
  started: () => {},
  finished(reply, request) {
    shownChanges = reply.kind === "changed" ? { reply, request } : undefined;
    showAnalytics();
  },
});

let sweptKey = "";
function simulate(values: FormValues) {
  submitEvaluation(buildRequest(values, input("precise").checked));
  shownChanges = undefined;
  submitChanges(buildSearchRequest(values));
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
  showBombOutCap();
  showPlans(values);
  byId("results").setAttribute("aria-busy", "true");
  status("status", "Updating…");
  clearTimeout(typingPause);
  typingPause = setTimeout(() => simulate(values), TYPING_PAUSE_MS);
}

const findButton = byId("find-plan") as HTMLButtonElement;
const barFindButton = byId("bar-find-plan") as HTMLButtonElement;
const cancelButton = byId("cancel-search") as HTMLButtonElement;
let stopSearch: (() => void) | undefined;

function searchEnded(message: string) {
  stopSearch = undefined;
  findButton.disabled = barFindButton.disabled = false;
  if (document.activeElement === cancelButton) findButton.focus();
  cancelButton.hidden = true;
  status("search-status", message);
}

findButton.addEventListener("click", () => {
  findButton.disabled = barFindButton.disabled = true;
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
barFindButton.addEventListener("click", () => findButton.click());

const compareButton = byId("compare-plans");
compareButton.addEventListener("click", () => {
  const tradeoff = byId("tradeoff");
  tradeoff.hidden = !tradeoff.hidden;
  compareButton.setAttribute("aria-expanded", String(!tradeoff.hidden));
  if (!tradeoff.hidden) tradeoff.scrollIntoView({ block: "start" });
});

form.addEventListener("click", (event) => {
  const button = (event.target as Element).closest<HTMLButtonElement>(
    "button[data-step]",
  );
  if (!button) return;
  const field = button.parentElement!.querySelector("input")!;
  if (button.dataset.step === "1") field.stepUp();
  else field.stepDown();
  field.dispatchEvent(new Event("input", { bubbles: true }));
});

/** Opening one lift closes the others, keeping the one being edited in view. */
byId("lifts").addEventListener("click", (event) => {
  const summary = (event.target as Element).closest("summary");
  const opened = summary?.parentElement as HTMLDetailsElement | undefined;
  if (!summary || !opened || opened.open) return;
  for (const lift of byId("lifts").querySelectorAll("details")) {
    if (lift !== opened) lift.open = false;
  }
  summary.scrollIntoView({ block: "nearest" });
});

const sex = () =>
  (form.elements.namedItem("sex") as RadioNodeList).value as Sex;
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
  const { name } = event.target as HTMLInputElement;
  if (!CONTROL_NAMES.has(name)) return;
  if (name === "sex") {
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
showBombOutCap();
showPlans(initial);
simulate(initial);
