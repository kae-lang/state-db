import type { AlterMachineResult, DataType, Constraint } from "../types.js";

type RunFn = (smql: string) => Promise<AlterMachineResult>;

interface AlterOp {
  toSmql(): string;
}

export class AlterMachineBuilder {
  private machine: string;
  private operations: AlterOp[] = [];
  private run: RunFn;

  constructor(machine: string, run: RunFn) {
    this.machine = machine;
    this.run = run;
  }

  addState(name: string): this {
    this.operations.push({ toSmql: () => `ADD STATE ${name}` });
    return this;
  }

  removeState(state: string, migrateTo: string): this {
    this.operations.push({
      toSmql: () => `REMOVE STATE ${state} MIGRATE TO ${migrateTo}`,
    });
    return this;
  }

  addTransition(from: string, to: string): this {
    this.operations.push({
      toSmql: () => `ADD TRANSITION ${from} -> ${to}`,
    });
    return this;
  }

  removeTransition(from: string, to: string): this {
    this.operations.push({
      toSmql: () => `REMOVE TRANSITION ${from} -> ${to}`,
    });
    return this;
  }

  addData(field: string, type: DataType, constraints?: Constraint[], backfillValue?: string): this {
    this.operations.push({
      toSmql: () => {
        let s = `ADD DATA ${field} : ${formatDataType(type)}`;
        if (constraints && constraints.length > 0) {
          s += ` -> ${constraints.map(formatConstraint).join(", ")}`;
        }
        if (backfillValue) {
          s += ` BACKFILL ${backfillValue}`;
        }
        return s;
      },
    });
    return this;
  }

  removeData(field: string): this {
    this.operations.push({ toSmql: () => `REMOVE DATA ${field}` });
    return this;
  }

  backfill(field: string, expr: string): this {
    this.operations.push({
      toSmql: () => `BACKFILL ${field} = ${expr}`,
    });
    return this;
  }

  toSmql(): string {
    const ops = this.operations.map((o) => o.toSmql()).join(" ");
    return `ALTER MACHINE ${this.machine} ${ops}`;
  }

  execute(): Promise<AlterMachineResult> {
    return this.run(this.toSmql());
  }
}

function formatDataType(t: DataType): string {
  if (typeof t === "string") return t;
  switch (t.type) {
    case "ENUM":
      return `ENUM(${t.variants.join(", ")})`;
    case "REF":
      return `REF(${t.target})`;
    case "LIST":
      return `LIST(${formatDataType(t.inner)})`;
    case "SET":
      return `SET(${formatDataType(t.inner)})`;
    case "MAP":
      return `MAP(${formatDataType(t.key)}, ${formatDataType(t.value)})`;
    case "MONEY":
      return `MONEY(${t.currency})`;
  }
}

function formatConstraint(c: Constraint): string {
  if (typeof c === "string") return c;
  switch (c.type) {
    case "MAX":
      return `MAX(${c.value})`;
    case "MIN":
      return `MIN(${c.value})`;
    case "RANGE":
      return `RANGE(${c.lo}, ${c.hi})`;
    case "DEFAULT":
      if (c.value === null) return "DEFAULT(NULL)";
      if (typeof c.value === "string") return `DEFAULT("${c.value}")`;
      return `DEFAULT(${c.value})`;
    case "PATTERN":
      return `PATTERN("${c.regex}")`;
    case "COMPUTED":
      return `COMPUTED(${c.expr})`;
  }
}
