import type {
  ChangesResponse,
  EvaluateRequest,
  Evaluation,
  Lift,
  MaxEstimate,
  Percentiles,
  Problem,
  TradeoffPoint,
  TradeoffRequest,
  TradeoffResponse,
} from "../pkg/attempt_sim_wasm.js";
import { describePoint, drawTradeoff } from "./chart.ts";
import { element } from "./dom.ts";
import type { Failed } from "./engine.ts";
import {
  CALIBRATIONS,
  describeKept,
  EXAMPLE,
  type Sex,
  switchCalibration,
} from "./example.ts";
import * as format from "./format.ts";
import { LIFT_NAMES, shortKg, weight } from "./format.ts";
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
} from "./outcome.ts";
import {
  BOMB_OUT_CAPS,
  bombOutCapLabel,
  bombOutCapPercent,
  jumpLabels,
  ladder,
  problemListId,
  sliderPosition,
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

/**
 * A number input between −/+ buttons; the buttons stay out of the tab order as arrow keys
 * step the input. A jump shows signed, "+12.5" or "repeat", while it is not being edited.
 */
function stepper(lift: Lift, field: LiftField, label: string): string {
  const shownJump =
    field === "openerKg"
      ? ""
      : '<span class="shown-jump" aria-hidden="true"></span>';
  return `
    <span class="stepper">
      <button type="button" tabindex="-1" data-step="-1" aria-label="${label}: 2.5 kg less">−</button>
      <span class="stepper-value">
        <input name="${inputName(lift, field)}" type="number" min="${field === "openerKg" ? 20 : 0}"
          step="2.5" aria-label="${label} (kg)" />${shownJump}
      </span>
      <button type="button" tabindex="-1" data-step="1" aria-label="${label}: 2.5 kg more">+</button>
    </span>`;
}

const ATTEMPT_JUMPS = [
  {
    attempt: "2nd",
    afterMake: "secondAfterMakeKg",
    afterMiss: "secondAfterMissKg",
  },
  {
    attempt: "3rd",
    afterMake: "thirdAfterMakeKg",
    afterMiss: "thirdAfterMissKg",
  },
] as const satisfies {
  attempt: string;
  afterMake: LiftField;
  afterMiss: LiftField;
}[];

function attemptCard(
  lift: Lift,
  { attempt, afterMake, afterMiss }: (typeof ATTEMPT_JUMPS)[number],
): string {
  const label = (outcome: string) =>
    `${LIFT_NAMES[lift].full} ${attempt} attempt jump after a ${outcome}`;
  return `
    <div class="attempt">
      <p class="n">${attempt}</p>
      <p class="kg"><span id="${lift}-${attempt}"></span> <small>kg if made</small></p>
      <div class="jump"><span class="tag make" aria-hidden="true">✓ made</span>${stepper(lift, afterMake, label("make"))}</div>
      <div class="jump"><span class="tag miss" aria-hidden="true">✗ missed</span>${stepper(lift, afterMiss, label("miss"))}</div>
    </div>`;
}

function liftCard(lift: Lift, index: number): string {
  const name = LIFT_NAMES[lift].full;
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
        ${ATTEMPT_JUMPS.map((jumps) => attemptCard(lift, jumps)).join("")}
      </div>
      <p class="path" id="${lift}-paths"></p>
      <ul class="problems" id="${lift}-problems"></ul>
    </details>`;
}

const maxField = (lift: Lift) => `
  <label class="field">
    <span>${LIFT_NAMES[lift].full}<span class="visually-hidden"> expected max</span></span>
    <span class="input big">
      <input name="${inputName(lift, "expectedMaxKg")}" type="number" min="20" step="2.5" />
      <span aria-hidden="true">kg</span>
    </span>
  </label>`;

function curveRow(lift: Lift): string {
  const name = LIFT_NAMES[lift].full;
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

const bombOutCapSlider = input("bombOutCapPercent");
bombOutCapSlider.max = String(BOMB_OUT_CAPS.length - 1);
const noBombOutCap = input("noBombOutCap");

/** Sets an input; the cap slider takes the position of the cap. */
function setInput(name: InputName, value: number) {
  input(name).valueAsNumber =
    name === "bombOutCapPercent" ? sliderPosition(value) : value;
}

for (const name of INPUT_NAMES) setInput(name, EXAMPLE[name]);

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

const strong = (text: string) => element("b", "", text);

function showPlans(values: FormValues) {
  const { plans } = buildRequest(values, false);
  for (const lift of LIFTS) {
    const view = ladder(plans[lift], values[inputName(lift, "expectedMaxKg")]);
    byId(`${lift}-2nd`).textContent = view.secondKg;
    byId(`${lift}-3rd`).textContent = view.thirdKg;
    byId(`${lift}-share`).textContent = view.shareOfMax;
    byId(`${lift}-line`).textContent = view.line;
    byId(`${lift}-paths`).replaceChildren(
      ...view.paths.map(([label, weights]) => {
        const item = element("span", "", `${label}: `);
        item.append(strong(weights));
        return item;
      }),
    );
    for (const [name, label] of jumpLabels(values, lift)) {
      const stepper = input(name).closest(".stepper")!;
      stepper.toggleAttribute("data-shown", label !== undefined);
      stepper.querySelector(".shown-jump")!.textContent = label ?? "";
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
    const item = element("li", "", describeProblem(problem));
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
      const figure = element("span");
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
      heading.textContent = LIFT_NAMES[lift].full;
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

/** The evaluation shown, with the goal it was simulated for. */
let shown: { evaluation: Evaluation; goalKg?: number } | undefined;
/** Single changes to the plans shown, cleared while newer plans are being simulated. */
let shownChanges: Extract<ChangesResponse, { kind: "changed" }> | undefined;

function showAnalytics() {
  byId("analytics").hidden = !shown;
  if (!shown) return;
  renderAnalytics(byId("analytics-panels"), {
    ...shown,
    ...(shownChanges && { changes: shownChanges }),
    onApply: usePlans,
  });
}
let resultsInView = false;
/** The summary bar stands in for the results while they are out of sight. */
const updateSummaryBar = () => {
  summaryBar.hidden = !shown || resultsInView;
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
    setInput(name, kg);
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
      current: shown && {
        bombOutProbability: shown.evaluation.bombOutProbability,
        expectedTotalKg: shown.evaluation.expectedTotalKg,
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
    shown =
      reply.kind === "evaluated"
        ? { evaluation: reply.evaluation, goalKg: request.goalKg }
        : undefined;
    showEvaluation(shown?.evaluation, request);
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
  finished(reply) {
    shownChanges = reply.kind === "changed" ? reply : undefined;
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
const cancelButton = byId("cancel-search") as HTMLButtonElement;
const barFindButton = byId("bar-find-plan") as HTMLButtonElement;
const barCancelButton = byId("bar-cancel-search") as HTMLButtonElement;
let stopSearch: (() => void) | undefined;

/** The search's status, shown in the bottom bar too while it runs. */
function searchStatus(text: string) {
  status("search-status", text);
  status("bar-search-status", text);
}

/** Shows the search running or not, in the outcome card and the bottom bar. */
function showSearching(searching: boolean) {
  findButton.disabled = barFindButton.disabled = searching;
  cancelButton.hidden = barCancelButton.hidden = !searching;
  barFindButton.hidden = searching;
  byId("summary-bar-figures").hidden = searching;
  byId("bar-search-status").hidden = !searching;
}

function searchEnded(message: string) {
  stopSearch = undefined;
  const focused = document.activeElement;
  showSearching(false);
  if (focused === cancelButton) findButton.focus();
  if (focused === barCancelButton) barFindButton.focus();
  searchStatus(message);
}

/** Starts a search, moving focus to the Cancel button next to the one pressed. */
function search(cancel: HTMLButtonElement) {
  showSearching(true);
  cancel.focus();
  searchStatus("Searching…");
  const request = buildSearchRequest(readValues());
  stopSearch = startSearch(request, {
    progress(progress) {
      if (!progress.done) {
        searchStatus(describeSearch(progress));
        return;
      }
      usePlans(progress.plans);
      searchEnded(describeSearch(progress, request.bombOutCap));
    },
    failed: (message) => searchEnded(`The search failed: ${message}`),
  });
}

function cancelSearch() {
  stopSearch?.();
  searchEnded("Search cancelled. The plan is unchanged.");
}

findButton.addEventListener("click", () => search(cancelButton));
barFindButton.addEventListener("click", () => search(barCancelButton));
cancelButton.addEventListener("click", cancelSearch);
barCancelButton.addEventListener("click", cancelSearch);

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
                element(
                  "li",
                  "",
                  `${LIFT_NAMES[lift].full}: ${format.kg(found.expectedMaxKg)} ± ${format.percent(found.uncertainty)}, from ${count.format(found.meets)} ${found.meets === 1 ? "meet" : "meets"}`,
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
  for (const [name, value] of estimatedValues) setInput(name, value);
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
    for (const [name, value] of changes) setInput(name, value);
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
