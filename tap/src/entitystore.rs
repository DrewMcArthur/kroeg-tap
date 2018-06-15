//! Traits for all things that have to do with storing and retrieving entities.

use super::entity::StoreItem;
use std::fmt::{self, Debug};

use futures::future;
use futures::prelude::*;

use std::error::Error;

/// An entity store, storing JSON-LD `Entity` objects.
pub trait EntityStore: Debug + Send + 'static {
    /// The error type that will be returned if this store fails to get or put
    /// the `StoreItem`
    type Error: Error + Send;

    /// The `Future` that is returned when `get`ting a `StoreItem`.
    type GetFuture: Future<Item = Option<StoreItem>, Error = Self::Error> + 'static + Send;

    /// The `Future` that is returned when `put`ting a `StoreItem`.
    type StoreFuture: Future<Item = StoreItem, Error = Self::Error> + 'static + Send;

    /// Gets a single `StoreItem` from the store. Missing entities are no error,
    /// but instead returns a `None`.
    fn get(&self, path: String) -> Self::GetFuture;

    /// Stores a single `StoreItem` into the store.
    ///
    /// To delete an Entity, set its type to as:Tombstone. This may
    /// instantly remove it, or queue it for possible future deletion.
    fn put(&mut self, path: String, item: StoreItem) -> Self::StoreFuture;
}

/// A recursive entity store, that, if trying to get an unknown `Entity`, will
/// instead ask the next `EntityStore`.
pub trait RecursiveEntityStore<T>: EntityStore
where
    T: EntityStore,
{
    /// Get a mutable reference to the next EntityStore. This function is
    /// needed in rare cases inside the `MessageHandler`, mostly for `Update`.
    fn next(&mut self) -> &mut T;
}

#[derive(Debug)]
/// Error returned when the message reaches (), and thus fails to process it.
pub struct EndOfChainError;

impl fmt::Display for EndOfChainError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "end of chain without store method being handled")
    }
}

impl Error for EndOfChainError {
    fn cause(&self) -> Option<&Error> {
        None
    }
}

/// Null implementation, to allow for type chains that end with an empty tuple.
///
/// If you have two entity stores, you could do e.g. `StoreOne<StoreTwo<()>>`.
impl EntityStore for () {
    type Error = EndOfChainError;

    type GetFuture = future::FutureResult<Option<StoreItem>, EndOfChainError>;
    type StoreFuture = future::FutureResult<StoreItem, EndOfChainError>;

    fn get(&self, _: String) -> Self::GetFuture {
        future::ok(None)
    }

    fn put(&mut self, _: String, _: StoreItem) -> Self::StoreFuture {
        future::err(EndOfChainError)
    }
}
