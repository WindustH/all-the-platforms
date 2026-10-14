//! What all-the-platforms's Facebook, Instagram and Threads platforms
//! share: the Graph API transport ([`graph`], also for graph.threads.net)
//! and its error mapping ([`errors`]), times and analytics windows
//! ([`time`]), and the Marketing API commands ([`ads`]).

pub mod ads;
pub mod errors;
pub mod graph;
pub mod time;

pub use graph::Graph;
