//! Frontend-agnostic MCP server. Owns no state; drives the live OpenRig
//! instance through `application::bridge::CommandBridge` and surfaces every
//! event (GUI- or MCP-originated) via `application::bridge::EventStreamRx`.
//!
//! Filled in by Task 4 of the #165 plan.

#![allow(dead_code)]
