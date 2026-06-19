//! Method implementations, grouped by responsibility. Each returns
//! `Result<Resp, CodeError>`; the router wraps the result into `CodeResponse`.

pub mod edit;
pub mod git;
pub mod read;
pub mod search;
pub mod symbols;
