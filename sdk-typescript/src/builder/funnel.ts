import type { FunnelResult } from "../types.js";

type RunFn = (smql: string) => Promise<FunnelResult>;

export class FunnelBuilder {
  private machine: string;
  private states: string[] = [];
  private filterExpr?: string;
  private run: RunFn;

  constructor(machine: string, run: RunFn) {
    this.machine = machine;
    this.run = run;
  }

  through(states: string[]): this {
    this.states = states;
    return this;
  }

  where(expr: string): this {
    this.filterExpr = expr;
    return this;
  }

  toSmql(): string {
    let s = `FUNNEL ${this.machine} THROUGH [${this.states.join(", ")}]`;

    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }

    return s;
  }

  execute(): Promise<FunnelResult> {
    return this.run(this.toSmql());
  }
}
