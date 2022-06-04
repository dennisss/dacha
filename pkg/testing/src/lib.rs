#![no_std]

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

extern crate common;

use alloc::string::String;
use core::cmp::PartialEq;
use core::fmt::Debug;
use core::marker::PhantomData;
use std::collections::HashSet;

use common::line_builder::LineBuilder;

pub struct MatchResult {
    pub matches: bool,
    pub explanation: String,
}

pub trait Matcher<T> {
    fn check(&self, value: &T) -> MatchResult;
}

pub fn assert_that<T, M: Matcher<T>>(value: &T, matcher: M) {
    let r = matcher.check(value);
    if !r.matches {
        panic!("{}", r.explanation);
    }
}

pub fn eq<T: Debug + PartialEq>(expected_value: T) -> impl Matcher<T> {
    EqualMatcher { expected_value }
}

struct EqualMatcher<T> {
    expected_value: T,
}

impl<T: Debug + PartialEq> Matcher<T> for EqualMatcher<T> {
    fn check(&self, value: &T) -> MatchResult {
        if *value != self.expected_value {
            return MatchResult {
                matches: false,
                explanation: format!("left: {:?}\nright:{:?}\n", value, self.expected_value),
            };
        }

        MatchResult {
            matches: true,
            explanation: String::new(),
        }
    }
}

pub fn unordered_elements_are<'a, T: 'a + Debug, Ts: AsRef<[T]>, M: Matcher<T>>(
    element_matchers: &'a [M],
) -> impl Matcher<Ts> + 'a {
    UnorderedElementsAreMatcher {
        element_matchers,
        element_type: PhantomData,
    }
}

struct UnorderedElementsAreMatcher<'a, M, T> {
    element_matchers: &'a [M],
    element_type: PhantomData<T>,
}

impl<'a, T: Debug, Ts: AsRef<[T]>, M: Matcher<T>> Matcher<Ts>
    for UnorderedElementsAreMatcher<'a, M, T>
{
    fn check(&self, value: &Ts) -> MatchResult {
        let values = value.as_ref();

        if values.len() != self.element_matchers.len() {
            return MatchResult {
                matches: false,
                explanation: format!(
                    "Length. Expected: {}. Actual {}",
                    self.element_matchers.len(),
                    values.len()
                ),
            };
        }

        // Indices in 'values' which we have found corresponding matches for.
        let mut matched_values = HashSet::new();

        // Indices of matchers which
        let mut matched_matchers = HashSet::new();

        for i in 0..self.element_matchers.len() {
            for j in 0..values.len() {
                if matched_values.contains(&j) {
                    continue;
                }

                let r = self.element_matchers[i].check(&values[j]);
                if r.matches {
                    matched_matchers.insert(i);
                    matched_values.insert(j);
                }
            }
        }

        if matched_values.len() != values.len()
            || matched_matchers.len() != self.element_matchers.len()
        {
            let mut explanation = LineBuilder::new();

            explanation.add("Only on left:");
            for i in 0..values.len() {
                if matched_values.contains(&i) {
                    continue;
                }

                explanation.add(format!("{:#?}", values[i]));
            }

            explanation.add("Only on right:");
            explanation.add(format!("- num: {}", self.element_matchers.len() - matched_matchers.len()));

            return MatchResult {
                matches: false,
                explanation: explanation.to_string()
            }
        }

        MatchResult {
            matches: true,
            explanation: String::new(),
        }
    }
}
