import type { EventsResult } from "../types.js";
import { escapeString } from "./value.js";

type RunFn = (smql: string) => Promise<EventsResult>;

export class GetEventsBuilder {
  private machine?: string;
  private afterId?: string;
  private limitVal?: number;
  private run: RunFn;

  constructor(run: RunFn, machine?: string) {
    this.machine = machine;
    this.run = run;
  }

  after(id: string): this {
    this.afterId = id;
    return this;
  }

  limit(n: number): this {
    this.limitVal = n;
    return this;
  }

  toSmql(): string {
    let s = "GET EVENTS";

    if (this.machine) {
      s += ` ${this.machine}`;
    }

    if (this.afterId) {
      s += ` AFTER "${escapeString(this.afterId)}"`;
    }

    if (this.limitVal !== undefined) {
      s += ` LIMIT ${this.limitVal}`;
    }

    return s;
  }

  execute(): Promise<EventsResult> {
    return this.run(this.toSmql());
  }
}
