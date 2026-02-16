use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGaugeVec, Opts, Registry, TextEncoder,
};

/// All Prometheus metrics for the SMQL server.
#[derive(Clone)]
pub struct SmqlMetrics {
    pub registry: Registry,
    pub instances_total: IntGaugeVec,
    pub transitions_total: IntCounterVec,
    pub transition_duration_seconds: HistogramVec,
    pub guard_failures_total: IntCounterVec,
    pub timeout_fires_total: IntCounterVec,
    pub query_duration_seconds: HistogramVec,
    pub spawns_total: IntCounterVec,
}

impl SmqlMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let instances_total = IntGaugeVec::new(
            Opts::new(
                "smql_instances_total",
                "Number of active instances by machine and state",
            ),
            &["machine", "state"],
        )
        .unwrap();

        let transitions_total = IntCounterVec::new(
            Opts::new(
                "smql_transitions_total",
                "Total transitions by machine, from, to",
            ),
            &["machine", "from", "to"],
        )
        .unwrap();

        let transition_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "smql_transition_duration_seconds",
                "Transition duration in seconds",
            ),
            &["machine"],
        )
        .unwrap();

        let guard_failures_total = IntCounterVec::new(
            Opts::new(
                "smql_guard_failures_total",
                "Total guard failures by machine",
            ),
            &["machine"],
        )
        .unwrap();

        let timeout_fires_total = IntCounterVec::new(
            Opts::new(
                "smql_timeout_fires_total",
                "Total timeout transitions by machine and state",
            ),
            &["machine", "state"],
        )
        .unwrap();

        let query_duration_seconds = HistogramVec::new(
            HistogramOpts::new("smql_query_duration_seconds", "Query duration in seconds"),
            &["query_type"],
        )
        .unwrap();

        let spawns_total = IntCounterVec::new(
            Opts::new("smql_spawns_total", "Total spawned instances by machine"),
            &["machine"],
        )
        .unwrap();

        registry
            .register(Box::new(instances_total.clone()))
            .unwrap();
        registry
            .register(Box::new(transitions_total.clone()))
            .unwrap();
        registry
            .register(Box::new(transition_duration_seconds.clone()))
            .unwrap();
        registry
            .register(Box::new(guard_failures_total.clone()))
            .unwrap();
        registry
            .register(Box::new(timeout_fires_total.clone()))
            .unwrap();
        registry
            .register(Box::new(query_duration_seconds.clone()))
            .unwrap();
        registry.register(Box::new(spawns_total.clone())).unwrap();

        Self {
            registry,
            instances_total,
            transitions_total,
            transition_duration_seconds,
            guard_failures_total,
            timeout_fires_total,
            query_duration_seconds,
            spawns_total,
        }
    }

    /// Encode all metrics as Prometheus text format.
    pub fn encode(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

impl Default for SmqlMetrics {
    fn default() -> Self {
        Self::new()
    }
}
