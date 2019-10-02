use kroeg_tap::{
    CollectionPointer, Context, EntityStore, QuadQuery, QueueStore, StoreError, StoreItem, User,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct TestStore {
    data: HashMap<String, StoreItem>,
    items: HashMap<String, Vec<String>>,
    reads: HashSet<String>,
}

#[async_trait::async_trait]
impl EntityStore for TestStore {
    async fn get(&mut self, path: String, local: bool) -> Result<Option<StoreItem>, StoreError> {
        println!("store: get {} (local: {})", path, local);
        self.reads.insert(path.to_owned());

        Ok(self.data.get(&path).cloned())
    }

    async fn put(&mut self, path: String, item: &mut StoreItem) -> Result<(), StoreError> {
        println!("store: put {}", path);
        self.data.insert(path, item.clone());

        Ok(())
    }

    async fn query(&mut self, _query: Vec<QuadQuery>) -> Result<Vec<Vec<String>>, StoreError> {
        Err("not implemented".into())
    }

    async fn read_collection(
        &mut self,
        path: String,
        _count: Option<u32>,
        _cursor: Option<String>,
    ) -> Result<CollectionPointer, StoreError> {
        println!("store: read collection {}", path);

        Err("not implemented".into())
    }

    async fn find_collection(
        &mut self,
        _path: String,
        _item: String,
    ) -> Result<CollectionPointer, StoreError> {
        Err("not implemented".into())
    }

    async fn read_collection_inverse(
        &mut self,
        _item: String,
    ) -> Result<CollectionPointer, StoreError> {
        Err("not implemented".into())
    }

    async fn insert_collection(&mut self, path: String, item: String) -> Result<(), StoreError> {
        println!("store: insert collection {}, item {}", path, item);
        if let None = self.items.get(&path) {
            self.items.insert(path.to_owned(), vec![]);
        }

        let list = self.items.get_mut(&path).unwrap();
        if !list.contains(&item) {
            list.push(item);
        }

        Ok(())
    }

    async fn remove_collection(&mut self, path: String, item: String) -> Result<(), StoreError> {
        println!("store: remove collection {}, item {}", path, item);
        if let None = self.items.get(&path) {
            return Ok(());
        }

        let list = self.items.get_mut(&path).unwrap();
        if let Some(index) = list.iter().position(|f| f == &item) {
            list.remove(index);
        }

        Ok(())
    }
}

impl TestStore {
    pub fn new(data: Vec<StoreItem>) -> TestStore {
        TestStore {
            data: data.into_iter().map(|f| (f.id().to_owned(), f)).collect(),
            items: HashMap::new(),
            reads: HashSet::new(),
        }
    }

    pub fn contains(&self, val: &str, item: &str) -> bool {
        self.items
            .get(&String::from(val))
            .map(|f| f.iter().any(|v| v == item))
            .unwrap_or(false)
    }

    pub fn has_read(&self, val: &str) -> bool {
        self.reads.contains(&String::from(val))
    }

    pub fn has_read_all(&self, val: &[&str]) -> bool {
        let mut result = true;

        for item in val {
            result = result && self.has_read(*item)
        }

        result
    }

    pub fn context<'a, 'b>(&'a mut self, queue: &'b mut dyn QueueStore) -> Context<'a, 'b> {
        Context {
            user: User {
                claims: HashMap::new(),
                issuer: None,
                subject: "/subject".to_owned(),
                audience: vec![],
                token_identifier: "test".to_owned(),
            },

            server_base: "".to_owned(),
            name: String::new(),
            description: String::new(),
            instance_id: 1,
            entity_store: self,
            queue_store: queue,
        }
    }
}

#[macro_export]
macro_rules! handle_object_pair {
    ($entity:expr, ) => {};

    ($entity:expr, types => $values:expr ; $($tail:tt)* ) => {{
        $entity.types.extend($values.iter().map(|f| f.to_string()));

        handle_object_pair!($entity, $($tail)*);
    }};

    ($entity:expr, $key:expr => $values:expr ; $($tail:tt)* ) => {{
        use jsonld::nodemap::Pointer;
        $entity.get_mut($key).extend($values.iter().map(|f| Pointer::Id(f.to_string())));

        handle_object_pair!($entity, $($tail)*);
    }};
}

#[macro_export]
macro_rules! object_under_test {
    (remote $name:expr => { $($pairs:tt)* }) => {{
        use jsonld::nodemap::Entity;
        use kroeg_tap::StoreItem;
        use std::collections::HashMap;

        let mut item = Entity::new($name.to_string());
        handle_object_pair! { item, $($pairs)* };

        let mut map = HashMap::new();
        map.insert($name.to_string(), item);

        StoreItem::new($name.to_string(), map)
    }};

    (local $name:expr => $pairs:tt ) => {{
        use jsonld::nodemap::{Pointer, Value};

        let mut item = object_under_test!(remote $name => $pairs);

        item.meta().get_mut("https://puckipedia.com/kroeg/ns#instance").push(
            Pointer::Value(Value {
                value: 1.into(),
                type_id: Some("http://www.w3.org/2001/XMLSchema#integer".to_owned()),
                language: None
            })
        );

        item
    }};
}
