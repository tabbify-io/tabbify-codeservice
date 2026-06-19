//! Method implementations, grouped by responsibility. Each returns
//! `Result<Resp, CodeError>`; the router wraps the result into `CodeResponse`.

pub mod read;
pub mod search;
