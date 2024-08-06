#![no_std]

use core::time::Duration;

use iroha_data_model::{
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
}

#[derive(Deserialize, Serialize)]
pub struct BondDetails {
    pub currency: AssetDefinitionId,
    /// Value of the bond
    pub nominal_value: Numeric,
    // WARN: Coupon rate can be changed by the issuer
    // after the bond is issued (not required, for now)
    /// Yearly coupon rate
    pub coupon_rate: Numeric,
    /// How many bonds to issue
    pub quantity: Numeric,

    pub maturation_date: Duration,
    pub registration_time: Duration,
    pub payment_frequency: Duration,

    // WARN: Can it be changed by the issuer like coupon_rate?
    // NOTE: fixed fee is an absolute value, i.e 0.1$.
    // Should it be a percentage of the nominal value?
    pub fee: Numeric,
    pub fee_beneficiary: AccountId,
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
