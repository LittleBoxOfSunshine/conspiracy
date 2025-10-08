use std::{collections::HashMap, hash::Hash};

use pest::{Parser, iterators::Pair};

use crate::flight::{Evaluation, Evaluator};

#[derive(Default)]
pub struct CargoLikeEvaluator {}

#[derive(Debug)]
enum Predicate {
    All(Vec<Predicate>),
    Any(Vec<Predicate>),
    Not(Box<Predicate>),
    MatchValue {
        ident: String,
        value: Option<String>,
    },
}

impl Predicate {
    fn evaluate(&self, env: &HashMap<String, String>) -> bool {
        match self {
            Predicate::All(children) => children.iter().all(|child| child.evaluate(env)),
            Predicate::Any(children) => children.iter().any(|child| child.evaluate(env)),
            Predicate::Not(child) => !child.evaluate(env),
            Predicate::MatchValue { ident, value } => match (env.get(ident), value) {
                (None, _) => false,
                // If no value is specified, just check for existence
                (Some(_env_val), None) => true,
                (Some(env_val), Some(expected_val)) => env_val == expected_val,
            },
        }
    }
}

impl CargoLikeEvaluator {
    fn parse_cfg_expr(pair: Pair<Rule>) -> Predicate {
        match pair.as_rule() {
            Rule::all => {
                let children = pair.into_inner().map(Self::parse_cfg_expr).collect();
                Predicate::All(children)
            }
            Rule::any => {
                let children = pair.into_inner().map(Self::parse_cfg_expr).collect();
                Predicate::Any(children)
            }
            Rule::not => {
                let child = pair.into_inner().next();
                let child = child.expect("not always has 1 child predicate");

                Predicate::Not(Box::new(Self::parse_cfg_expr(child)))
            }
            Rule::match_value => {
                let mut inner = pair.into_inner();

                Predicate::MatchValue {
                    ident: {
                        let pair = inner.next().expect("match_value always has ident");
                        match pair.as_rule() {
                            Rule::ident => pair.as_str().to_string(),
                            _ => {
                                panic!("Unexpected rule: {:?}", pair.as_rule());
                            }
                        }
                    },
                    value: {
                        inner.next().map(|pair| match pair.as_rule() {
                            Rule::value_string => pair.as_str().trim_matches('"').to_string(),
                            _ => {
                                panic!("Unexpected rule: {:?}", pair.as_rule());
                            }
                        })
                    },
                }
            }
            Rule::ident => Predicate::MatchValue {
                ident: pair.as_str().to_string(),
                value: None,
            },
            // Silent rules + cfg_expr (which was already parsed at entry level)
            // ident and value_string are hit, but as inner details of match_value.
            _ => {
                panic!("Unexpected rule: {:?}", pair.as_rule());
            }
        }
    }
}

#[derive(pest_derive::Parser)]
#[grammar = "cargo_like_cfg.pest"]
struct CargoCfgLikeParser;

impl Evaluator for CargoLikeEvaluator {
    type Key = String;
    type Environment = HashMap<String, String>;
    type Error = pest::error::Error<Rule>;

    fn evaluate(
        &self,
        environment: &Self::Environment,
        key: &Self::Key,
    ) -> Evaluation<Self::Error> {
        if key.starts_with("cfg(") {
            match CargoCfgLikeParser::parse(Rule::cfg_expr, key) {
                Ok(mut pairs) => {
                    let pair = pairs.next().expect("cfg_expr always has 1 child predicate");
                    Evaluation::Conditional(Ok(Self::parse_cfg_expr(pair).evaluate(environment)))
                }
                Err(e) => Evaluation::Conditional(Err(e)),
            }
        } else {
            Evaluation::Unconditional
        }
    }
}
