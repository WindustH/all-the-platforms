//! What all-the-platforms's Facebook and Instagram platforms share: the Graph
//! API transport ([`graph`]) and its error mapping ([`errors`]), times and
//! analytics windows ([`time`]), and the Marketing API commands ([`ads`]).

pub mod ads;
pub mod errors;
pub mod graph;
pub mod time;

pub use graph::Graph;
