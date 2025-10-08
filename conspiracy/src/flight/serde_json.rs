use serde_json::Value;

use crate::flight::Flattenable;

#[cfg(feature = "serde_json")]
impl Flattenable for Value {
    type Key = String;

    fn children_keys(&self) -> Option<impl Iterator<Item = &Self::Key>> {
        match self {
            Value::Object(map) => Some(map.keys()),
            _ => None,
        }
    }

    fn children_values_mut(&mut self) -> Option<impl Iterator<Item = &mut Self>> {
        match self {
            Value::Object(map) => Some(map.iter_mut().map(|(_, v)| v)),
            _ => None,
        }
    }

    fn promote_child(&mut self, key: &Self::Key) {
        if let Value::Object(self_map) = self
            && let Some(child) = self_map.remove(key)
            && let Value::Object(child) = child
        {
            for (k, v) in child {
                self_map.insert(k, v);
            }
        }
    }

    fn drop_child(&mut self, key: &Self::Key) {
        if let Value::Object(map) = self {
            map.remove(key);
        }
    }
}
