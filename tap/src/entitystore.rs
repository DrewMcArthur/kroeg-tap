//! Traits for all things that have to do with storing and retrieving entities.

use super::entity::StoreItem;

use futures::prelude::*;

use std::error::Error;
use std::fmt::Debug;

use super::QuadQuery;

/// An entity store, storing JSON-LD `Entity` objects.
pub trait EntityStore: Debug + Send + Sized + 'static {
    /// The error type that will be returned if this store fails to get or put
    /// the `StoreItem`
    type Error: Error + Send + Sync + 'static;

    // ---

    /// The `Future` that is returned when `get`ting a `StoreItem`.
    type GetFuture: Future<Item = (Option<StoreItem>, Self), Error = (Self::Error, Self)>
        + 'static
        + Send;

    /// Gets a single `StoreItem` from the store. Missing entities are no error,
    /// but instead returns a `None`.
    fn get(self, path: String, local: bool) -> Self::GetFuture;

    // ---

    /// The `Future` that is returned when `put`ting a `StoreItem`.
    type StoreFuture: Future<Item = (StoreItem, Self), Error = (Self::Error, Self)> + 'static + Send;

    /// Stores a single `StoreItem` into the store.
    ///
    /// To delete an Entity, set its type to as:Tombstone. This may
    /// instantly remove it, or queue it for possible future deletion.
    fn put(self, path: String, item: StoreItem) -> Self::StoreFuture;

    // -----

    /// The `Future` that is returned when querying the database.
    type QueryFuture: Future<Item = (Vec<Vec<String>>, Self), Error = (Self::Error, Self)>
        + 'static
        + Send;

    /// Queries the entire store for a specific set of parameters.
    /// The return value is a list for every result in the database that matches the query.
    /// The array elements are in numeric order of the placeholders.
    fn query(self, query: Vec<QuadQuery>) -> Self::QueryFuture;

    // -----

    /// The `Future` that is returned when reading the collection data.
    type ReadCollectionFuture: Future<Item = (CollectionPointer, Self), Error = (Self::Error, Self)>
        + 'static
        + Send;

    /// Reads N amount of items from the collection corresponding to a specific ID. If a cursor is passed,
    /// it can be used to paginate.
    fn read_collection(
        self,
        path: String,
        count: Option<u32>,
        cursor: Option<String>,
    ) -> Self::ReadCollectionFuture;

    // -----

    type FindCollectionFuture: Future<Item = (CollectionPointer, Self), Error = (Self::Error, Self)>
        + 'static
        + Send;

    /// Finds an item in a collection. The result will contain cursors to just before and after the item, if it exists.
    fn find_collection(self, path: String, item: String) -> Self::FindCollectionFuture;

    // -----

    /// The `Future` that is returned when writing into a collection.
    type WriteCollectionFuture: Future<Item = Self, Error = (Self::Error, Self)> + 'static + Send;

    /// Inserts an item into the back of the collection.
    fn insert_collection(self, path: String, item: String) -> Self::WriteCollectionFuture;

    // -----

    type ReadCollectionInverseFuture: Future<
            Item = (CollectionPointer, Self),
            Error = (Self::Error, Self),
        >
        + 'static
        + Send;

    /// Finds all the collections containing a specific object.
    fn read_collection_inverse(self, item: String) -> Self::ReadCollectionInverseFuture;

    // -----

    type RemoveCollectionFuture: Future<Item = Self, Error = (Self::Error, Self)> + 'static + Send;

    /// Removes an item from the collection.
    fn remove_collection(self, path: String, item: String) -> Self::RemoveCollectionFuture;
}

#[derive(Debug)]
pub struct CollectionPointer {
    pub items: Vec<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub count: Option<u32>,
}

pub trait QueueItem {
    fn event(&self) -> &str;
    fn data(&self) -> &str;
}

pub trait QueueStore: Debug + Send + Sized + 'static {
    type Item: QueueItem + 'static;
    type Error: Error + Send + Sync + 'static;
    type GetItemFuture: Future<Item = (Option<Self::Item>, Self), Error = (Self::Error, Self)>
        + Send
        + 'static;
    type MarkFuture: Future<Item = Self, Error = (Self::Error, Self)> + Send + 'static;

    fn get_item(self) -> Self::GetItemFuture;

    fn mark_success(self, item: Self::Item) -> Self::MarkFuture;
    fn mark_failure(self, item: Self::Item) -> Self::MarkFuture;

    fn add(self, event: String, data: String) -> Self::MarkFuture;
}

impl QueueItem for () {
    fn event(&self) -> &str {
        panic!();
    }

    fn data(&self) -> &str {
        panic!();
    }
}

use futures::future;
use std::num::ParseIntError;
impl QueueStore for () {
    type Item = ();
    type Error = ParseIntError;
    type GetItemFuture =
        Box<Future<Item = (Option<Self::Item>, Self), Error = (Self::Error, Self)> + Send>;
    type MarkFuture = Box<Future<Item = Self, Error = (Self::Error, Self)> + Send>;

    fn get_item(self) -> Self::GetItemFuture {
        Box::new(future::ok((None, ())))
    }

    fn mark_success(self, item: Self::Item) -> Self::MarkFuture {
        Box::new(future::ok(()))
    }

    fn mark_failure(self, item: Self::Item) -> Self::MarkFuture {
        Box::new(future::ok(()))
    }

    fn add(self, event: String, data: String) -> Self::MarkFuture {
        Box::new(future::ok(()))
    }
}
