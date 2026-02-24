import type { DefineResult, SortDirection } from "../types.js";

type RunFn = (smql: string) => Promise<DefineResult>;

export class DefineViewBuilder {
  private name: string;
  private machineName?: string;
  private filterExpr?: string;
  private sorts: { field: string; direction: SortDirection }[] = [];
  private limitVal?: number;
  private offsetVal?: number;
  private run: RunFn;

  constructor(name: string, run: RunFn) {
    this.name = name;
    this.run = run;
  }

  find(machine: string): this {
    this.machineName = machine;
    return this;
  }

  where(expr: string): this {
    this.filterExpr = expr;
    return this;
  }

  sortBy(field: string, direction: SortDirection = "ASC"): this {
    this.sorts.push({ field, direction });
    return this;
  }

  limit(n: number): this {
    this.limitVal = n;
    return this;
  }

  offset(n: number): this {
    this.offsetVal = n;
    return this;
  }

  toSmql(): string {
    let s = `DEFINE VIEW ${this.name} AS FIND ${this.machineName}`;

    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }

    if (this.sorts.length > 0) {
      const clauses = this.sorts.map((c) => `${c.field} ${c.direction}`);
      s += ` SORT BY ${clauses.join(", ")}`;
    }

    if (this.limitVal !== undefined) {
      s += ` LIMIT ${this.limitVal}`;
    }

    if (this.offsetVal !== undefined) {
      s += ` OFFSET ${this.offsetVal}`;
    }

    return s;
  }

  execute(): Promise<DefineResult> {
    return this.run(this.toSmql());
  }
}
