use alloc::string::{String, ToString};
use alloc::vec::Vec;

use common::errors::*;
use pkix::PKIX1Implicit88;

/// Constraints on the names a certificate and its children can use.
///
/// Only DNS name constraints are supported.
///
/// A constraint will be of the form "example.com" which matches "example.com",
/// "host.example.com", but does not match "example.net" or "bigexample.com"
///
/// A name must be present in one of the 'permitted_subtrees' and not be present
/// in one of the 'excluded_subtrees' to be valid.
pub struct NameConstraints {
    /// A value of None implies all names are allowed.
    permitted_subtrees: Option<Vec<String>>,

    /// A value of None implies that no names are explicitly disallowed.
    excluded_subtrees: Option<Vec<String>>,
}

impl NameConstraints {
    pub fn create(extension: &PKIX1Implicit88::NameConstraints) -> Result<Self> {
        let mut permitted_subtrees = Self::make_subtree_vec(&extension.permittedSubtrees)?;
        let mut excluded_subtrees = Self::make_subtree_vec(&extension.excludedSubtrees)?;

        let len = permitted_subtrees.as_ref().map(|v| v.len()).unwrap_or(0)
            + excluded_subtrees.as_ref().map(|v| v.len()).unwrap_or(0);
        if len == 0 {
            return Err(err_msg("Empty name constraints not allowed"));
        }

        let mut instance = Self {
            permitted_subtrees,
            excluded_subtrees,
        };

        instance.simplify();

        Ok(instance)
    }

    fn simplify(&mut self) {
        // Temporarily remove all exclusions from self.
        let mut excluded_subtrees = None;
        std::mem::swap(&mut excluded_subtrees, &mut self.excluded_subtrees);

        // Simplify the excluded_subtrees list by only including subtrees not already
        // restricted by permitted_subtrees.
        if let Some(subtrees) = &mut excluded_subtrees {
            subtrees.retain(|subtree| self.is_allowed_dns_name(subtree));
        }

        self.excluded_subtrees = excluded_subtrees;
    }

    fn make_subtree_vec(
        general_subtrees: &Option<PKIX1Implicit88::GeneralSubtrees>,
    ) -> Result<Option<Vec<String>>> {
        let subtrees = match general_subtrees {
            Some(v) => v,
            None => return Ok(None),
        };

        let mut out = vec![];

        for subtree in &subtrees.items {
            // These have fixed values defined in the RFC.
            if !subtree.minimum.is_zero() || subtree.maximum.is_some() {
                return Err(err_msg("Unexpected min/max values for subtree contraint"));
            }

            if let PKIX1Implicit88::GeneralName::dNSName(s) = &subtree.base {
                out.push(s.to_string());
            } else {
                return Err(err_msg("Only DNS name constraints supported"));
            }
        }

        Ok(Some(out))
    }

    /// Checks if the given DNS name is allowed based on these constraints.
    pub fn is_allowed_dns_name(&self, name: &str) -> bool {
        let mut allow = false;
        if let Some(permitted) = &self.permitted_subtrees {
            for subtree in permitted {
                if Self::name_in_subtree(name, subtree) {
                    allow = true;
                    break;
                }
            }
        } else {
            allow = true;
        }

        if let Some(excluded) = &self.excluded_subtrees {
            for subtree in excluded {
                if Self::name_in_subtree(name, subtree) {
                    allow = false;
                    break;
                }
            }
        }

        allow
    }

    fn name_in_subtree(name: &str, subtree: &str) -> bool {
        if let Some(prefix) = name.strip_suffix(subtree) {
            prefix.is_empty() || prefix.ends_with('.')
        } else {
            false
        }
    }

    /// Applies constraints from the parent certificate to this set of
    /// contraints.
    ///
    /// - permitted_subtrees will become the intersection of the child and
    ///   parent allowed subtrees.
    /// - excluded_subtrees will become a union.
    pub fn inherit(&mut self, parent_constraints: &NameConstraints) {
        if let Some(permitted) = &mut self.permitted_subtrees {
            // Remove anything not allowed in the parent.
            permitted.retain(|subtree| parent_constraints.is_allowed_dns_name(subtree));
        } else {
            self.permitted_subtrees = parent_constraints.permitted_subtrees.clone();
        }

        if let Some(excluded) = &mut self.excluded_subtrees {
            if let Some(parent_excluded) = &parent_constraints.excluded_subtrees {
                excluded.extend_from_slice(&parent_excluded);
            }
        } else {
            self.excluded_subtrees = parent_constraints.excluded_subtrees.clone();
        }

        self.simplify();
    }
}
