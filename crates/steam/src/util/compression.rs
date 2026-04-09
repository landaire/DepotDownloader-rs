//! Compression utilities.
//!
//! The primary decompression logic for depot chunks lives in
//! [`crate::depot::chunk`]. This module re-exports the compression
//! format enum for use elsewhere.

pub use crate::depot::chunk::ChunkCompression;
