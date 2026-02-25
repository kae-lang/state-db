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

  // --- Query predicates ---

  static alive(): Expr {
    return new Expr("ALIVE");
  }

  static terminated(): Expr {
    return new Expr("TERMINATED");
  }

  static stuckIn(state: string, duration: string): Expr {
    return new Expr(`STUCK_IN("${state}", ${duration})`);
  }

  static hasVisited(state: string): Expr {
    return new Expr(`HAS_VISITED("${state}")`);
  }

  static neverVisited(state: string): Expr {
    return new Expr(`NEVER_VISITED("${state}")`);
  }

  static tag(key: string, value: string): Expr {
    return new Expr(`TAG "${key}" == "${value}"`);
  }

  // --- Composition predicates ---

  static parentState(): Expr {
    return new Expr("PARENT.STATE");
  }

  static parentField(field: string): Expr {
    return new Expr(`PARENT.${field}`);
  }

  static signalFrom(machine: string, condition: string): Expr {
    return new Expr(`SIGNAL FROM ${machine} WHERE ${condition}`);
  }

  static all(collection: string, predicate: string): Expr {
    return new Expr(`ALL(${collection}, ${predicate})`);
  }

  static any(collection: string, predicate: string): Expr {
    return new Expr(`ANY(${collection}, ${predicate})`);
  }

  static countOf(collection: string): Expr {
    return new Expr(`COUNT(${collection})`);
  }

  // --- Built-in functions ---

  static elapsed(): Expr {
    return new Expr("elapsed()");
  }

  static elapsedSince(state: string): Expr {
    return new Expr(`elapsed_since("${state}")`);
  }

  static now(): Expr {
    return new Expr("NOW()");
  }

  static today(): Expr {
    return new Expr("TODAY()");
  }

  static timeoutRemaining(): Expr {
    return new Expr("timeout_remaining()");
  }

  static len(expr: string): Expr {
    return new Expr(`len(${expr})`);
  }

  static lower(expr: string): Expr {
    return new Expr(`lower(${expr})`);
  }

  static upper(expr: string): Expr {
    return new Expr(`upper(${expr})`);
  }

  static pattern(regex: string): Expr {
    return new Expr(`PATTERN("${regex}")`);
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

  // --- Arithmetic ---

  add(other: Expr | SmqlValue): Expr {
    return new Expr(`${this.expr} + ${toExprStr(other)}`);
  }

  sub(other: Expr | SmqlValue): Expr {
    return new Expr(`${this.expr} - ${toExprStr(other)}`);
  }

  mul(other: Expr | SmqlValue): Expr {
    return new Expr(`${this.expr} * ${toExprStr(other)}`);
  }

  div(other: Expr | SmqlValue): Expr {
    return new Expr(`${this.expr} / ${toExprStr(other)}`);
  }

  // --- Field access ---

  dot(field: string): Expr {
    return new Expr(`${this.expr}.${field}`);
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
