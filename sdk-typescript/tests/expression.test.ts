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
});
