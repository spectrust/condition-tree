//! Arbitrarily combinable boolean conditions in JSON
//!
//! You can use this to enable parsing arbitrary boolean combinations for
//! anything that implements Deserialize.

use std::future::Future;

use futures::{
    future::{self, BoxFuture},
    FutureExt, StreamExt, TryStreamExt,
};
use serde::{Deserialize, Serialize};

/// Base case for recursive condition specification.
///
/// Either a single condition (`Just(condition)`) or a combinatoric operator
/// applied to some condition (in the case of `not`) or some combination of
/// conditions (in the case of `any` and `all`).
#[derive(Debug, Deserialize, Serialize, Clone)]
#[cfg_attr(debug_assertions, derive(PartialEq, Eq))]
#[serde(rename_all = "snake_case", from = "ConditionIntermediate<T>")]
pub enum Condition<T> {
    Just(T),
    Any(Box<[Condition<T>]>),
    All(Box<[Condition<T>]>),
    Not(Box<Condition<T>>),
}

impl<R, T> ResolvableCondition<R, T> for Condition<T>
where
    R: ConditionResolver<T> + Send + Sync,
    T: Send + Sync,
{
    fn resolve<'a>(&'a self, resolver: &'a R) -> BoxFuture<'a, Result<bool, R::Error>> {
        async move {
            match self {
                // base case
                Condition::Just(condition) => resolver.resolve_condition(condition).boxed().await,
                Condition::Any(conditions) => {
                    futures::stream::iter(conditions)
                        .then(|condition| async { condition.resolve(resolver).await })
                        .try_any(future::ready)
                        .await
                }
                Condition::All(conditions) => {
                    futures::stream::iter(conditions)
                        .then(|condition| async { condition.resolve(resolver).await })
                        .try_all(future::ready)
                        .await
                }
                Condition::Not(condition) => {
                    condition.resolve(resolver).map(|res| res.map(|v| !v)).await
                }
            }
        }
        .boxed()
    }
}

impl<T> Condition<T> {
    /// Get an iterator over all individual conditions, without boolean context
    pub fn flat_conditions(&self) -> impl Iterator<Item = &T> {
        match self {
            // base case
            Condition::Just(condition) => vec![condition],
            Condition::Any(conditions) => conditions
                .iter()
                .flat_map(|c| c.flat_conditions())
                .collect(),
            Condition::All(conditions) => conditions
                .iter()
                .flat_map(|c| c.flat_conditions())
                .collect(),
            Condition::Not(condition) => condition.flat_conditions().collect(),
        }
        .into_iter()
    }

    // Helper function for tests
    #[cfg(debug_assertions)]
    pub fn all(conditions: impl IntoIterator<Item = Condition<T>>) -> Self {
        Condition::All(
            conditions
                .into_iter()
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )
    }

    // Helper function for tests
    #[cfg(debug_assertions)]
    pub fn any(conditions: impl IntoIterator<Item = Condition<T>>) -> Self {
        Condition::Any(
            conditions
                .into_iter()
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )
    }
}

// Helper function for tests
#[cfg(debug_assertions)]
impl<T> From<T> for Condition<T> {
    fn from(value: T) -> Self {
        Self::Just(value)
    }
}

/// Resolve a single base condition
pub trait ConditionResolver<T> {
    type Error;

    fn resolve_condition(
        &self,
        condition: &T,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

pub trait ResolvableCondition<R, T>
where
    R: ConditionResolver<T>,
{
    /// Resolve a condition, using the passed resolver.
    ///
    /// This returns a boxed future, because the current iteration of the trait
    /// solver can't deal with the recursive bounds when returning an impl Future.
    fn resolve<'a>(
        &'a self,
        resolver: &'a R,
    ) -> BoxFuture<'a, Result<bool, <R as ConditionResolver<T>>::Error>>;
}

/// Intermediate struct representing the JSON structure of conditions, which allows
/// any of:
///
/// - a single session condition, e.g. {"session": "is_new"} or {"event": "has_workflow"}
/// - an array of conditions, which implies "all" combinatorial logic, e.g.:
///   [{"session": "is_new"}, {"event": "has_workflow"}]
/// - an explicit boolean combination, e.g.:
///   {"any": [{"session": "is_new"}, {"event": "has_workflow"}]}
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case", untagged)]
pub enum ConditionIntermediate<T> {
    Just(T),
    ImplicitAll(Box<[Condition<T>]>),
    Any { any: Box<[Condition<T>]> },
    All { all: Box<[Condition<T>]> },
    Not { not: Box<Condition<T>> },
}

impl<T> From<ConditionIntermediate<T>> for Condition<T> {
    fn from(value: ConditionIntermediate<T>) -> Self {
        match value {
            ConditionIntermediate::Just(condition) => Self::Just(condition),
            // All or Any with a single item can deserialize to the item
            ConditionIntermediate::ImplicitAll(array)
            | ConditionIntermediate::All { all: array }
            | ConditionIntermediate::Any { any: array }
                if array.len() == 1 =>
            {
                array.into_vec().pop().expect("array has one item")
            }
            ConditionIntermediate::ImplicitAll(all) => Self::All(all),
            ConditionIntermediate::Any { any } => Self::Any(any),
            ConditionIntermediate::All { all } => Self::All(all),
            ConditionIntermediate::Not { not } => Self::Not(not),
        }
    }
}

#[cfg(test)]
mod test_conditions_deser {
    use super::*;
    use serde_json::json;

    #[derive(Deserialize, PartialEq, Debug)]
    enum Cond {
        A,
        B,
    }

    #[test]
    fn condition_deser() {
        let cases = [
            (
                json!({"all": ["A"], "comment": "foo"}),
                Condition::Just(Cond::A),
            ),
            (
                json!({"all": [{"all": ["A"]}], "comment": "foo"}),
                Condition::Just(Cond::A),
            ),
            (
                json!({"all": [{"any": ["A"]}], "comment": "foo"}),
                Condition::Just(Cond::A),
            ),
            (
                json!({"all": [{"any": [{"any": ["A"]}]}], "comment": "foo"}),
                Condition::Just(Cond::A),
            ),
            (json!(["A"]), Condition::Just(Cond::A)),
            (
                json!({"all": ["A", "B"], "comment": "foo"}),
                Condition::All(Box::new([
                    Condition::Just(Cond::A),
                    Condition::Just(Cond::B),
                ])),
            ),
            (
                json!({"all": [{"any": [{"any": ["A", "B"]}]}], "comment": "foo"}),
                Condition::Any(Box::new([
                    Condition::Just(Cond::A),
                    Condition::Just(Cond::B),
                ])),
            ),
            (
                json!({"all": [{"any": [{"any": ["A"]}, "A"]}], "comment": "foo"}),
                Condition::Any(Box::new([
                    Condition::Just(Cond::A),
                    Condition::Just(Cond::A),
                ])),
            ),
            (
                json!(["A", "B"]),
                Condition::All(Box::new([
                    Condition::Just(Cond::A),
                    Condition::Just(Cond::B),
                ])),
            ),
            (
                json!({"all": ["A", "B"], "comment": "foo"}),
                Condition::All(Box::new([
                    Condition::Just(Cond::A),
                    Condition::Just(Cond::B),
                ])),
            ),
            (
                json!({"any": ["A"], "comment": "foo"}),
                Condition::Just(Cond::A),
            ),
            (
                json!({"any": ["A", "B"], "comment": "foo"}),
                Condition::Any(Box::new([
                    Condition::Just(Cond::A),
                    Condition::Just(Cond::B),
                ])),
            ),
            (
                json!({"all": [], "comment": "foo"}),
                Condition::All(Box::new([])),
            ),
            (
                json!({"all": [{"all": []}], "comment": "foo"}),
                Condition::All(Box::new([])),
            ),
            (json!([{"all": [[]]}]), Condition::All(Box::new([]))),
            (
                json!([{"all": [{"all": []}]}]),
                Condition::All(Box::new([])),
            ),
            (
                json!({"any": [], "comment": "foo"}),
                Condition::Any(Box::new([])),
            ),
            (
                json!({"not": [], "comment": "foo"}),
                Condition::Not(Box::new(Condition::All(Box::new([])))),
            ),
            (
                json!({"not": "A", "comment": "foo"}),
                Condition::Not(Box::new(Condition::Just(Cond::A))),
            ),
            (
                json!({"not": {"all": ["A"]}, "comment": "foo"}),
                Condition::Not(Box::new(Condition::Just(Cond::A))),
            ),
            (
                json!({"not": {"any": ["A"]}, "comment": "foo"}),
                Condition::Not(Box::new(Condition::Just(Cond::A))),
            ),
            (
                json!({"not": {"any": ["A", "B"]}, "comment": "foo"}),
                Condition::Not(Box::new(Condition::Any(Box::new([
                    Condition::Just(Cond::A),
                    Condition::Just(Cond::B),
                ])))),
            ),
        ];

        for (val, exp) in cases {
            dbg!(&val, &exp);

            let res = serde_json::from_value(val).unwrap();

            assert_eq!(exp, res);
        }
    }

    #[derive(Debug)]
    struct TestCondition;

    #[derive(Debug)]
    struct TrueResolver;
    impl ConditionResolver<TestCondition> for TrueResolver {
        type Error = &'static str;

        async fn resolve_condition(&self, _condition: &TestCondition) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    #[derive(Debug)]
    struct ErrResolver;
    impl ConditionResolver<TestCondition> for ErrResolver {
        type Error = &'static str;

        async fn resolve_condition(&self, _condition: &TestCondition) -> Result<bool, Self::Error> {
            Err("error")
        }
    }

    #[derive(Debug)]
    enum Resolver {
        True(TrueResolver),
        Err(ErrResolver),
    }

    #[tokio::test]
    async fn test_condition_resolution() {
        // condition, resolver, expected result
        let cases = [
            (
                Condition::Just(TestCondition),
                Resolver::True(TrueResolver),
                Ok(true),
            ),
            (
                Condition::Any(Box::new([
                    Condition::Just(TestCondition),
                    Condition::Just(TestCondition),
                ])),
                Resolver::True(TrueResolver),
                Ok(true),
            ),
            (
                Condition::Any(Box::new([
                    Condition::Not(Box::new(Condition::Just(TestCondition))),
                    Condition::Not(Box::new(Condition::Just(TestCondition))),
                    Condition::Not(Box::new(Condition::Just(TestCondition))),
                    Condition::Not(Box::new(Condition::Not(Box::new(Condition::Just(
                        TestCondition,
                    ))))),
                ])),
                Resolver::True(TrueResolver),
                Ok(true),
            ),
            (
                Condition::All(Box::new([
                    Condition::Just(TestCondition),
                    Condition::Just(TestCondition),
                ])),
                Resolver::True(TrueResolver),
                Ok(true),
            ),
            (
                Condition::Not(Box::new(Condition::Just(TestCondition))),
                Resolver::True(TrueResolver),
                Ok(false),
            ),
            (
                Condition::All(Box::new([
                    Condition::Any(Box::new([Condition::Just(TestCondition)])),
                    Condition::Just(TestCondition),
                ])),
                Resolver::True(TrueResolver),
                Ok(true),
            ),
            (
                Condition::All(Box::new([
                    Condition::Any(Box::new([Condition::Just(TestCondition)])),
                    Condition::Just(TestCondition),
                    Condition::Not(Box::new(Condition::All(Box::new([Condition::Just(
                        TestCondition,
                    )])))),
                ])),
                Resolver::True(TrueResolver),
                Ok(false),
            ),
            (
                Condition::Just(TestCondition),
                Resolver::Err(ErrResolver),
                Err("error"),
            ),
            (
                Condition::Any(Box::new([
                    Condition::Just(TestCondition),
                    Condition::Just(TestCondition),
                ])),
                Resolver::Err(ErrResolver),
                Err("error"),
            ),
            (
                Condition::All(Box::new([
                    Condition::Just(TestCondition),
                    Condition::Just(TestCondition),
                ])),
                Resolver::Err(ErrResolver),
                Err("error"),
            ),
            (
                Condition::Not(Box::new(Condition::Just(TestCondition))),
                Resolver::Err(ErrResolver),
                Err("error"),
            ),
        ];

        for (condition, resolver, exp) in &cases {
            dbg!(condition, resolver, exp);

            let res: Result<bool, &'static str> = match resolver {
                Resolver::True(r) => condition.resolve(r).await,
                Resolver::Err(r) => condition.resolve(r).await,
            };
            assert_eq!(exp, &res);
        }
    }
}
