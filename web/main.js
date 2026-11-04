import init, {
  barKg,
  checkAttempts,
  plateStepKg,
} from "./pkg/attempt_sim_wasm.js";

await init();

const form = document.getElementById("attempts");
const verdict = document.getElementById("verdict");
const weightInputs = [...form.querySelectorAll('input[name="weight"]')];
const madeInputs = [...form.querySelectorAll('input[name="made"]')];

for (const input of weightInputs) {
  input.min = barKg();
  input.step = plateStepKg();
}

function check() {
  const weights = Float64Array.from(
    weightInputs,
    (input) => input.valueAsNumber,
  );
  if (weights.some(Number.isNaN)) {
    return "Enter a weight for every attempt.";
  }
  const made = Uint8Array.from(madeInputs, (input) => (input.checked ? 1 : 0));
  try {
    return checkAttempts(weights, made);
  } catch (error) {
    return error.message;
  }
}

function update() {
  const problem = check();
  verdict.textContent = problem ?? "These attempts are allowed.";
  verdict.classList.toggle("problem", problem !== undefined);
}

form.addEventListener("input", update);
update();
