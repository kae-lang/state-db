import type { DefineResult } from "../types.js";

type RunFn = (smql: string) => Promise<DefineResult>;

interface MeasureClause {
  func: string;
  field?: string;
  alias?: string;
}

export class DefineProjectionBuilder {
  private name: string;
  private machineName?: string;
  private measures: MeasureClause[] = [];
  private filterExpr?: string;
  private groupByClauses: string[] = [];
  private refreshPolicy?: string;
  private run: RunFn;

  constructor(name: string, run: RunFn) {
    this.name = name;
    this.run = run;
  }

  aggregate(machine: string): this {
    this.machineName = machine;
    return this;
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

  refreshOnTransition(): this {
    this.refreshPolicy = "REFRESH ON TRANSITION";
    return this;
  }

  refreshOnInterval(seconds: number): this {
    this.refreshPolicy = `REFRESH ON INTERVAL ${seconds}`;
    return this;
  }

  refreshManual(): this {
    this.refreshPolicy = "REFRESH MANUAL";
    return this;
  }

  toSmql(): string {
    let s = `DEFINE PROJECTION ${this.name} AS AGGREGATE ${this.machineName}`;

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

    if (this.refreshPolicy) {
      s += ` ${this.refreshPolicy}`;
    }

    return s;
  }

  execute(): Promise<DefineResult> {
    return this.run(this.toSmql());
  }
}
