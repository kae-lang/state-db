import { describe, it, expect } from "vitest";
import { Expr } from "../src/builder/expression.js";

describe("Expr", () => {
  it("creates field reference", () => {
    expect(Expr.field("name").toString()).toBe("name");
  });

  it("creates value literal", () => {
    expect(Expr.val("hello").toString()).toBe('"hello"');
    expect(Expr.val(42).toString()).toBe("42");
    expect(Expr.val(null).toString()).toBe("NULL");
    expect(Expr.val(true).toString()).toBe("true");
  });

  it("creates state predicates", () => {
    expect(Expr.stateIs("open").toString()).toBe("STATE IS open");
    expect(Expr.stateIn("open", "pending").toString()).toBe(
      "STATE IN { open, pending }",
    );
  });

  it("creates null checks", () => {
    expect(Expr.isSet("email").toString()).toBe("email IS SET");
    expect(Expr.isNotSet("phone").toString()).toBe("phone IS NOT SET");
  });

  it("creates comparisons", () => {
    expect(Expr.field("age").eq(18).toString()).toBe("age == 18");
    expect(Expr.field("age").neq(0).toString()).toBe("age != 0");
    expect(Expr.field("age").gt(18).toString()).toBe("age > 18");
    expect(Expr.field("age").gte(18).toString()).toBe("age >= 18");
    expect(Expr.field("age").lt(65).toString()).toBe("age < 65");
    expect(Expr.field("age").lte(65).toString()).toBe("age <= 65");
  });

  it("creates comparison with Expr values", () => {
    expect(
      Expr.field("a").eq(Expr.field("b")).toString(),
    ).toBe("a == b");
  });

  it("creates logical operators", () => {
    const a = Expr.field("age").gt(18);
    const b = Expr.field("verified").eq(true);
    expect(a.and(b).toString()).toBe(
      "(age > 18) AND (verified == true)",
    );
    expect(a.or(b).toString()).toBe(
      "(age > 18) OR (verified == true)",
    );
    expect(a.not().toString()).toBe("NOT (age > 18)");
  });

  it("creates set membership", () => {
    expect(
      Expr.field("status").in("open", "pending", "review").toString(),
    ).toBe('status IN { "open", "pending", "review" }');
  });

  it("creates raw expressions", () => {
    expect(Expr.raw("STUCK IN open FOR 24h").toString()).toBe(
      "STUCK IN open FOR 24h",
    );
  });

  // --- Query predicates ---

  it("creates alive predicate", () => {
    expect(Expr.alive().toString()).toBe("ALIVE");
  });

  it("creates terminated predicate", () => {
    expect(Expr.terminated().toString()).toBe("TERMINATED");
  });

  it("creates stuckIn predicate", () => {
    expect(Expr.stuckIn("open", "24h").toString()).toBe(
      'STUCK_IN("open", 24h)',
    );
  });

  it("creates hasVisited predicate", () => {
    expect(Expr.hasVisited("reviewed").toString()).toBe(
      'HAS_VISITED("reviewed")',
    );
  });

  it("creates neverVisited predicate", () => {
    expect(Expr.neverVisited("rejected").toString()).toBe(
      'NEVER_VISITED("rejected")',
    );
  });

  it("creates tag predicate", () => {
    expect(Expr.tag("env", "production").toString()).toBe(
      'TAG "env" == "production"',
    );
  });

  // --- Composition ---

  it("creates parent state", () => {
    expect(Expr.parentState().toString()).toBe("PARENT.STATE");
  });

  it("creates parent field", () => {
    expect(Expr.parentField("priority").toString()).toBe("PARENT.priority");
  });

  it("creates signalFrom", () => {
    expect(
      Expr.signalFrom("Child", "STATE IS done").toString(),
    ).toBe("SIGNAL FROM Child WHERE STATE IS done");
  });

  it("creates all predicate", () => {
    expect(
      Expr.all("items", "STATE IS shipped").toString(),
    ).toBe("ALL(items, STATE IS shipped)");
  });

  it("creates any predicate", () => {
    expect(
      Expr.any("items", "STATE IS failed").toString(),
    ).toBe("ANY(items, STATE IS failed)");
  });

  it("creates countOf", () => {
    expect(Expr.countOf("items").toString()).toBe("COUNT(items)");
  });

  // --- Built-in functions ---

  it("creates elapsed", () => {
    expect(Expr.elapsed().toString()).toBe("elapsed()");
  });

  it("creates elapsedSince", () => {
    expect(Expr.elapsedSince("pending").toString()).toBe(
      'elapsed_since("pending")',
    );
  });

  it("creates now", () => {
    expect(Expr.now().toString()).toBe("NOW()");
  });

  it("creates today", () => {
    expect(Expr.today().toString()).toBe("TODAY()");
  });

  it("creates timeoutRemaining", () => {
    expect(Expr.timeoutRemaining().toString()).toBe("timeout_remaining()");
  });

  it("creates len", () => {
    expect(Expr.len("tags").toString()).toBe("len(tags)");
  });

  it("creates lower", () => {
    expect(Expr.lower("name").toString()).toBe("lower(name)");
  });

  it("creates upper", () => {
    expect(Expr.upper("name").toString()).toBe("upper(name)");
  });

  it("creates pattern", () => {
    expect(Expr.pattern("^[A-Z]{3}$").toString()).toBe(
      'PATTERN("^[A-Z]{3}$")',
    );
  });

  // --- Arithmetic ---

  it("creates add", () => {
    expect(Expr.field("a").add(1).toString()).toBe("a + 1");
  });

  it("creates sub", () => {
    expect(Expr.field("a").sub(Expr.field("b")).toString()).toBe("a - b");
  });

  it("creates mul", () => {
    expect(Expr.field("price").mul(Expr.field("qty")).toString()).toBe(
      "price * qty",
    );
  });

  it("creates div", () => {
    expect(Expr.field("total").div(Expr.field("count")).toString()).toBe(
      "total / count",
    );
  });

  // --- Dot access ---

  it("creates dot access", () => {
    expect(Expr.field("address").dot("city").toString()).toBe("address.city");
  });

  it("chains dot access", () => {
    expect(
      Expr.field("order").dot("shipping").dot("country").toString(),
    ).toBe("order.shipping.country");
  });
});
