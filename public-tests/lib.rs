use assignment_1_solution::{PlainSender, PlainSenderMessage, StableStorage};
use crossbeam_channel::Sender;
use std::collections::HashMap;
use uuid::Uuid;

pub struct RamStorage {
    map: HashMap<String, Vec<u8>>,
}

impl RamStorage {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
}

impl Default for RamStorage {
    fn default() -> Self {
        RamStorage::new()
    }
}

impl StableStorage for RamStorage {
    fn put(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
        self.map.insert(key.to_string(), value.to_vec());
        Ok(())
    }

    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.map.get(key).cloned()
    }
}

#[derive(Clone)]
pub struct TestSender {
    pub tx: Sender<Uuid>,
}

impl PlainSender for TestSender {
    fn send_to(&self, uuid: &Uuid, _msg: PlainSenderMessage) {
        self.tx.send(*uuid).unwrap();
    }
}
