//! Traits for all things that have to do with storing and retrieving entities.

use crate::entity::StoreItem;

use std::error::Error;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

use crate::QuadQuery;

pub type StoreError = Box<dyn Error + Send + Sync + 'static>;

/// An entity store, storing JSON-LD `Entity` objects.
#[async_trait::async_trait]
pub trait EntityStore: Debug + Send {
    /// Gets a single `StoreItem` from the store. Missing entities are no error,
    /// but instead returns a `None`.
    async fn get(&mut self, path: String, local: bool) -> Result<Option<StoreItem>, StoreError>;

    /// Stores a single `StoreItem` into the store.
    ///
    /// To delete an Entity, set its type to as:Tombstone. This may
    /// instantly remove it, or queue it for possible future deletion.
    async fn put(&mut self, path: String, item: &mut StoreItem) -> Result<(), StoreError>;

    /// Queries the entire store for a specific set of parameters.
    /// The return value is a list for every result in the database that matches the query.
    /// The array elements are in numeric order of the placeholders.
    async fn query(&mut self, query: Vec<QuadQuery>) -> Result<Vec<Vec<String>>, StoreError>;

    /// Reads N amount of items from the collection corresponding to a specific ID. If a cursor is passed,
    /// it can be used to paginate.
    async fn read_collection(
        &mut self,
        path: String,
        count: Option<u32>,
        cursor: Option<String>,
    ) -> Result<CollectionPointer, StoreError>;

    /// Finds an item in a collection. The result will contain cursors to just before and after the item, if it exists.
    async fn find_collection(
        &mut self,
        path: String,
        item: String,
    ) -> Result<CollectionPointer, StoreError>;

    /// Inserts an item into the back of the collection.
    async fn insert_collection(&mut self, path: String, item: String) -> Result<(), StoreError>;

    /// Finds all the collections containing a specific object.
    async fn read_collection_inverse(
        &mut self,
        item: String,
    ) -> Result<CollectionPointer, StoreError>;

    /// Removes an item from the collection.
    async fn remove_collection(&mut self, path: String, item: String) -> Result<(), StoreError>;
}

// Manually implement async_trait to save an indirection; this will hopefully be inlined.
impl EntityStore for &mut dyn EntityStore {
    fn get<'a: 'res, 'res>(
        &'a mut self,
        path: String,
        local: bool,
    ) -> Pin<Box<dyn Future<Output = Result<Option<StoreItem>, StoreError>> + Send + 'res>>
    where
        Self: 'res,
    {
        (**self).get(path, local)
    }

    fn put<'a: 'res, 'b: 'res, 'res>(
        &'a mut self,
        path: String,
        item: &'b mut StoreItem,
    ) -> Pin<Box<dyn Future<Output = Result<(), StoreError>> + Send + 'res>>
    where
        Self: 'res,
    {
        (**self).put(path, item)
    }

    fn query<'a: 'res, 'res>(
        &'a mut self,
        query: Vec<QuadQuery>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Vec<String>>, StoreError>> + Send + 'res>>
    where
        Self: 'res,
    {
        (**self).query(query)
    }

    fn read_collection<'a: 'res, 'res>(
        &'a mut self,
        path: String,
        count: Option<u32>,
        cursor: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<CollectionPointer, StoreError>> + Send + 'res>>
    where
        Self: 'res,
    {
        (**self).read_collection(path, count, cursor)
    }

    fn find_collection<'a: 'res, 'res>(
        &'a mut self,
        path: String,
        item: String,
    ) -> Pin<Box<dyn Future<Output = Result<CollectionPointer, StoreError>> + Send + 'res>>
    where
        Self: 'res,
    {
        (**self).find_collection(path, item)
    }

    fn insert_collection<'a: 'res, 'res>(
        &'a mut self,
        path: String,
        item: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), StoreError>> + Send + 'res>>
    where
        Self: 'res,
    {
        (**self).insert_collection(path, item)
    }

    fn read_collection_inverse<'a: 'res, 'res>(
        &'a mut self,
        item: String,
    ) -> Pin<Box<dyn Future<Output = Result<CollectionPointer, StoreError>> + Send + 'res>>
    where
        Self: 'res,
    {
        (**self).read_collection_inverse(item)
    }

    fn remove_collection<'a: 'res, 'res>(
        &'a mut self,
        path: String,
        item: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), StoreError>> + Send + 'res>>
    where
        Self: 'res,
    {
        (**self).remove_collection(path, item)
    }
}

#[derive(Debug)]
pub struct CollectionPointer {
    pub items: Vec<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub count: Option<u32>,
}

#[derive(Debug)]
pub struct QueueItem {
    pub id: u64,
    pub event: String,
    pub data: String,
}

#[async_trait::async_trait]
pub trait QueueStore: Debug + Send {
    async fn get_item(&mut self) -> Result<Option<QueueItem>, StoreError>;

    async fn mark_success(&mut self, item: QueueItem) -> Result<(), StoreError>;
    async fn mark_failure(&mut self, item: QueueItem) -> Result<(), StoreError>;

    async fn add(&mut self, event: String, data: String) -> Result<(), StoreError>;
}

impl QueueStore for &mut dyn QueueStore {
    fn get_item<'a: 'res, 'res>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<QueueItem>, StoreError>> + Send + 'res>> {
        (**self).get_item()
    }

    fn mark_success<'a: 'res, 'res>(
        &'a mut self,
        item: QueueItem,
    ) -> Pin<Box<dyn Future<Output = Result<(), StoreError>> + Send + 'res>> {
        (**self).mark_success(item)
    }

    fn mark_failure<'a: 'res, 'res>(
        &'a mut self,
        item: QueueItem,
    ) -> Pin<Box<dyn Future<Output = Result<(), StoreError>> + Send + 'res>> {
        (**self).mark_failure(item)
    }

    fn add<'a: 'res, 'res>(
        &'a mut self,
        event: String,
        data: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), StoreError>> + Send + 'res>> {
        (**self).add(event, data)
    }
}

#[async_trait::async_trait]
impl QueueStore for () {
    async fn get_item(&mut self) -> Result<Option<QueueItem>, StoreError> {
        Err("hewwo".into())
    }

    async fn mark_success(&mut self, _: QueueItem) -> Result<(), StoreError> {
        Err("hewwo".into())
    }

    async fn mark_failure(&mut self, _: QueueItem) -> Result<(), StoreError> {
        Err("hewwo".into())
    }

    async fn add(&mut self, _: String, _: String) -> Result<(), StoreError> {
        Err("hewwo".into())
    }
}
