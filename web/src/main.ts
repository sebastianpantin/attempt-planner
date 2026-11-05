import type {
  ChangesResponse,
  EvaluateRequest,
  Evaluation,
  Lift,
  MaxEstimate,
  Percentiles,
  Problem,
  TradeoffPoint,
} from "../pkg/attempt_sim_wasm.js";
import {
  aboveMaxWarnings,
  defaultShapes,
  isPlanField,
  isUsableMax,
  PLAN_FIELDS,
  regroupedShapes,
  scaledPlan,
  shapeOf,
} from "./attempt-scaling.ts";
import { describePoint, drawTradeoff } from "./chart.ts";
import { element } from "./dom.ts";
import type { Failed } from "./engine.ts";
import {
  CALIBRATIONS,
  describeKept,
  EXAMPLE,
  EXAMPLE_LIFTER_NAMES,
  type Sex,
  START,
  switchCalibration,
} from "./example.ts";
import * as format from "./format.ts";
import { LIFT_NAMES } from "./format.ts";
import {
  buildRequest,
  buildSearchRequest,
  buildTradeoffRequest,
  type Swept,
  sweepKey,
  type FormValues,
  INPUT_NAMES,
  type InputName,
  inputName,
  type LiftField,
  LIFTS,
  parseInputName,
  planValues,
  sweepOnlyProblems,
} from "./inputs.ts";
import {
  bestMade,
  buildEstimateRequest,
  estimateLine,
  estimateValues,
  parsePastMeets,
  today,
} from "./past-meets.ts";
import {
  averageLine,
  barFigures,
  outcomeSentence,
  rangeLabels,
  rangeLayout,
  ringFigure,
  riskWarning,
  totalKg,
} from "./outcome.ts";
import {
  BOMB_OUT_CAPS,
  bombOutCapLabel,
  bombOutCapPercent,
  jumpLabels,
  ladder,
  sliderPosition,
  valueText,
} from "./plan-view.ts";
import {
  describeProblem,
  FIRST_MEET_CAP_PERCENT,
  firstMeetPrompt,
  inputsFor,
  maxesPrompt,
  problemListId,
  shownProblems,
} from "./problems.ts";
import { isTerm, termHelp } from "./terms.ts";
import { renderAnalytics } from "./analytics/analytics.ts";
import { startSearch } from "./search-runner.ts";
import { describeSearch } from "./search-view.ts";
import { simulationWorker } from "./simulation.ts";
import {
  bestForGoalLabel,
  comparisonRows,
  noGoalPickNote,
  planLines,
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
 * step the input. A jump shows signed, "+12.5" or "repeat", while it is not being edited,
 * and a screen reader hears the same in words.
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
        <span class="over" id="${lift}-over"></span>
        <span class="pill" id="${lift}-make"></span>
      </summary>
      <ul class="warnings" id="${lift}-warnings"></ul>
      <div class="ladder">
        <div class="attempt opener">
          <div class="with-term"><p class="n">Opener</p>${termHelp("opener")}</div>
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
  <div>
    <label class="field">
      <span>${LIFT_NAMES[lift].full}<span class="visually-hidden"> expected max</span></span>
      <span class="input big">
        <input name="${inputName(lift, "expectedMaxKg")}" type="number" min="20" step="2.5"
          aria-describedby="maxes-hint" data-describedby="maxes-hint" />
        <span aria-hidden="true">kg</span>
      </span>
    </label>
    <ul class="problems" id="${lift}-max-problems"></ul>
  </div>`;

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

/** The cap slider takes the position of the cap. */
function setInput(name: InputName, value: number) {
  input(name).valueAsNumber =
    name === "bombOutCapPercent" ? sliderPosition(value) : value;
}

for (const placeholder of document.querySelectorAll<HTMLElement>(
  "[data-term]",
)) {
  const term = placeholder.dataset.term!;
  if (isTerm(term)) placeholder.outerHTML = termHelp(term);
}

for (const name of INPUT_NAMES) setInput(name, START[name]);

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
    const maxKg = values[inputName(lift, "expectedMaxKg")];
    const view = ladder(plans[lift], maxKg);
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
    for (const field of PLAN_FIELDS) {
      const name = inputName(lift, field);
      input(name).setAttribute(
        "aria-valuetext",
        valueText(field, values[name]),
      );
    }
    const warnings = aboveMaxWarnings(plans[lift], maxKg);
    byId(`${lift}-warnings`).replaceChildren(
      ...warnings.map((warning) => element("li", "", warning)),
    );
    byId(`${lift}-over`).textContent =
      warnings.length > 0 ? "Attempt above your max" : "";
  }
}

function headedRow(...heading: (Node | string)[]) {
  const row = element("tr");
  const cell = element("th");
  cell.scope = "row";
  cell.append(...heading);
  row.append(cell);
  return row;
}

const setOrRemove = (target: Element, name: string, value: string) =>
  value ? target.setAttribute(name, value) : target.removeAttribute(name);

/** Marks the inputs with problems, each described by its own hint (`data-describedby`) and its problems. */
function markProblems(problems: Problem[], values: FormValues) {
  for (const list of form.querySelectorAll("ul.problems")) {
    list.replaceChildren();
  }
  const problemIds = new Map<InputName, string[]>();
  for (const [index, problem] of problems.entries()) {
    const item = element("li", "", describeProblem(problem));
    item.id = `problem-${index}`;
    for (const name of inputsFor(problem, values)) {
      problemIds.set(name, [...(problemIds.get(name) ?? []), item.id]);
      const collapsed = input(name).closest("details");
      if (collapsed) collapsed.open = true;
    }
    byId(problemListId(problem)).append(item);
  }
  for (const name of INPUT_NAMES) {
    const marked = input(name);
    const ids = problemIds.get(name) ?? [];
    setOrRemove(marked, "aria-invalid", ids.length > 0 ? "true" : "");
    setOrRemove(
      marked,
      "aria-describedby",
      [marked.dataset["describedby"] ?? [], ids].flat().join(" "),
    );
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
  byId("typical-total").textContent = totalKg(
    evaluation.totalPercentilesKg.p50,
  );
  byId("bomb-out").textContent = format.percent(evaluation.bombOutProbability);
  byId("risk-warning").textContent = riskWarning(evaluation.bombOutProbability);
  byId("average-line").textContent = averageLine(
    evaluation.expectedTotalKg,
    evaluation.expectedTotalWithoutBombOutKg,
  );
  showRange(evaluation.totalPercentilesKg, goalKg);
  byId("summary-bar-figures").replaceChildren(
    ...barFigures(evaluation, goalKg).map(([value, meaning]) => {
      const figure = element("span");
      figure.append(strong(value), ` ${meaning}`);
      return figure;
    }),
  );
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
      const row = headedRow(LIFT_NAMES[lift].full);
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
  const labels = rangeLabels(totals);
  byId("range-low").textContent = labels.low;
  byId("range-median-label").textContent = labels.median;
  byId("range-high").textContent = labels.high;
  byId("range-note").textContent = labels.note;
}

const status = (id: string, text: string) => {
  byId(id).textContent = text;
};

let shown:
  | (Pick<EvaluateRequest, "goalKg" | "plans"> & { evaluation: Evaluation })
  | undefined;
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
/** The values last simulated, which the problems shown belong to. */
let simulatedValues = START;
const showProblems = () => {
  const shownList = shownProblems(
    [...problems.evaluation, ...problems.sweep],
    simulatedValues,
  );
  markProblems(shownList, simulatedValues);
  byId("to-problem").hidden = shownList.length === 0;
};
let shownTradeoff: Swept | undefined;

const sex = () =>
  (form.elements.namedItem("sex") as RadioNodeList).value as Sex;
let calibratedSex: Sex = "any";

/** Each lift's plan as shares of its max, which the attempts follow when the max changes. */
const shapes = defaultShapes(CALIBRATIONS[calibratedSex].typicalOpener);
/** Lifts whose attempts were edited by hand since they last followed the max. */
const handEdited = new Set<Lift>();

/** Takes the attempts as they are now as the ones to follow the maxes. */
function rememberShapes() {
  const values = readValues();
  for (const lift of LIFTS) {
    const shape = shapeOf(values, lift);
    if (!shape) continue;
    shapes[lift] = shape;
    handEdited.delete(lift);
  }
}

/** Rescales a lift's attempts to its max, unless they were edited by hand. */
function followMax(lift: Lift) {
  const maxKg = input(inputName(lift, "expectedMaxKg")).valueAsNumber;
  if (handEdited.has(lift) || !isUsableMax(maxKg)) return;
  for (const [name, kg] of scaledPlan(shapes[lift], lift, maxKg)) {
    setInput(name, kg);
  }
}

function usePlans(plans: TradeoffPoint["plans"]) {
  for (const [name, kg] of planValues(plans)) {
    setInput(name, kg);
  }
  rememberShapes();
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
  byId("tradeoff-colour-key").hidden = goalKg === undefined;
  byId("tradeoff-note").textContent = noGoalPickNote(
    goalKg,
    bombOutCap,
    bestForGoal,
  );
  const bestLabel = bestForGoalLabel(bombOutCap);
  byId("tradeoff-best-label").textContent = bestLabel;
  byId("tradeoff-plans").replaceChildren(
    ...comparisonRows(points, bestForGoal).map(
      ({ point, label, bestForGoal }) => {
        const title = element("span", "plan-label", label);
        if (bestForGoal) title.append(" ", element("span", "badge", bestLabel));
        const row = headedRow(
          title,
          ...[shapeLabel(point.shape), ...planLines(point.plans)].flatMap(
            (text) => [" ", element("span", "plan-detail", text)],
          ),
        );
        for (const [heading, text] of [
          ["Bomb-out", format.percent(point.bombOutProbability)],
          ["Average total", format.kg(point.expectedTotalKg)],
          [
            "Chance of goal",
            point.goalProbability === undefined
              ? "–"
              : format.percent(point.goalProbability),
          ],
        ] as const) {
          const cell = row.insertCell();
          cell.dataset["label"] = heading;
          cell.textContent = text;
        }
        const use = document.createElement("button");
        use.type = "button";
        use.textContent = "Use";
        use.setAttribute("aria-label", `Use ${label}: ${describePoint(point)}`);
        use.addEventListener("click", () => usePlans(point.plans));
        row.insertCell().append(use);
        return row;
      },
    ),
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
        ? {
            evaluation: reply.evaluation,
            goalKg: request.goalKg,
            plans: request.plans,
          }
        : undefined;
    showEvaluation(shown?.evaluation, request);
    showAnalytics();
    showTradeoff();
    report(
      "status",
      reply,
      () => `${count.format(request.simulations)} simulated meets.`,
      maxesPrompt(simulatedValues) ||
        "Results paused: fix the marked inputs to see them again.",
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
  simulatedValues = values;
  submitEvaluation(buildRequest(values, input("precise").checked));
  shownChanges = undefined;
  showAnalytics();
  submitChanges(buildSearchRequest(values));
  const sweep = buildTradeoffRequest(values);
  const key = sweepKey(sweep);
  if (key !== sweptKey) {
    sweptKey = key;
    byId("tradeoff").setAttribute("aria-busy", "true");
    submitTradeoff(sweep);
  }
}

/** The search waits for every max, so it shows as soon as they are in. */
const showSearchArea = (values: FormValues) => {
  byId("search").hidden = maxesPrompt(values) !== "";
};

let typingPause: ReturnType<typeof setTimeout> | undefined;
function update() {
  const values = readValues();
  showBombOutCap();
  showPlans(values);
  showSearchArea(values);
  byId("results").setAttribute("aria-busy", "true");
  status("status", "Updating…");
  clearTimeout(typingPause);
  typingPause = setTimeout(() => simulate(values), TYPING_PAUSE_MS);
}

const findButton = byId("find-plan") as HTMLButtonElement;
const firstMeetButton = byId("first-meet") as HTMLButtonElement;
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
  findButton.disabled =
    barFindButton.disabled =
    firstMeetButton.disabled =
      searching;
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
  const request = buildSearchRequest(readValues(), shownTradeoff);
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

/** A first meet asks for a plan that hardly ever bombs out, searched at once when the maxes are in. */
firstMeetButton.addEventListener("click", () => {
  noBombOutCap.checked = false;
  setInput("bombOutCapPercent", FIRST_MEET_CAP_PERCENT);
  update();
  const prompt = firstMeetPrompt(readValues());
  status("first-meet-status", prompt);
  if (!prompt) search(cancelButton);
});

byId("try-example").addEventListener("click", () => {
  for (const name of EXAMPLE_LIFTER_NAMES) setInput(name, EXAMPLE[name]);
  noBombOutCap.checked = false;
  rememberShapes();
  status(
    "example-status",
    "Showing an example lifter. Enter your own maxes and goal to plan your meet.",
  );
  update();
});

byId("to-problem").addEventListener("click", () => {
  form.querySelector<HTMLInputElement>('[aria-invalid="true"]')?.focus();
});

const howItWorks = byId("how") as HTMLDialogElement;
byId("how-it-works").addEventListener("click", () => howItWorks.showModal());

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

const pastMeets = byId("past-meets") as HTMLTextAreaElement;
const estimateButton = byId("estimate-maxes") as HTMLButtonElement;
const useEstimateButton = byId("use-estimate") as HTMLButtonElement;
let estimatedValues: [InputName, number][] = [];
let skippedMeets = 0;
let bestMadeKg = bestMade([]);

function showEstimate(estimate: MaxEstimate | undefined) {
  estimatedValues = estimate ? estimateValues(estimate) : [];
  useEstimateButton.hidden = estimatedValues.length === 0;
  byId("estimate-results").replaceChildren(
    ...(estimate
      ? LIFTS.flatMap((lift) => {
          const found = estimate.lifts[lift];
          return found
            ? [element("li", "", estimateLine(lift, found, bestMadeKg[lift]))]
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
        ? ` Skipped ${count.format(skippedMeets)} ${skippedMeets === 1 ? "line that was" : "lines that were"} not a raw meet with attempts.`
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
  bestMadeKg = bestMade(meets);
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
  for (const lift of LIFTS) followMax(lift);
  update();
});

form.addEventListener("input", (event) => {
  const { name } = event.target as HTMLInputElement;
  if (!CONTROL_NAMES.has(name)) return;
  status("example-status", "");
  status("first-meet-status", "");
  const liftInput = parseInputName(name);
  if (liftInput && isPlanField(liftInput.field)) handEdited.add(liftInput.lift);
  if (liftInput?.field === "expectedMaxKg") followMax(liftInput.lift);
  if (name === "sex") {
    const chosen = sex();
    const { changes, kept } = switchCalibration(
      readValues(),
      calibratedSex,
      chosen,
    );
    for (const [lift, shape] of regroupedShapes(
      shapes,
      CALIBRATIONS[calibratedSex].typicalOpener,
      CALIBRATIONS[chosen].typicalOpener,
    )) {
      if (handEdited.has(lift)) continue;
      shapes[lift] = shape;
      followMax(lift);
    }
    calibratedSex = chosen;
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
showSearchArea(initial);
simulate(initial);
