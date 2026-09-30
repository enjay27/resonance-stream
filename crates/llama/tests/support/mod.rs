//! Test doubles for the two ends of the app: the game server that sends chat
//! on port 5003, and llama-server. Each test binary uses part of them.
#![allow(dead_code)]

pub mod game_server;
pub mod harness;
pub mod llama_server;
