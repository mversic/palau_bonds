#![no_std]

use iroha_trigger::data_model::{
    account::AccountId,
    asset::{AssetDefinitionId, NewAssetDefinition},
    prelude::*,
};
use serde::{Deserialize, Serialize};

/// Arguments for `register_bond` trigger
#[derive(Deserialize, Serialize)]
pub struct RegisterBondArgs {
    /// Bond to be registered
    pub bond: NewAssetDefinition,
}

#[derive(Deserialize, Serialize)]
pub struct LogEntry {
    pub bond: AssetDefinitionId,
    pub amount: Numeric,
    pub quantity: Numeric,
}

#[derive(Deserialize, Serialize)]
pub struct BondDetails {
    pub currency: JsonString,
    /// Value of the bond
    pub nominal_value: Numeric,
    // WARN: Coupon rate can be changed by the issuer
    // after the bond is issued (not required, for now)
    /// Yearly coupon rate
    pub coupon_rate: Numeric,
    /// How many bonds to issue
    pub quantity: Numeric,

    pub maturation_date_sec: Numeric,
    pub registration_time_sec: Numeric,
    pub payment_frequency_sec: Numeric,

    // WARN: Can it be changed by the issuer like coupon_rate?
    // NOTE: fixed fee is an absolute value, i.e 0.1$.
    // Should it be a percentage of the nominal value?
    pub fee: Numeric,
    pub fee_beneficiary: JsonString,
}

pub trait FromJsonString: Sized {
    fn from_json_string(json_string: &JsonString) -> Result<Self, serde_json::Error>;
}

impl FromJsonString for Metadata {
    fn from_json_string(json_string: &JsonString) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_string.as_ref())
    }
}

impl FromJsonString for AssetDefinitionId {
    fn from_json_string(json_string: &JsonString) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_string.as_ref())
    }
}

impl FromJsonString for AccountId {
    fn from_json_string(json_string: &JsonString) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_string.as_ref())
    }
}

impl From<RegisterBondArgs> for JsonString {
    fn from(details: RegisterBondArgs) -> Self {
        JsonString::new(details)
    }
}

impl TryFrom<&JsonString> for RegisterBondArgs {
    type Error = serde_json::Error;

    fn try_from(payload: &JsonString) -> serde_json::Result<Self> {
        serde_json::from_str::<Self>(payload.as_ref())
    }
}

impl From<LogEntry> for JsonString {
    fn from(details: LogEntry) -> Self {
        JsonString::new(details)
    }
}

impl TryFrom<&JsonString> for LogEntry {
    type Error = serde_json::Error;

    fn try_from(payload: &JsonString) -> serde_json::Result<Self> {
        serde_json::from_str::<Self>(payload.as_ref())
    }
}

impl From<BondDetails> for JsonString {
    fn from(details: BondDetails) -> Self {
        JsonString::new(details)
    }
}

impl TryFrom<&JsonString> for BondDetails {
    type Error = serde_json::Error;

    fn try_from(payload: &JsonString) -> serde_json::Result<Self> {
        serde_json::from_str::<Self>(payload.as_ref())
    }
}
