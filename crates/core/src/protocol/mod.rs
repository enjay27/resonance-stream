//! Port 5003 chat protocol: TCP stream framing and protobuf-style decoding.

mod compression;
mod decoder;
pub mod framing;
pub mod parser;
