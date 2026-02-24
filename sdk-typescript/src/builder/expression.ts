import { valueToSmql } from "./value.js";
import type { SmqlValue } from "../types.js";

export class Expr {
  private readonly expr: string;

  private constructor(expr: string) {
    this.expr = expr;
  }

  toString(): string {
    return this.expr;
  }

  // --- Factories ---

  static field(name: string): Expr {
    return new Expr(name);
  }

  static val(v: SmqlValue): Expr {
    return new Expr(valueToSmql(v));
  }

  static stateIs(state: string): Expr {
    return new Expr(`STATE IS ${state}`);
  }

  static stateIn(...states: string[]): Expr {
    return new Expr(`STATE IN { ${states.join(", ")} }`);
  }

  static isSet(field: string): Expr {
    return new Expr(`${field} IS SET`);
  }

  static isNotSet(field: string): Expr {
    return new Expr(`${field} IS NOT SET`);
  }

  static raw(expr: string): Expr {
    return new Expr(expr);
  }

  // --- Comparisons ---

  eq(other: Expr | SmqlValue): Expr {
    return new Expr(`${this.expr} == ${toExprStr(other)}`);
  }

  neq(other: Expr | SmqlValue): Expr {
    return new Expr(`${this.expr} != ${toExprStr(other)}`);
  }

  gt(other: Expr | SmqlValue): Expr {
    return new Expr(`${this.expr} > ${toExprStr(other)}`);
  }

  gte(other: Expr | SmqlValue): Expr {
    return new Expr(`${this.expr} >= ${toExprStr(other)}`);
  }

  lt(other: Expr | SmqlValue): Expr {
    return new Expr(`${this.expr} < ${toExprStr(other)}`);
  }

  lte(other: Expr | SmqlValue): Expr {
    return new Expr(`${this.expr} <= ${toExprStr(other)}`);
  }

  // --- Logical ---

  and(other: Expr): Expr {
    return new Expr(`(${this.expr}) AND (${other.expr})`);
  }

  or(other: Expr): Expr {
    return new Expr(`(${this.expr}) OR (${other.expr})`);
  }

  not(): Expr {
    return new Expr(`NOT (${this.expr})`);
  }

  // --- Set membership ---

  in(...values: SmqlValue[]): Expr {
    const items = values.map(valueToSmql);
    return new Expr(`${this.expr} IN { ${items.join(", ")} }`);
  }
}

function toExprStr(v: Expr | SmqlValue): string {
  if (v instanceof Expr) return v.toString();
  return valueToSmql(v);
}
