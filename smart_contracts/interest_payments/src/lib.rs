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
fn main(id: TriggerId, _owner: AccountId, event: Event) {
    let bond_id: AssetDefinitionId = id
        .name()
        .as_ref()
        .strip_suffix("_bond_maturation")
        .dbg_expect("INTERNAL BUG: Trigger name must end with `_bond_maturation`")
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

    // FIXME: This is yearly coupon rate, needs to be calculated from payment frequency
    let coupon_rate: Fixed = bond
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

    for issued_bond in issued_bonds {
        let quantity: u64 = NumericValue::try_from(issued_bond.value().to_owned())
            .dbg_expect("INTERNAL BUG: bond quantity is not of the `NumericValue::u64` type")
            .try_into()
            .dbg_expect("INTERNAL BUG: bond quantity is not of the `u64` type");
        let amount = Fixed::try_from(quantity as f64)
            .and_then(|qty| qty.checked_mul(nominal_value))
            .and_then(|qty| qty.checked_mul(coupon_rate))
            .dbg_expect("Bond total price overflow");

        // WARN: Should interest be minted or transferred from the bond issuer to the buyer?
        MintExpr::new(amount, issued_bond.id().clone())
            .execute()
            .dbg_expect("Failed to pay bond interest");
    }
}
