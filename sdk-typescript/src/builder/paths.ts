import type { PathsResult } from "../types.js";

type RunFn = (smql: string) => Promise<PathsResult>;

export class PathsBuilder {
  private machine: string;
  private filterExpr?: string;
  private limitVal?: number;
  private run: RunFn;

  constructor(machine: string, run: RunFn) {
    this.machine = machine;
    this.run = run;
  }

  where(expr: string): this {
    this.filterExpr = expr;
    return this;
  }

  limit(n: number): this {
    this.limitVal = n;
    return this;
  }

  toSmql(): string {
    let s = `PATHS FROM ${this.machine}`;

    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }

    if (this.limitVal !== undefined) {
      s += ` LIMIT ${this.limitVal}`;
    }

    return s;
  }

  execute(): Promise<PathsResult> {
    return this.run(this.toSmql());
  }
}
