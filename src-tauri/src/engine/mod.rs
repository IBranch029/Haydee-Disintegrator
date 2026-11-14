//! Deep purge engine.
//!
//! The engine is intentionally split into small, single-responsibility modules
//! that all destructive operations funnel through:
//!
//! * [`safety`] – the heuristic guard that decides what may be destroyed.
//! * [`registry`] – registry traversal, isolation and scrubbing.
//! * [`filesystem`] – recursive obliteration with reboot deferral.
//! * [`processes`] – process tree neutralisation and lock resolution.
//! * [`services`] – Windows service stop and deletion.
//! * [`tasks`] – scheduled task enumeration and removal.
//! * [`startup`] – autostart hooks (registry `Run` values, startup folder).
//! * [`apps`] – installed application inventory.
//! * [`residue`] – deep inspection that aggregates every residue source.
//! * [`purge`] – the orchestrated destruction pipeline.
//! * [`restore`] – system restore point creation.
//! * [`sysinfo`] – host telemetry for the dashboard gauges.

pub mod apps;
pub mod filesystem;
pub mod processes;
pub mod purge;
pub mod registry;
pub mod residue;
pub mod restore;
pub mod safety;
pub mod services;
pub mod startup;
pub mod sysinfo;
pub mod tasks;
