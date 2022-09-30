use std::collections::HashMap;

#[derive(Default)]
pub struct TactKeys {
    keys: HashMap<[u8; 8], [u8; 16]>,
}

impl TactKeys {
    pub fn get_key(&self, key_name: &[u8]) -> Option<&[u8; 16]> {
        self.keys.get(key_name)
    }

    pub fn add_key(&mut self, key_name: [u8; 8], key: [u8; 16]) {
        self.keys.insert(key_name, key);
    }
}
