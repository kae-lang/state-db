import { describe, it, expect } from "vitest";
import { valueToSmql, escapeString } from "../src/builder/value.js";

describe("valueToSmql", () => {
  it("converts null", () => {
    expect(valueToSmql(null)).toBe("NULL");
  });

  it("converts booleans", () => {
    expect(valueToSmql(true)).toBe("true");
    expect(valueToSmql(false)).toBe("false");
  });

  it("converts integers", () => {
    expect(valueToSmql(42)).toBe("42");
  });

  it("converts floats", () => {
    expect(valueToSmql(3.125)).toBe("3.125");
  });

  it("converts strings", () => {
    expect(valueToSmql("hello")).toBe('"hello"');
  });

  it("converts arrays", () => {
    expect(valueToSmql([1, 2, 3])).toBe("[1, 2, 3]");
  });

  it("converts empty array", () => {
    expect(valueToSmql([])).toBe("[]");
  });

  it("converts objects", () => {
    expect(valueToSmql({ name: "Alice" })).toBe('{name: "Alice"}');
  });

  it("converts empty object", () => {
    expect(valueToSmql({})).toBe("{}");
  });

  it("converts nested objects", () => {
    expect(valueToSmql({ outer: { inner: 42 } })).toBe(
      "{outer: {inner: 42}}",
    );
  });

  it("converts strings with quotes", () => {
    expect(valueToSmql('say "hello"')).toBe('"say \\"hello\\""');
  });
});

describe("escapeString", () => {
  it("escapes quotes", () => {
    expect(escapeString('hello "world"')).toBe('hello \\"world\\"');
  });

  it("escapes backslashes before quotes", () => {
    expect(escapeString("path\\to\\file")).toBe("path\\\\to\\\\file");
    expect(escapeString('back\\and"quote')).toBe('back\\\\and\\"quote');
  });
});
