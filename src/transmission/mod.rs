//! Transmission BitTorrent client have an [RPC API](https://github.com/transmission/transmission/blob/main/docs/rpc-spec.md) allowing it to be controlled

/// Adaptor to handle transmission
pub mod adaptor;

/// Module for [RPC API](https://github.com/transmission/transmission/blob/main/docs/rpc-spec.md)
pub mod api;
