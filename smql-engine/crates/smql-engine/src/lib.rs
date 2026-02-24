// SMQL Engine Core — State machine execution engine

pub mod alter;
pub mod engine;
pub mod eval;
pub mod query;

#[cfg(test)]
mod eval_tests;
#[cfg(test)]
mod tests;

pub use alter::AlterResult;
pub use engine::{BatchSpawnResult, BatchSpawnFailure, Engine, SpawnResult, TransactionStepResult, TransitionResult, WatchResult};
pub use eval::{eval_expr, eval_guard, ActorInfo, ChildInfo, EvalContext};
pub use query::{AggregateRow, FunnelResult, FunnelStage, PathResult, QueryResult};
