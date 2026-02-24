import type { ComparePathsResult } from "../types.js";

type RunFn = (smql: string) => Promise<ComparePathsResult>;

export class ComparePathsBuilder {
  private machine: string;
  private segmentField?: string;
  private filterExpr?: string;
  private run: RunFn;

  constructor(machine: string, run: RunFn) {
    this.machine = machine;
    this.run = run;
  }

  segmentBy(field: string): this {
    this.segmentField = field;
    return this;
  }

  where(expr: string): this {
    this.filterExpr = expr;
    return this;
  }

  toSmql(): string {
    let s = `COMPARE PATHS ${this.machine}`;

    if (this.segmentField) {
      s += ` SEGMENT BY ${this.segmentField}`;
    }

    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }

    return s;
  }

  execute(): Promise<ComparePathsResult> {
    return this.run(this.toSmql());
  }
}
