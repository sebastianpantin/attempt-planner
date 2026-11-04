/**
 * Runs one request at a time. Requests that arrive meanwhile replace each other,
 * so only the newest one runs next.
 */
export class LatestOnly<T> {
  #running: { request: T } | undefined;
  #waiting: { request: T } | undefined;

  constructor(private readonly send: (request: T) => void) {}

  submit(request: T): void {
    if (this.#running) {
      this.#waiting = { request };
      return;
    }
    this.#start({ request });
  }

  /**
   * Call when the running request finishes. Returns that request, or `undefined`
   * when a newer one has started in its place.
   */
  finish(): T | undefined {
    const finished = this.#running;
    const next = this.#waiting;
    this.#waiting = undefined;
    this.#running = undefined;
    if (next) {
      this.#start(next);
      return undefined;
    }
    return finished?.request;
  }

  #start(running: { request: T }) {
    this.#running = running;
    this.send(running.request);
  }
}
