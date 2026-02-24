import type { AggregateResult } from "../types.js";

type RunFn = (smql: string) => Promise<AggregateResult>;

interface MeasureClause {
  func: string;
  field?: string;
  alias?: string;
}

export class AggregateBuilder {
  private machine: string;
  private measures: MeasureClause[] = [];
  private filterExpr?: string;
  private groupByClauses: string[] = [];
  private run: RunFn;

  constructor(machine: string, run: RunFn) {
    this.machine = machine;
    this.run = run;
  }

  measure(func: string, field?: string, alias?: string): this {
    this.measures.push({ func, field, alias });
    return this;
  }

  count(alias?: string): this {
    return this.measure("COUNT", undefined, alias);
  }

  sum(field: string, alias?: string): this {
    return this.measure("SUM", field, alias);
  }

  avg(field: string, alias?: string): this {
    return this.measure("AVG", field, alias);
  }

  min(field: string, alias?: string): this {
    return this.measure("MIN", field, alias);
  }

  max(field: string, alias?: string): this {
    return this.measure("MAX", field, alias);
  }

  where(expr: string): this {
    this.filterExpr = expr;
    return this;
  }

  groupByState(): this {
    this.groupByClauses.push("STATE");
    return this;
  }

  groupBy(field: string): this {
    this.groupByClauses.push(field);
    return this;
  }

  toSmql(): string {
    let s = `AGGREGATE ${this.machine}`;

    if (this.measures.length > 0) {
      const parts = this.measures.map((m) => {
        let clause = `${m.func}(${m.field ?? ""})`;
        if (m.alias) clause += ` AS ${m.alias}`;
        return clause;
      });
      s += ` MEASURE ${parts.join(", ")}`;
    }

    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }

    if (this.groupByClauses.length > 0) {
      s += ` GROUP BY ${this.groupByClauses.join(", ")}`;
    }

    return s;
  }

  execute(): Promise<AggregateResult> {
    return this.run(this.toSmql());
  }
}
