//! This is part of `websocket` crate that is independent from `hyper`.
//! It contains code for processing WebSocket streams,
//! (after HTTP upgrade already happened)
//! WebSocket message definition, some error type.
//!
//! Note that there is no normal example of usage so far.

extern crate byteorder;
extern crate bytes;
extern crate futures;
extern crate rand;
#[macro_use]
extern crate bitflags;

#[cfg(any(feature = "sync-ssl", feature = "async-ssl"))]
extern crate native_tls;

#[cfg(feature = "async")]
extern crate tokio_codec;
#[cfg(feature = "async")]
extern crate tokio_io;
#[cfg(feature = "async")]
extern crate tokio_tcp;
#[cfg(feature = "async-ssl")]
extern crate tokio_tls;

pub mod codec;
pub mod dataframe;
pub mod header;
pub mod message;
pub mod result;
pub mod stream;
pub mod ws;

pub use crate::message::Message;
pub use crate::message::OwnedMessage;
