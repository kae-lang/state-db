// Client
export { SmqlClient } from "./client.js";

// Subscription
export { SmqlSubscription } from "./subscription.js";

// Types
export type {
  SmqlClientConfig,
  ExecuteResponse,
  Instance,
  DefineResult,
  SpawnResult,
  TransitionResult,
  TryTransitionResult,
  BatchTransitionResult,
  AlterMachineResult,
  FindResult,
  TrailEntry,
  TrailResult,
  AggregateRow,
  AggregateResult,
  PathEntry,
  PathsResult,
  FunnelStage,
  FunnelResult,
  ComparePathsSegment,
  ComparePathsResult,
  HealthResponse,
  MachinesListResponse,
  MachineInfo,
  DeleteInstanceResult,
  SubscriptionEvent,
  SubscribeOptions,
  SmqlValue,
  SortDirection,
  SortClause,
  DataType,
  Constraint,
} from "./types.js";

// Errors
export {
  SmqlError,
  SmqlErrorCode,
  BadRequestError,
  UnauthorizedError,
  NotFoundError,
  TransitionDeniedError,
  ConflictError,
  NetworkError,
  TimeoutError,
  SubscriptionError,
} from "./errors.js";

// Expression builder
export { Expr } from "./builder/expression.js";

// Builder classes (for advanced usage)
export {
  SpawnBuilder,
  TransitionBuilder,
  BatchTransitionBuilder,
  GetBuilder,
  FindBuilder,
  AggregateBuilder,
  TrailBuilder,
  PathsBuilder,
  FunnelBuilder,
  ComparePathsBuilder,
  DefineMachineBuilder,
  TransitionDefBuilder,
  HookDefBuilder,
  RoleDefBuilder,
  DefinePolicyBuilder,
  DefineViewBuilder,
  DefineProjectionBuilder,
  DefineRuleBuilder,
  DefineSubscriptionBuilder,
  DefineSagaBuilder,
  SagaStepBuilder,
  AlterMachineBuilder,
} from "./builder/index.js";

// Value serialization utilities
export { valueToSmql, escapeString } from "./builder/value.js";
