import type {
  EvaluateRequest,
  Evaluation,
  Lift,
  Problem,
} from "../pkg/attempt_sim_wasm.js";
import { type AttemptNode, attemptTree } from "./attempt-tree.ts";
import type { Reply } from "./engine.ts";
import { EXAMPLE } from "./example.ts";
import * as format from "./format.ts";
import {
  buildRequest,
  describeProblem,
  type FormValues,
  INPUT_NAMES,
  type InputName,
  inputName,
  inputsFor,
  type LiftField,
  LIFTS,
} from "./inputs.ts";
import { LatestOnly } from "./latest-only.ts";

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
    </tr>`;
}

byId("lifts").innerHTML = LIFTS.map(liftFieldset).join("");
byId("curves").innerHTML = `
  <table>
    <thead><tr><td></td><th scope="col">Midpoint</th><th scope="col">Steepness</th></tr></thead>
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

function treeItem(node: AttemptNode, attempt = 0, outcome = ""): HTMLLIElement {
  const item = listItem(
    `${outcome}${ORDINALS[attempt]} ${format.kg(node.weightKg)}`,
  );
  const branches = [
    ["after a make: ", node.afterMake],
    ["after a miss: ", node.afterMiss],
  ] as const;
  const children = branches.flatMap(([label, child]) =>
    child ? [treeItem(child, attempt + 1, label)] : [],
  );
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

function showEvaluation(evaluation: Evaluation | undefined) {
  byId("evaluation").hidden = !evaluation;
  if (!evaluation) return;
  const { standardErrors: errors, totalPercentilesKg: totals } = evaluation;
  const goalRows: [string, string][] =
    evaluation.goalProbability === undefined
      ? []
      : [
          [
            "Chance of reaching the goal",
            format.percentWithError(
              evaluation.goalProbability,
              errors.goalProbability ?? 0,
            ),
          ],
        ];
  const rows: [string, string][] = [
    [
      "Expected total, counting a bomb-out as 0",
      format.kgWithError(evaluation.expectedTotalKg, errors.expectedTotalKg),
    ],
    ...goalRows,
    [
      "Chance of bombing out",
      format.percentWithError(
        evaluation.bombOutProbability,
        errors.bombOutProbability,
      ),
    ],
    [
      "Expected total without a bomb-out",
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
  byId("lift-results").replaceChildren(
    ...LIFTS.map((lift) => {
      const { makeProbability, expectedBestKg } = evaluation.lifts[lift];
      const row = document.createElement("tr");
      const heading = document.createElement("th");
      heading.scope = "row";
      heading.textContent = LIFT_NAMES[lift];
      row.append(heading);
      row.insertCell().textContent = format.percent(makeProbability);
      row.insertCell().textContent =
        expectedBestKg === undefined ? "–" : format.kg(expectedBestKg);
      return row;
    }),
  );
}

const status = (text: string) => {
  byId("status").textContent = text;
};

function show(reply: Reply, simulations: number, elapsedMs: number) {
  byId("results").removeAttribute("aria-busy");
  markProblems(reply.kind === "invalid" ? reply.problems : []);
  showEvaluation(reply.kind === "evaluated" ? reply.evaluation : undefined);
  switch (reply.kind) {
    case "evaluated":
      status(
        `${count.format(simulations)} simulated meets in ${count.format(Math.round(elapsedMs))} ms.`,
      );
      break;
    case "invalid":
      status("Fix the marked inputs to see results.");
      break;
    case "failed":
      status(`The simulation failed: ${reply.message}`);
      break;
  }
}

const worker = new Worker(new URL("./worker.ts", import.meta.url), {
  type: "module",
});
let startedAt = 0;
const runs = new LatestOnly<EvaluateRequest>((request) => {
  startedAt = performance.now();
  status(`Simulating ${count.format(request.simulations)} meets…`);
  worker.postMessage(request);
});

function workerFailed(message: string) {
  const finished = runs.finish();
  if (finished) show({ kind: "failed", message }, finished.simulations, 0);
}

worker.addEventListener("error", (event) => {
  event.preventDefault();
  workerFailed(event.message || "the simulation worker could not start");
});
worker.addEventListener("messageerror", () =>
  workerFailed("the simulation result could not be read"),
);

worker.addEventListener("message", (event: MessageEvent<Reply>) => {
  const elapsedMs = performance.now() - startedAt;
  const finished = runs.finish();
  if (finished) show(event.data, finished.simulations, elapsedMs);
});

const currentRequest = () =>
  buildRequest(readValues(), input("precise").checked);

let typingPause: ReturnType<typeof setTimeout> | undefined;
form.addEventListener("input", () => {
  const request = currentRequest();
  showTrees(request);
  byId("results").setAttribute("aria-busy", "true");
  status("Updating…");
  clearTimeout(typingPause);
  typingPause = setTimeout(() => runs.submit(request), TYPING_PAUSE_MS);
});
form.addEventListener("submit", (event) => event.preventDefault());

const initial = currentRequest();
showTrees(initial);
runs.submit(initial);
