//! Periodic time trigger for making interest payments
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use alloc::{borrow::ToOwned as _, format};

use dlmalloc::GlobalDlmalloc;
use iroha_trigger::{data_model::prelude::*, debug::dbg_panic};

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

#[iroha_trigger::main]
fn main(id: TriggerId, issuer: AccountId, event: Event) {
    let bond_id: AssetDefinitionId = id
        .name()
        .as_ref()
        .strip_suffix("%%interest_payments")
        .dbg_expect("INTERNAL BUG: Trigger name must end with `%%interest_payments`")
        .replace("%%", "#")
        .parse()
        .dbg_expect(
            "INTERNAL BUG: Unable to parse bond id from trigger name prefix.
            Prefix trigger name with the id of the bond it's registered for",
        );

    if !matches!(event, Event::Time(_)) {
        dbg_panic(
            "INTERNAL BUG: Triggering event is not TimeEvent.
            To avoid this error, register the trigger using the correct filter",
        );
    }

    let bond = FindAssetDefinitionById::new(bond_id.clone())
        .execute()
        .dbg_expect(&format!("{bond_id}: Bond not found"));
    let issued_bonds = FindAssetsByAssetDefinitionId::new(bond_id.clone())
        .execute()
        .dbg_expect(&format!("{bond_id}: Bond not found"));

    // WARN: Coupon rate can be changed by the issuer after the bond is issued
    // FIXME: This is yearly coupon rate, needs to be combined with payment frequency
    let yearly_coupon_rate: Fixed = bond
        .metadata()
        .get(&"coupon_rate".parse::<Name>().unwrap())
        .dbg_expect("INTERNAL BUG: bond missing `coupon_rate`")
        .to_owned()
        .try_into()
        .dbg_expect("`coupon_rate` not of the `NumericValue::Fixed` type");

    let nominal_value: Fixed = bond
        .metadata()
        .get(&"nominal_value".parse::<Name>().unwrap())
        .dbg_expect("INTERNAL BUG: bond missing `nominal_value`")
        .to_owned()
        .try_into()
        .dbg_expect("`nominal_value` not of the `NumericValue::Fixed` type");

    let currency: AssetDefinitionId = bond
        .metadata()
        .get("currency")
        .dbg_expect("Currency not found")
        .to_owned()
        .try_into()
        .dbg_expect("`currency` not of the `AssetDefinitionId` type");

    for issued_bond in issued_bonds {
        let issuer_money = AssetId::new(currency.clone(), issuer.clone());

        let quantity: u32 = issued_bond
            .value()
            .to_owned()
            .try_into()
            .dbg_expect("INTERNAL BUG: bond quantity is not of the `u32` type");
        let amount = Fixed::try_from(quantity as f64)
            .and_then(|qty| qty.checked_mul(nominal_value))
            .and_then(|qty| qty.checked_mul(yearly_coupon_rate))
            .dbg_expect("Bond total price overflow");

        TransferExpr::new(issuer_money, amount, issued_bond.id().account_id().clone())
            .execute()
            .dbg_expect("Failed to pay bond interest");
    }
}
