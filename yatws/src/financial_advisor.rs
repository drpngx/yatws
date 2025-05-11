
//! Defines data structures for Financial Advisor (FA) configurations.
//!
//! These structures are used to represent FA groups, profiles, and aliases
//! as received from and sent to TWS.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// Enumerates the types of Financial Advisor data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum FADataType {
  Groups = 1,
  Profiles = 2,
  Aliases = 3,
}

impl FADataType {
  /// Converts an `i32` value from TWS into an `FADataType`.
  /// Returns `None` if the integer does not correspond to a known FA data type.
  pub fn from_i32(value: i32) -> Option<Self> {
    match value {
      1 => Some(FADataType::Groups),
      2 => Some(FADataType::Profiles),
      3 => Some(FADataType::Aliases),
      _ => None,
    }
  }

  /// Converts `FADataType` to its string representation for use in requests or logging.
  pub fn to_string_type(&self) -> &'static str {
    match self {
      FADataType::Groups => "GROUPS",
      FADataType::Profiles => "PROFILES",
      FADataType::Aliases => "ALIASES",
    }
  }
}

/// Represents an FA Group, which is a list of accounts with a default allocation method.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct FAGroup {
  pub name: String,
  pub default_method: String,
  pub accounts: Vec<String>, // List of account IDs
}

/// Represents an allocation within an FA Profile.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct FAProfileAllocation {
  pub account_id: String, // Account ID for this allocation
  pub amount: String,   // Allocation amount or ratio (can be percentage or ratio string)
}

/// Represents an FA Profile, which defines how trades are allocated across accounts.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct FAProfile {
  pub name: String,
  pub profile_type: String, // e.g., "Percent", "Ratio", "Shares"
  pub allocations: Vec<FAProfileAllocation>,
}

/// Represents an FA Alias, which maps a custom name to an account ID.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct FAAlias {
  pub alias: String,
  pub account_id: String,
}

/// Holds the complete Financial Advisor configuration.
#[derive(Debug, Default, Clone)]
pub struct FinancialAdvisorConfig {
  pub groups: HashMap<String, FAGroup>, // Keyed by group name
  pub profiles: HashMap<String, FAProfile>, // Keyed by profile name
  pub aliases: HashMap<String, FAAlias>, // Keyed by alias name
  pub last_updated_groups: Option<DateTime<Utc>>,
  pub last_updated_profiles: Option<DateTime<Utc>>,
  pub last_updated_aliases: Option<DateTime<Utc>>,
}
