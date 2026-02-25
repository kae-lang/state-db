import type { ExplainTransitionsResult } from "../types.js";
import { escapeString } from "./value.js";

type RunFn = (smql: string) => Promise<ExplainTransitionsResult>;

export class ExplainTransitionsBuilder {
  private machine: string;
  private instanceId?: string;
  private actor?: string;
  private run: RunFn;

  constructor(machine: string, run: RunFn) {
    this.machine = machine;
    this.run = run;
  }

  instance(id: string): this {
    this.instanceId = id;
    return this;
  }

  asActor(actor: string): this {
    this.actor = actor;
    return this;
  }

  toSmql(): string {
    let s = `EXPLAIN TRANSITIONS FOR ${this.machine}`;

    if (this.instanceId) {
      s += ` "${escapeString(this.instanceId)}"`;
    }

    if (this.actor) {
      s += ` AS ${this.actor}`;
    }

    return s;
  }

  execute(): Promise<ExplainTransitionsResult> {
    return this.run(this.toSmql());
  }
}
