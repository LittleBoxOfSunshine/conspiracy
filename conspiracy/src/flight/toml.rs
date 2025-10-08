use toml::Value;

use crate::flight::Flattenable;

impl Flattenable for Value {
    type Key = String;

    fn children_keys(&self) -> Option<impl Iterator<Item = &Self::Key>> {
        match self {
            Value::Table(table) => Some(table.keys()),
            _ => None,
        }
    }

    fn children_values_mut(&mut self) -> Option<impl Iterator<Item = &mut Self>> {
        match self {
            Value::Table(table) => Some(table.iter_mut().map(|(_, v)| v)),
            _ => None,
        }
    }

    fn promote_child(&mut self, key: &Self::Key) {
        if let Value::Table(self_table) = self
            && let Some(child) = self_table.remove(key)
            && let Value::Table(child) = child
        {
            for (k, v) in child {
                self_table.insert(k, v);
            }
        }
    }

    fn drop_child(&mut self, key: &Self::Key) {
        if let Value::Table(table) = self {
            table.remove(key);
        }
    }
}
