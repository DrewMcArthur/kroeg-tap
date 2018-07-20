//! Traits for all things that have to do with storing and retrieving entities.

use super::entity::StoreItem;

use futures::prelude::*;

use std::error::Error;
use std::fmt::Debug;

/// An entity store, storing JSON-LD `Entity` objects.
pub trait EntityStore: Debug + Send + 'static {
    /// The error type that will be returned if this store fails to get or put
    /// the `StoreItem`
    type Error: Error + Send + Sync + 'static;

    /// The `Future` that is returned when `get`ting a `StoreItem`.
    type GetFuture: Future<Item = Option<StoreItem>, Error = Self::Error> + 'static + Send;

    /// The `Future` that is returned when `put`ting a `StoreItem`.
    type StoreFuture: Future<Item = StoreItem, Error = Self::Error> + 'static + Send;

    /// The `Future` that is returned when reading the collection data.
    type ReadCollectionFuture: Future<Item = CollectionPointer, Error = Self::Error>
        + 'static
        + Send;

    /// The `Future` that is returned when writing into a collection.
    type WriteCollectionFuture: Future<Item = (), Error = Self::Error> + 'static + Send;

    /// Gets a single `StoreItem` from the store. Missing entities are no error,
    /// but instead returns a `None`.
    fn get(&self, path: String) -> Self::GetFuture;

    /// Stores a single `StoreItem` into the store.
    ///
    /// To delete an Entity, set its type to as:Tombstone. This may
    /// instantly remove it, or queue it for possible future deletion.
    fn put(&mut self, path: String, item: StoreItem) -> Self::StoreFuture;

    /// Reads N amount of items from the collection corresponding to a specific ID. If a cursor is passed,
    /// it can be used to paginate.
    fn read_collection(
        &self,
        path: String,
        count: Option<u32>,
        cursor: Option<String>,
    ) -> Self::ReadCollectionFuture;

    /// Finds an item in a collection. The result will contain cursors to just before and after the item, if it exists.
    fn find_collection(&self, path: String, item: String) -> Self::ReadCollectionFuture;

    /// Inserts an item into the back of the collection.
    fn insert_collection(&mut self, path: String, item: String) -> Self::WriteCollectionFuture;

    /// Removes an item from the collection.
    fn remove_collection(&mut self, path: String, item: String) -> Self::WriteCollectionFuture;
}

#[derive(Debug)]
pub struct CollectionPointer {
    pub items: Vec<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub count: Option<u32>,
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

pub trait QueueItem {
    fn event(&self) -> &str;
    fn data(&self) -> &str;
}

pub trait QueueStore: Debug + Send + 'static {
    type Item: QueueItem + 'static;
    type Error: Error + Send + Sync + 'static;
    type GetItemFuture: Future<Item = Option<Self::Item>, Error = Self::Error> + Send + 'static;
    type MarkFuture: Future<Item = (), Error = Self::Error> + Send + 'static;

    fn get_item(&self) -> Self::GetItemFuture;

    fn mark_success(&self, item: Self::Item) -> Self::MarkFuture;
    fn mark_failure(&self, item: Self::Item) -> Self::MarkFuture;

    fn add(&self, event: String, data: String) -> Self::MarkFuture;
}
