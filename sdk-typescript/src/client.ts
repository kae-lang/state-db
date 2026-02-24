import type {
  SmqlClientConfig,
  ExecuteResponse,
  Instance,
  TransitionResult,
  TryTransitionResult,
  BatchTransitionResult,
  AlterMachineResult,
  DefineResult,
  FindResult,
  TrailResult,
  AggregateResult,
  PathsResult,
  FunnelResult,
  ComparePathsResult,
  HealthResponse,
  MachinesListResponse,
  MachineInfo,
  DeleteInstanceResult,
  SubscribeOptions,
} from "./types.js";
import {
  SmqlError,
  SmqlErrorCode,
  BadRequestError,
  UnauthorizedError,
  NotFoundError,
  TransitionDeniedError,
  ConflictError,
  NetworkError,
  TimeoutError,
} from "./errors.js";
import { SpawnBuilder } from "./builder/spawn.js";
import { TransitionBuilder } from "./builder/transition.js";
import { BatchTransitionBuilder } from "./builder/batch-transition.js";
import { GetBuilder } from "./builder/get.js";
import { FindBuilder } from "./builder/find.js";
import { AggregateBuilder } from "./builder/aggregate.js";
import { TrailBuilder } from "./builder/trail.js";
import { PathsBuilder } from "./builder/paths.js";
import { FunnelBuilder } from "./builder/funnel.js";
import { ComparePathsBuilder } from "./builder/compare-paths.js";
import { DefineMachineBuilder } from "./builder/define-machine.js";
import { DefinePolicyBuilder } from "./builder/define-policy.js";
import { DefineViewBuilder } from "./builder/define-view.js";
import { DefineProjectionBuilder } from "./builder/define-projection.js";
import { DefineRuleBuilder } from "./builder/define-rule.js";
import { DefineSubscriptionBuilder } from "./builder/define-subscription.js";
import { DefineSagaBuilder } from "./builder/define-saga.js";
import { AlterMachineBuilder } from "./builder/alter-machine.js";
import { SmqlSubscription } from "./subscription.js";

export class SmqlClient {
  private baseUrl: string;
  private token?: string;
  private timeout: number;
  private headers: Record<string, string>;

  constructor(config: SmqlClientConfig) {
    this.baseUrl = config.url.replace(/\/+$/, "");
    this.token = config.token;
    this.timeout = config.timeout ?? 30_000;
    this.headers = config.headers ?? {};
  }

  // --- Internal request helper ---

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = {
      ...this.headers,
    };

    if (body !== undefined) {
      headers["Content-Type"] = "application/json";
    }

    if (this.token) {
      headers["Authorization"] = `Bearer ${this.token}`;
    }

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);

    let response: Response;
    try {
      response = await fetch(url, {
        method,
        headers,
        body: body !== undefined ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });
    } catch (err: unknown) {
      clearTimeout(timer);
      if (err instanceof DOMException && err.name === "AbortError") {
        throw new TimeoutError(`Request timed out after ${this.timeout}ms`);
      }
      const message = err instanceof Error ? err.message : String(err);
      throw new NetworkError(`Network error: ${message}`);
    } finally {
      clearTimeout(timer);
    }

    if (!response.ok) {
      let errorMsg: string;
      try {
        const errBody = await response.json() as { error?: string };
        errorMsg = errBody.error ?? response.statusText;
      } catch {
        errorMsg = response.statusText;
      }

      switch (response.status) {
        case 400:
          throw new BadRequestError(errorMsg);
        case 401:
          throw new UnauthorizedError(errorMsg);
        case 404:
          throw new NotFoundError(errorMsg);
        case 409:
          throw new TransitionDeniedError(errorMsg);
        default:
          throw new SmqlError(
            errorMsg,
            SmqlErrorCode.ServerError,
            response.status,
          );
      }
    }

    return (await response.json()) as T;
  }

  // --- Raw execute ---

  async execute(smql: string): Promise<ExecuteResponse> {
    const url = `${this.baseUrl}/execute`;
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      ...this.headers,
    };

    if (this.token) {
      headers["Authorization"] = `Bearer ${this.token}`;
    }

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);

    let response: Response;
    try {
      response = await fetch(url, {
        method: "POST",
        headers,
        body: JSON.stringify({ smql }),
        signal: controller.signal,
      });
    } catch (err: unknown) {
      clearTimeout(timer);
      if (err instanceof DOMException && err.name === "AbortError") {
        throw new TimeoutError(`Request timed out after ${this.timeout}ms`);
      }
      const message = err instanceof Error ? err.message : String(err);
      throw new NetworkError(`Network error: ${message}`);
    } finally {
      clearTimeout(timer);
    }

    const body = (await response.json()) as ExecuteResponse;

    if (!body.success) {
      const msg = body.error ?? "Unknown error";
      switch (response.status) {
        case 400:
          throw new BadRequestError(msg);
        case 401:
          throw new UnauthorizedError(msg);
        case 404:
          throw new NotFoundError(msg);
        case 409:
          throw new TransitionDeniedError(msg);
        default:
          throw new SmqlError(msg, SmqlErrorCode.ServerError, response.status);
      }
    }

    return body;
  }

  async executeAs<T>(smql: string): Promise<T> {
    const resp = await this.execute(smql);
    if (resp.result === undefined) {
      throw new SmqlError("No result in response", SmqlErrorCode.ServerError);
    }
    return resp.result as T;
  }

  // --- REST wrappers ---

  async health(): Promise<boolean> {
    const body = await this.request<HealthResponse>("GET", "/health");
    return body.status === "ok";
  }

  async listMachines(): Promise<string[]> {
    const body = await this.request<MachinesListResponse>("GET", "/machines");
    return body.machines;
  }

  async getMachine(name: string): Promise<MachineInfo> {
    return this.request<MachineInfo>("GET", `/machines/${encodeURIComponent(name)}`);
  }

  async getInstance(id: string): Promise<Instance> {
    return this.request<Instance>("GET", `/instances/${encodeURIComponent(id)}`);
  }

  async deleteInstance(id: string): Promise<DeleteInstanceResult> {
    return this.request<DeleteInstanceResult>(
      "DELETE",
      `/instances/${encodeURIComponent(id)}`,
    );
  }

  async getMetrics(): Promise<string> {
    const url = `${this.baseUrl}/metrics`;
    const headers: Record<string, string> = { ...this.headers };
    if (this.token) headers["Authorization"] = `Bearer ${this.token}`;

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);

    let response: Response;
    try {
      response = await fetch(url, { headers, signal: controller.signal });
    } catch (err: unknown) {
      clearTimeout(timer);
      if (err instanceof DOMException && err.name === "AbortError") {
        throw new TimeoutError(`Request timed out after ${this.timeout}ms`);
      }
      const message = err instanceof Error ? err.message : String(err);
      throw new NetworkError(`Network error: ${message}`);
    } finally {
      clearTimeout(timer);
    }

    return response.text();
  }

  // --- Builder entry points ---

  spawn(machine: string): SpawnBuilder {
    return new SpawnBuilder(machine, (smql) =>
      this.executeAs<Instance>(smql),
    );
  }

  transition(
    machine: string,
    id: string,
    state: string,
  ): TransitionBuilder<TransitionResult> {
    return new TransitionBuilder<TransitionResult>(
      machine,
      id,
      state,
      false,
      (smql) => this.executeAs<TransitionResult>(smql),
    );
  }

  tryTransition(
    machine: string,
    id: string,
    state: string,
  ): TransitionBuilder<TryTransitionResult> {
    return new TransitionBuilder<TryTransitionResult>(
      machine,
      id,
      state,
      true,
      (smql) => this.executeAs<TryTransitionResult>(smql),
    );
  }

  transitionAll(machine: string): BatchTransitionBuilder {
    return new BatchTransitionBuilder(machine, (smql) =>
      this.executeAs<BatchTransitionResult>(smql),
    );
  }

  get(machine: string, id: string): GetBuilder {
    return new GetBuilder(machine, id, (smql) =>
      this.executeAs<Instance>(smql),
    );
  }

  find(machine: string): FindBuilder {
    return new FindBuilder(machine, (smql) =>
      this.executeAs<FindResult>(smql),
    );
  }

  aggregate(machine: string): AggregateBuilder {
    return new AggregateBuilder(machine, (smql) =>
      this.executeAs<AggregateResult>(smql),
    );
  }

  trail(id: string): TrailBuilder {
    return new TrailBuilder(id, (smql) =>
      this.executeAs<TrailResult>(smql),
    );
  }

  paths(machine: string): PathsBuilder {
    return new PathsBuilder(machine, (smql) =>
      this.executeAs<PathsResult>(smql),
    );
  }

  funnel(machine: string): FunnelBuilder {
    return new FunnelBuilder(machine, (smql) =>
      this.executeAs<FunnelResult>(smql),
    );
  }

  comparePaths(machine: string): ComparePathsBuilder {
    return new ComparePathsBuilder(machine, (smql) =>
      this.executeAs<ComparePathsResult>(smql),
    );
  }

  defineMachine(name: string): DefineMachineBuilder {
    return new DefineMachineBuilder(name, (smql) =>
      this.executeAs<DefineResult>(smql),
    );
  }

  definePolicy(name: string): DefinePolicyBuilder {
    return new DefinePolicyBuilder(name, (smql) =>
      this.executeAs<DefineResult>(smql),
    );
  }

  defineView(name: string): DefineViewBuilder {
    return new DefineViewBuilder(name, (smql) =>
      this.executeAs<DefineResult>(smql),
    );
  }

  defineProjection(name: string): DefineProjectionBuilder {
    return new DefineProjectionBuilder(name, (smql) =>
      this.executeAs<DefineResult>(smql),
    );
  }

  defineRule(name: string): DefineRuleBuilder {
    return new DefineRuleBuilder(name, (smql) =>
      this.executeAs<DefineResult>(smql),
    );
  }

  defineSubscription(name: string): DefineSubscriptionBuilder {
    return new DefineSubscriptionBuilder(name, (smql) =>
      this.executeAs<DefineResult>(smql),
    );
  }

  defineSaga(name: string): DefineSagaBuilder {
    return new DefineSagaBuilder(name, (smql) =>
      this.executeAs<DefineResult>(smql),
    );
  }

  alterMachine(name: string): AlterMachineBuilder {
    return new AlterMachineBuilder(name, (smql) =>
      this.executeAs<AlterMachineResult>(smql),
    );
  }

  async getView(name: string): Promise<FindResult> {
    return this.executeAs<FindResult>(`GET VIEW ${name}`);
  }

  async getProjection(name: string): Promise<AggregateResult> {
    return this.executeAs<AggregateResult>(`GET PROJECTION ${name}`);
  }

  subscribe(options?: SubscribeOptions): SmqlSubscription {
    return new SmqlSubscription(this.baseUrl, {
      ...options,
      token: this.token,
    });
  }
}
