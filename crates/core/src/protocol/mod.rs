//! Port 5003 chat protocol: TCP stream reassembly and protobuf-style decoding.

mod decoder;
pub mod packet_buffer;
pub mod parser;
