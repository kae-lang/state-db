import { describe, it, expect } from "vitest";
import { SpawnBuilder } from "../src/builder/spawn.js";
import { TransitionBuilder } from "../src/builder/transition.js";
import { BatchTransitionBuilder } from "../src/builder/batch-transition.js";
import { GetBuilder } from "../src/builder/get.js";
import { FindBuilder } from "../src/builder/find.js";
import { AggregateBuilder } from "../src/builder/aggregate.js";
import { TrailBuilder } from "../src/builder/trail.js";
import { PathsBuilder } from "../src/builder/paths.js";
import { FunnelBuilder } from "../src/builder/funnel.js";
import { ComparePathsBuilder } from "../src/builder/compare-paths.js";
import { DefineMachineBuilder } from "../src/builder/define-machine.js";
import { DefinePolicyBuilder } from "../src/builder/define-policy.js";
import { DefineViewBuilder } from "../src/builder/define-view.js";
import { DefineProjectionBuilder } from "../src/builder/define-projection.js";
import { DefineRuleBuilder } from "../src/builder/define-rule.js";
import { DefineSubscriptionBuilder } from "../src/builder/define-subscription.js";
import { DefineSagaBuilder } from "../src/builder/define-saga.js";
import { AlterMachineBuilder } from "../src/builder/alter-machine.js";
import { ExplainTransitionsBuilder } from "../src/builder/explain-transitions.js";
import { GetEventsBuilder } from "../src/builder/get-events.js";

const noop = () => Promise.resolve(null as any);

describe("SpawnBuilder", () => {
  it("generates empty spawn", () => {
    const b = new SpawnBuilder("counter", noop);
    expect(b.toSmql()).toBe("SPAWN counter {}");
  });

  it("generates spawn with data", () => {
    const b = new SpawnBuilder("ticket", noop);
    b.set({ title: "Bug report" });
    const smql = b.toSmql();
    expect(smql).toContain("SPAWN ticket {");
    expect(smql).toContain('title: "Bug report"');
  });

  it("generates spawn with key-value set", () => {
    const b = new SpawnBuilder("ticket", noop);
    b.set("count", 42);
    expect(b.toSmql()).toContain("count: 42");
  });

  it("generates spawn with thenTransitionTo", () => {
    const b = new SpawnBuilder("ticket", noop);
    b.set({}).thenTransitionTo("active");
    expect(b.toSmql()).toBe("SPAWN ticket {} THEN TRANSITION TO active");
  });
});

describe("TransitionBuilder", () => {
  it("generates basic transition", () => {
    const b = new TransitionBuilder("Machine", "abc-123", "active", false, noop);
    expect(b.toSmql()).toBe('TRANSITION Machine "abc-123" TO active');
  });

  it("generates transition with data and memo", () => {
    const b = new TransitionBuilder("Machine", "id1", "next", false, noop);
    b.with({ count: 10 }).memo("bump count");
    const smql = b.toSmql();
    expect(smql).toContain("WITH { count: 10 }");
    expect(smql).toContain('MEMO "bump count"');
  });

  it("generates transition with actor", () => {
    const b = new TransitionBuilder("Machine", "id1", "next", false, noop);
    b.asActor("admin@example.com");
    expect(b.toSmql()).toBe(
      'TRANSITION Machine "id1" TO next AS "admin@example.com"',
    );
  });

  it("generates transition with all options", () => {
    const b = new TransitionBuilder("M", "id", "done", false, noop);
    b.with({ count: 10, label: "test" })
      .memo("full options")
      .asActor("user1");
    const smql = b.toSmql();
    expect(smql).toContain('WITH { count: 10, label: "test" }');
    expect(smql).toContain('MEMO "full options"');
    expect(smql).toContain('AS "user1"');
  });

  it("generates try transition", () => {
    const b = new TransitionBuilder("Machine", "abc", "next", true, noop);
    expect(b.toSmql()).toBe('TRY TRANSITION Machine "abc" TO next');
  });

  it("generates try transition with options", () => {
    const b = new TransitionBuilder("M", "id", "done", true, noop);
    b.with({ x: 1 }).memo("try it").asActor("bot");
    const smql = b.toSmql();
    expect(smql).toMatch(/^TRY TRANSITION M/);
    expect(smql).toContain("WITH { x: 1 }");
    expect(smql).toContain('MEMO "try it"');
    expect(smql).toContain('AS "bot"');
  });

  it("generates transition with THROUGH", () => {
    const b = new TransitionBuilder("M", "id", "done", false, noop);
    b.through(["step1", "step2"]);
    expect(b.toSmql()).toContain("THROUGH [step1, step2]");
  });

  it("generates transition with OR_STAY", () => {
    const b = new TransitionBuilder("M", "id", "done", false, noop);
    b.orStay();
    expect(b.toSmql()).toContain("OR_STAY");
  });

  it("generates transition with CASCADE", () => {
    const b = new TransitionBuilder("M", "id", "done", false, noop);
    b.cascade();
    expect(b.toSmql()).toContain("CASCADE");
  });
});

describe("BatchTransitionBuilder", () => {
  it("generates batch transition", () => {
    const b = new BatchTransitionBuilder("ticket", noop);
    b.where("STATE IS open").to("closed").memo("closing all");
    const smql = b.toSmql();
    expect(smql).toBe(
      'TRANSITION ALL ticket WHERE STATE IS open TO closed MEMO "closing all"',
    );
  });
});

describe("GetBuilder", () => {
  it("generates basic get", () => {
    const b = new GetBuilder("ticket", "abc", noop);
    expect(b.toSmql()).toBe('GET ticket "abc"');
  });

  it("generates get with actor", () => {
    const b = new GetBuilder("ticket", "abc", noop);
    b.asActor("admin");
    expect(b.toSmql()).toBe('GET ticket "abc" AS ACTOR admin');
  });
});

describe("FindBuilder", () => {
  it("generates find without filter", () => {
    const b = new FindBuilder("ticket", noop);
    expect(b.toSmql()).toBe("FIND ticket");
  });

  it("generates full find", () => {
    const b = new FindBuilder("ticket", noop);
    b.where("STATE IS open")
      .sortBy("created_at", "DESC")
      .limit(10)
      .offset(5);
    expect(b.toSmql()).toBe(
      "FIND ticket WHERE STATE IS open SORT BY created_at DESC LIMIT 10 OFFSET 5",
    );
  });

  it("generates find with inState shorthand", () => {
    const b = new FindBuilder("ticket", noop);
    b.inState("open");
    expect(b.toSmql()).toBe("FIND ticket WHERE STATE IS open");
  });

  it("generates find with multiple sort clauses", () => {
    const b = new FindBuilder("ticket", noop);
    b.sortBy("priority", "DESC").sortBy("created_at", "ASC");
    expect(b.toSmql()).toBe(
      "FIND ticket SORT BY priority DESC, created_at ASC",
    );
  });

  it("generates find with offset only", () => {
    const b = new FindBuilder("ticket", noop);
    b.offset(20);
    expect(b.toSmql()).toBe("FIND ticket OFFSET 20");
  });

  it("generates find with AFTER cursor", () => {
    const b = new FindBuilder("ticket", noop);
    b.after("abc123");
    expect(b.toSmql()).toBe('FIND ticket AFTER "abc123"');
  });
});

describe("AggregateBuilder", () => {
  it("generates aggregate with measure and group", () => {
    const b = new AggregateBuilder("ticket", noop);
    b.count().groupByState();
    expect(b.toSmql()).toBe(
      "AGGREGATE ticket MEASURE COUNT() GROUP BY STATE",
    );
  });

  it("generates aggregate with no options", () => {
    const b = new AggregateBuilder("ticket", noop);
    expect(b.toSmql()).toBe("AGGREGATE ticket");
  });

  it("generates aggregate with measure only", () => {
    const b = new AggregateBuilder("ticket", noop);
    b.avg("amount");
    expect(b.toSmql()).toBe("AGGREGATE ticket MEASURE AVG(amount)");
  });

  it("generates aggregate with group only", () => {
    const b = new AggregateBuilder("ticket", noop);
    b.groupBy("priority");
    expect(b.toSmql()).toBe("AGGREGATE ticket GROUP BY priority");
  });

  it("generates aggregate with alias", () => {
    const b = new AggregateBuilder("ticket", noop);
    b.count("total").sum("amount", "revenue");
    expect(b.toSmql()).toBe(
      "AGGREGATE ticket MEASURE COUNT() AS total, SUM(amount) AS revenue",
    );
  });
});

describe("TrailBuilder", () => {
  it("generates basic trail", () => {
    const b = new TrailBuilder("abc", noop);
    expect(b.toSmql()).toBe('TRAIL OF "abc"');
  });

  it("generates trail with filters", () => {
    const b = new TrailBuilder("abc", noop);
    b.byActor("admin").fromState("open").toState("closed");
    expect(b.toSmql()).toBe(
      'TRAIL OF "abc" WHERE ACTOR admin, FROM open, TO closed',
    );
  });
});

describe("PathsBuilder", () => {
  it("generates basic paths", () => {
    const b = new PathsBuilder("ticket", noop);
    expect(b.toSmql()).toBe("PATHS FROM ticket");
  });

  it("generates paths with filter and limit", () => {
    const b = new PathsBuilder("ticket", noop);
    b.where("priority == 1").limit(10);
    expect(b.toSmql()).toBe(
      "PATHS FROM ticket WHERE priority == 1 LIMIT 10",
    );
  });
});

describe("FunnelBuilder", () => {
  it("generates funnel", () => {
    const b = new FunnelBuilder("ticket", noop);
    b.through(["new", "in_progress", "done"]);
    expect(b.toSmql()).toBe(
      "FUNNEL ticket THROUGH [new, in_progress, done]",
    );
  });

  it("generates funnel with filter", () => {
    const b = new FunnelBuilder("ticket", noop);
    b.through(["a", "b"]).where("priority == 1");
    expect(b.toSmql()).toBe(
      "FUNNEL ticket THROUGH [a, b] WHERE priority == 1",
    );
  });
});

describe("ComparePathsBuilder", () => {
  it("generates compare paths", () => {
    const b = new ComparePathsBuilder("ticket", noop);
    b.segmentBy("priority");
    expect(b.toSmql()).toBe("COMPARE PATHS ticket SEGMENT BY priority");
  });

  it("generates compare paths with filter", () => {
    const b = new ComparePathsBuilder("ticket", noop);
    b.segmentBy("region").where("year == 2026");
    expect(b.toSmql()).toBe(
      "COMPARE PATHS ticket SEGMENT BY region WHERE year == 2026",
    );
  });
});

describe("DefineMachineBuilder", () => {
  it("generates simple machine definition", () => {
    const b = new DefineMachineBuilder("TestSDK", noop);
    b.states("a", "b")
      .initialState("a")
      .terminalStates("b")
      .transition("a", "b")
      .end();
    const smql = b.toSmql();
    expect(smql).toContain("DEFINE MACHINE TestSDK");
    expect(smql).toContain("STATES { a, b }");
    expect(smql).toContain("INITIAL STATE a");
    expect(smql).toContain("TERMINAL STATES { b }");
    expect(smql).toContain("a -> b {");
  });

  it("generates machine with data fields", () => {
    const b = new DefineMachineBuilder("Order", noop);
    b.data("name", "TEXT", "REQUIRED")
      .data("amount", "INT", { type: "MIN", value: 0 })
      .states("pending", "done")
      .initialState("pending");
    const smql = b.toSmql();
    expect(smql).toContain("name : TEXT -> REQUIRED");
    expect(smql).toContain("amount : INT -> MIN(0)");
  });

  it("generates machine with transition guards and actions", () => {
    const b = new DefineMachineBuilder("M", noop);
    b.states("a", "b")
      .initialState("a")
      .transition("a", "b")
      .guard("amount > 0")
      .action('LOG("transitioning")')
      .mutate("count", "count + 1")
      .timeout("30m", "expired")
      .applyPolicy("rate_limit")
      .reactive("amount > 100")
      .end();
    const smql = b.toSmql();
    expect(smql).toContain("GUARD: amount > 0");
    expect(smql).toContain('ACTION: LOG("transitioning")');
    expect(smql).toContain("MUTATE: count = count + 1");
    expect(smql).toContain("TIMEOUT: 30m -> expired");
    expect(smql).toContain("APPLY POLICY rate_limit");
    expect(smql).toContain("REACTIVE WHEN amount > 100");
  });

  it("generates machine with roles", () => {
    const b = new DefineMachineBuilder("M", noop);
    b.states("a")
      .initialState("a")
      .role("admin")
      .canAll()
      .end()
      .role("viewer")
      .canQuery()
      .canRead("name", "status")
      .end();
    const smql = b.toSmql();
    expect(smql).toContain("CAN ALL");
    expect(smql).toContain("CAN QUERY");
    expect(smql).toContain("CAN READ { name, status }");
  });
});

describe("DefinePolicyBuilder", () => {
  it("generates policy", () => {
    const b = new DefinePolicyBuilder("rate_limit", noop);
    b.guard("request_count < 100");
    expect(b.toSmql()).toBe(
      "DEFINE POLICY rate_limit {\n  GUARD: request_count < 100\n}",
    );
  });
});

describe("DefineViewBuilder", () => {
  it("generates view", () => {
    const b = new DefineViewBuilder("open_tickets", noop);
    b.find("ticket").where("STATE IS open").sortBy("created_at", "DESC").limit(50);
    expect(b.toSmql()).toBe(
      "DEFINE VIEW open_tickets AS FIND ticket WHERE STATE IS open SORT BY created_at DESC LIMIT 50",
    );
  });
});

describe("DefineProjectionBuilder", () => {
  it("generates projection", () => {
    const b = new DefineProjectionBuilder("ticket_stats", noop);
    b.aggregate("ticket").count().groupByState().refreshOnTransition();
    expect(b.toSmql()).toBe(
      "DEFINE PROJECTION ticket_stats AS AGGREGATE ticket MEASURE COUNT() GROUP BY STATE REFRESH ON TRANSITION",
    );
  });

  it("generates projection with interval refresh", () => {
    const b = new DefineProjectionBuilder("stats", noop);
    b.aggregate("ticket").count().refreshOnInterval(60);
    expect(b.toSmql()).toContain("REFRESH ON INTERVAL 60");
  });
});

describe("DefineRuleBuilder", () => {
  it("generates rule", () => {
    const b = new DefineRuleBuilder("positive_amount", noop);
    b.beforeTransition("Order").invariant("amount > 0", "Amount must be positive");
    const smql = b.toSmql();
    expect(smql).toContain("DEFINE RULE positive_amount BEFORE TRANSITION ON Order");
    expect(smql).toContain("INVARIANT: amount > 0");
    expect(smql).toContain('MESSAGE: "Amount must be positive"');
  });
});

describe("DefineSubscriptionBuilder", () => {
  it("generates subscription on enter", () => {
    const b = new DefineSubscriptionBuilder("notify_done", noop);
    b.onEnter("done", "ticket").action('NOTIFY(self.owner, "completed")');
    const smql = b.toSmql();
    expect(smql).toContain("DEFINE SUBSCRIPTION notify_done ON ENTER done ON ticket");
    expect(smql).toContain('ACTION: NOTIFY(self.owner, "completed")');
  });

  it("generates subscription with conditional action", () => {
    const b = new DefineSubscriptionBuilder("alert", noop);
    b.onTransition("order", "pending", "shipped")
      .actionWhen("amount > 1000", 'LOG("big order shipped")');
    const smql = b.toSmql();
    expect(smql).toContain("ON TRANSITION order FROM pending TO shipped");
    expect(smql).toContain("ACTION WHEN amount > 1000");
  });
});

describe("DefineSagaBuilder", () => {
  it("generates saga", () => {
    const b = new DefineSagaBuilder("checkout", noop);
    b.triggerOnEnter("submitted", "order")
      .step("reserve_inventory")
      .transition("inventory", "self.inventory_id", "reserved")
      .compensate("inventory", "self.inventory_id", "available")
      .end()
      .step("charge_payment")
      .transition("payment", "self.payment_id", "charged")
      .end()
      .onComplete('EMIT("checkout_done")')
      .onFailure('LOG("checkout failed")');
    const smql = b.toSmql();
    expect(smql).toContain("DEFINE SAGA checkout TRIGGER ON ENTER submitted ON order");
    expect(smql).toContain("STEP reserve_inventory TRANSITION inventory self.inventory_id TO reserved");
    expect(smql).toContain("COMPENSATE inventory self.inventory_id TO available");
    expect(smql).toContain("STEP charge_payment TRANSITION payment self.payment_id TO charged");
    expect(smql).toContain('ON COMPLETE: EMIT("checkout_done")');
    expect(smql).toContain('ON FAILURE: LOG("checkout failed")');
  });
});

describe("AlterMachineBuilder", () => {
  it("generates alter with add state", () => {
    const b = new AlterMachineBuilder("ticket", noop);
    b.addState("archived");
    expect(b.toSmql()).toBe("ALTER MACHINE ticket ADD STATE archived");
  });

  it("generates alter with remove state", () => {
    const b = new AlterMachineBuilder("ticket", noop);
    b.removeState("old", "new");
    expect(b.toSmql()).toBe(
      "ALTER MACHINE ticket REMOVE STATE old MIGRATE TO new",
    );
  });

  it("generates alter with multiple operations", () => {
    const b = new AlterMachineBuilder("ticket", noop);
    b.addState("archived")
      .addTransition("closed", "archived")
      .removeTransition("open", "deleted")
      .addData("priority", "INT", ["REQUIRED"], "0")
      .removeData("legacy_field")
      .backfill("priority", "1");
    const smql = b.toSmql();
    expect(smql).toContain("ADD STATE archived");
    expect(smql).toContain("ADD TRANSITION closed -> archived");
    expect(smql).toContain("REMOVE TRANSITION open -> deleted");
    expect(smql).toContain("ADD DATA priority : INT -> REQUIRED BACKFILL 0");
    expect(smql).toContain("REMOVE DATA legacy_field");
    expect(smql).toContain("BACKFILL priority = 1");
  });
});

describe("FindBuilder - select", () => {
  it("generates find with select fields", () => {
    const b = new FindBuilder("ticket", noop);
    b.select("subject", "priority").where("STATE IS open");
    expect(b.toSmql()).toBe(
      "FIND ticket SELECT subject, priority WHERE STATE IS open",
    );
  });

  it("generates find with select only", () => {
    const b = new FindBuilder("ticket", noop);
    b.select("id", "state");
    expect(b.toSmql()).toBe("FIND ticket SELECT id, state");
  });
});

describe("AggregateBuilder - percentile", () => {
  it("generates percentile measure", () => {
    const b = new AggregateBuilder("ticket", noop);
    b.percentile("resolution_time", "p50");
    expect(b.toSmql()).toBe(
      "AGGREGATE ticket MEASURE PERCENTILE(resolution_time) AS p50",
    );
  });

  it("generates percentile without alias", () => {
    const b = new AggregateBuilder("ticket", noop);
    b.percentile("duration");
    expect(b.toSmql()).toBe(
      "AGGREGATE ticket MEASURE PERCENTILE(duration)",
    );
  });
});

describe("TrailBuilder - since/until", () => {
  it("generates trail with since", () => {
    const b = new TrailBuilder("abc", noop);
    b.since('"2026-01-01T00:00:00Z"');
    expect(b.toSmql()).toBe(
      'TRAIL OF "abc" WHERE SINCE "2026-01-01T00:00:00Z"',
    );
  });

  it("generates trail with until", () => {
    const b = new TrailBuilder("abc", noop);
    b.until('"2026-02-01T00:00:00Z"');
    expect(b.toSmql()).toBe(
      'TRAIL OF "abc" WHERE UNTIL "2026-02-01T00:00:00Z"',
    );
  });

  it("generates trail with since and until", () => {
    const b = new TrailBuilder("abc", noop);
    b.since('"2026-01-01"').until('"2026-02-01"');
    expect(b.toSmql()).toBe(
      'TRAIL OF "abc" WHERE SINCE "2026-01-01", UNTIL "2026-02-01"',
    );
  });

  it("generates trail with all filters including since/until", () => {
    const b = new TrailBuilder("abc", noop);
    b.byActor("admin").fromState("open").toState("closed")
      .since('"2026-01-01"').until('"2026-02-01"');
    expect(b.toSmql()).toBe(
      'TRAIL OF "abc" WHERE ACTOR admin, FROM open, TO closed, SINCE "2026-01-01", UNTIL "2026-02-01"',
    );
  });
});

describe("ExplainTransitionsBuilder", () => {
  it("generates schema-level explain", () => {
    const b = new ExplainTransitionsBuilder("Order", noop);
    expect(b.toSmql()).toBe("EXPLAIN TRANSITIONS FOR Order");
  });

  it("generates instance-level explain", () => {
    const b = new ExplainTransitionsBuilder("Order", noop);
    b.instance("abc123");
    expect(b.toSmql()).toBe('EXPLAIN TRANSITIONS FOR Order "abc123"');
  });

  it("generates explain with actor", () => {
    const b = new ExplainTransitionsBuilder("Order", noop);
    b.instance("abc123").asActor("admin");
    expect(b.toSmql()).toBe(
      'EXPLAIN TRANSITIONS FOR Order "abc123" AS admin',
    );
  });

  it("generates schema-level explain with actor", () => {
    const b = new ExplainTransitionsBuilder("Order", noop);
    b.asActor("viewer");
    expect(b.toSmql()).toBe("EXPLAIN TRANSITIONS FOR Order AS viewer");
  });
});

describe("GetEventsBuilder", () => {
  it("generates get all events", () => {
    const b = new GetEventsBuilder(noop);
    expect(b.toSmql()).toBe("GET EVENTS");
  });

  it("generates get events for machine", () => {
    const b = new GetEventsBuilder(noop, "Order");
    expect(b.toSmql()).toBe("GET EVENTS Order");
  });

  it("generates get events with after cursor", () => {
    const b = new GetEventsBuilder(noop, "Order");
    b.after("01HQXYZ");
    expect(b.toSmql()).toBe('GET EVENTS Order AFTER "01HQXYZ"');
  });

  it("generates get events with limit", () => {
    const b = new GetEventsBuilder(noop);
    b.limit(50);
    expect(b.toSmql()).toBe("GET EVENTS LIMIT 50");
  });

  it("generates get events with all options", () => {
    const b = new GetEventsBuilder(noop, "Order");
    b.after("01HQXYZ").limit(25);
    expect(b.toSmql()).toBe('GET EVENTS Order AFTER "01HQXYZ" LIMIT 25');
  });
});
