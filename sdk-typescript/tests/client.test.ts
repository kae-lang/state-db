import { describe, it, expect, vi, beforeEach } from "vitest";
import { SmqlClient } from "../src/client.js";
import {
  BadRequestError,
  NotFoundError,
  TransitionDeniedError,
  UnauthorizedError,
  NetworkError,
} from "../src/errors.js";

// Mock global fetch
const mockFetch = vi.fn();
vi.stubGlobal("fetch", mockFetch);

function jsonResponse(body: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? "OK" : "Error",
    json: () => Promise.resolve(body),
    text: () => Promise.resolve(JSON.stringify(body)),
    headers: new Headers(),
  } as unknown as Response;
}

describe("SmqlClient", () => {
  beforeEach(() => {
    mockFetch.mockReset();
  });

  describe("URL construction", () => {
    it("strips trailing slashes from base URL", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200/" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({ status: "ok" }),
      );
      await client.health();
      expect(mockFetch).toHaveBeenCalledWith(
        "http://localhost:4200/health",
        expect.any(Object),
      );
    });
  });

  describe("header injection", () => {
    it("sends Authorization header when token is set", async () => {
      const client = new SmqlClient({
        url: "http://localhost:4200",
        token: "my-secret-token",
      });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({ status: "ok" }),
      );
      await client.health();
      const callArgs = mockFetch.mock.calls[0];
      expect(callArgs[1].headers["Authorization"]).toBe(
        "Bearer my-secret-token",
      );
    });

    it("sends custom headers", async () => {
      const client = new SmqlClient({
        url: "http://localhost:4200",
        headers: { "X-Custom": "value" },
      });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({ status: "ok" }),
      );
      await client.health();
      const callArgs = mockFetch.mock.calls[0];
      expect(callArgs[1].headers["X-Custom"]).toBe("value");
    });
  });

  describe("error mapping", () => {
    it("maps 400 to BadRequestError", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse(
          { success: false, error: "Parse error" },
          400,
        ),
      );
      await expect(client.execute("INVALID")).rejects.toThrow(
        BadRequestError,
      );
    });

    it("maps 401 to UnauthorizedError", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse(
          { success: false, error: "Unauthorized" },
          401,
        ),
      );
      await expect(client.execute("FIND x")).rejects.toThrow(
        UnauthorizedError,
      );
    });

    it("maps 404 to NotFoundError", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse(
          { success: false, error: "Not found" },
          404,
        ),
      );
      await expect(client.execute("GET x \"abc\"")).rejects.toThrow(
        NotFoundError,
      );
    });

    it("maps 409 to TransitionDeniedError", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse(
          { success: false, error: "Guard failed" },
          409,
        ),
      );
      await expect(
        client.execute('TRANSITION M "id" TO next'),
      ).rejects.toThrow(TransitionDeniedError);
    });

    it("maps fetch failure to NetworkError", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockRejectedValueOnce(new TypeError("fetch failed"));
      await expect(client.execute("FIND x")).rejects.toThrow(
        NetworkError,
      );
    });
  });

  describe("execute", () => {
    it("returns successful response", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({
          success: true,
          result: { id: "abc", machine: "test", state: "open" },
        }),
      );
      const resp = await client.execute("SPAWN test {}");
      expect(resp.success).toBe(true);
      expect(resp.result).toEqual({
        id: "abc",
        machine: "test",
        state: "open",
      });
    });

    it("sends SMQL in JSON body", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({ success: true, result: {} }),
      );
      await client.execute("FIND ticket");
      const body = JSON.parse(mockFetch.mock.calls[0][1].body);
      expect(body.smql).toBe("FIND ticket");
    });
  });

  describe("REST endpoints", () => {
    it("health returns true when ok", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({ status: "ok" }),
      );
      expect(await client.health()).toBe(true);
    });

    it("listMachines returns array", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({ machines: ["order", "ticket"] }),
      );
      const result = await client.listMachines();
      expect(result).toEqual(["order", "ticket"]);
    });

    it("getMachine returns info", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({
          name: "order",
          states: ["pending", "done"],
          initial_state: "pending",
          terminal_states: ["done"],
          version: 1,
        }),
      );
      const info = await client.getMachine("order");
      expect(info.name).toBe("order");
      expect(info.states).toEqual(["pending", "done"]);
    });

    it("getInstance returns instance", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({
          id: "abc",
          machine: "test",
          state: "open",
          data: {},
          created_at: "2026-01-01T00:00:00Z",
          updated_at: "2026-01-01T00:00:00Z",
          state_entered_at: "2026-01-01T00:00:00Z",
          trail_length: 0,
          version: 1,
        }),
      );
      const inst = await client.getInstance("abc");
      expect(inst.id).toBe("abc");
      expect(inst.state).toBe("open");
    });

    it("deleteInstance returns result", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({ deleted: true, id: "abc" }),
      );
      const result = await client.deleteInstance("abc");
      expect(result.deleted).toBe(true);
    });
  });

  describe("builder entry points", () => {
    it("spawn returns a builder", () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      const builder = client.spawn("test");
      expect(builder.toSmql()).toBe("SPAWN test {}");
    });

    it("transition returns a builder", () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      const builder = client.transition("test", "abc", "next");
      expect(builder.toSmql()).toBe('TRANSITION test "abc" TO next');
    });

    it("tryTransition returns a builder", () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      const builder = client.tryTransition("test", "abc", "next");
      expect(builder.toSmql()).toBe('TRY TRANSITION test "abc" TO next');
    });

    it("find returns a builder", () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      const builder = client.find("test");
      expect(builder.toSmql()).toBe("FIND test");
    });

    it("aggregate returns a builder", () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      const builder = client.aggregate("test");
      expect(builder.toSmql()).toBe("AGGREGATE test");
    });

    it("trail returns a builder", () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      const builder = client.trail("abc");
      expect(builder.toSmql()).toBe('TRAIL OF "abc"');
    });

    it("explainTransitions returns a builder", () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      const builder = client.explainTransitions("Order");
      expect(builder.toSmql()).toBe("EXPLAIN TRANSITIONS FOR Order");
    });

    it("explainTransitions with instance returns a builder", () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      const builder = client.explainTransitions("Order").instance("abc123");
      expect(builder.toSmql()).toBe(
        'EXPLAIN TRANSITIONS FOR Order "abc123"',
      );
    });

    it("getEvents returns a builder", () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      const builder = client.getEvents("Order");
      expect(builder.toSmql()).toBe("GET EVENTS Order");
    });

    it("getEvents without machine returns a builder", () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      const builder = client.getEvents();
      expect(builder.toSmql()).toBe("GET EVENTS");
    });
  });

  describe("REST shortcuts", () => {
    it("getTransitions calls REST endpoint", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({
          machine: "Order",
          current_state: "pending",
          instance_id: "abc",
          transitions: [],
        }),
      );
      const result = await client.getTransitions("abc");
      expect(mockFetch).toHaveBeenCalledWith(
        "http://localhost:4200/instances/abc/transitions",
        expect.any(Object),
      );
      expect(result.machine).toBe("Order");
    });

    it("getTransitions with actor passes query param", async () => {
      const client = new SmqlClient({ url: "http://localhost:4200" });
      mockFetch.mockResolvedValueOnce(
        jsonResponse({
          machine: "Order",
          transitions: [],
        }),
      );
      await client.getTransitions("abc", "admin");
      expect(mockFetch).toHaveBeenCalledWith(
        "http://localhost:4200/instances/abc/transitions?as=admin",
        expect.any(Object),
      );
    });
  });
});
