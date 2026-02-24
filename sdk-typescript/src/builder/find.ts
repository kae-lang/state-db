import type { FindResult, Instance, SortDirection } from "../types.js";
import { escapeString } from "./value.js";

type RunFn = (smql: string) => Promise<FindResult>;

export class FindBuilder {
  private machine: string;
  private filterExpr?: string;
  private sorts: { field: string; direction: SortDirection }[] = [];
  private limitVal?: number;
  private offsetVal?: number;
  private afterId?: string;
  private actorRole?: string;
  private run: RunFn;

  constructor(machine: string, run: RunFn) {
    this.machine = machine;
    this.run = run;
  }

  where(expr: string): this {
    this.filterExpr = expr;
    return this;
  }

  inState(state: string): this {
    this.filterExpr = `STATE IS ${state}`;
    return this;
  }

  stuckIn(state: string, duration: string): this {
    this.filterExpr = `STUCK IN ${state} FOR ${duration}`;
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

  after(id: string): this {
    this.afterId = id;
    return this;
  }

  asActor(role: string): this {
    this.actorRole = role;
    return this;
  }

  toSmql(): string {
    let s = `FIND ${this.machine}`;

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

    if (this.afterId !== undefined) {
      s += ` AFTER "${escapeString(this.afterId)}"`;
    }

    if (this.actorRole) {
      s += ` AS ACTOR ${this.actorRole}`;
    }

    return s;
  }

  execute(): Promise<FindResult> {
    return this.run(this.toSmql());
  }

  async first(): Promise<Instance | null> {
    const prev = this.limitVal;
    this.limitVal = 1;
    const result = await this.run(this.toSmql());
    this.limitVal = prev;
    return result.instances[0] ?? null;
  }

  async count(): Promise<number> {
    const result = await this.run(this.toSmql());
    return result.count;
  }
}
