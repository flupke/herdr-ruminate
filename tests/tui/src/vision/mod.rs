//! `make vision`: an MCP server that gives an agent eyes and hands on the reviewer, which runs in
//! herdr-common's app sandbox, a private herdr-app on a headless display, on a scratch
//! repository.

mod agent;
mod forks;
mod scratch;
mod server;
mod session;

pub use server::{Setup, serve};
