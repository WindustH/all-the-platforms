//! What all-the-platforms's Facebook and Instagram platforms share: the Graph API
//! transport ([`graph`]) and the Marketing API commands ([`ads`]).

pub mod ads;
pub mod graph;

pub use graph::Graph;
