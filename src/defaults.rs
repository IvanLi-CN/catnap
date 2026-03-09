// Topology probe discovers brand-new countries/regions quickly without adding much load.
pub const FIXED_CATALOG_TOPOLOGY_PROBE_INTERVAL_MINUTES: i64 = 15;
// Formal topology refresh still runs less often to retire removed targets conservatively.
pub const FIXED_CATALOG_TOPOLOGY_REFRESH_INTERVAL_HOURS: i64 = 1;
