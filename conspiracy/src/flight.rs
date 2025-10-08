use std::error::Error;

#[cfg(feature = "cfg_flights")]
pub mod cargo_like;

#[cfg(feature = "serde_json")]
pub mod serde_json;

#[cfg(feature = "toml")]
pub mod toml;

pub enum Evaluation<Error> {
    Unconditional,
    Conditional(Result<bool, Error>),
}

// TODO: Move all traits to theories crate
pub trait Evaluator {
    type Key: Clone;
    type Environment;
    type Error: Error + Send + Sync + 'static;

    // TODO: should this be asref?
    fn evaluate(&self, environment: &Self::Environment, key: &Self::Key)
    -> Evaluation<Self::Error>;
}

pub trait Flattenable {
    type Key: Clone;

    fn children_keys(&self) -> Option<impl Iterator<Item = &Self::Key>>;
    fn children_values_mut(&mut self) -> Option<impl Iterator<Item = &mut Self>>;

    fn promote_child(&mut self, key: &Self::Key);
    fn drop_child(&mut self, key: &Self::Key);
}

pub trait Merge {
    fn overriding_merge(&mut self, other: Self);
}

pub(crate) struct Flattener<E> {
    evaluator: E,
}

impl<E: Evaluator> Flattener<E> {
    pub fn new(evaluator: E) -> Self {
        Self { evaluator }
    }

    pub fn flatten<V: Flattenable<Key = E::Key>>(
        &self,
        environment: &E::Environment,
        node: &mut V,
    ) -> Result<(), E::Error> {
        let mut to_drop = Vec::new();
        let mut to_promote = Vec::new();

        if let Some(keys) = node.children_keys() {
            for child in keys {
                match self.evaluator.evaluate(environment, child) {
                    Evaluation::Unconditional => {}
                    Evaluation::Conditional(result) => match result {
                        Ok(applies) => {
                            let child = child.clone();
                            if applies {
                                to_promote.push(child);
                            } else {
                                to_drop.push(child);
                            }
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    },
                }
            }
        }

        for child in to_drop.iter() {
            node.drop_child(child);
        }
        for child in to_promote.iter() {
            node.promote_child(child);
        }

        // TODO: After tests are written, remove the recursion
        if let Some(values) = node.children_values_mut() {
            for child in values {
                self.flatten(environment, child)?;
            }
        }

        Ok(())
    }
}
