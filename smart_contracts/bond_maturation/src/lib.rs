//! Scheduled time trigger for bond maturation
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use alloc::{borrow::ToOwned as _, format};

use dlmalloc::GlobalDlmalloc;
use iroha_trigger::{data_model::prelude::*, debug::dbg_panic, log::{error, info}};

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

#[iroha_trigger::main]
fn main(id: TriggerId, issuer: AccountId, event: Event) {
    let bond_id: AssetDefinitionId = id
        .name()
        .as_ref()
        .strip_suffix("%%bond_maturation")
        .dbg_expect("INTERNAL BUG: Trigger name must end with `%%bond_maturation`")
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

    let bond_currency: AssetDefinitionId = bond
        .metadata()
        .get("currency")
        .dbg_expect("Currency not found")
        .to_owned()
        .try_into()
        .dbg_expect("`currency` not of the `AssetDefinitionId` type");

    let nominal_value: Fixed = bond
        .metadata()
        .get("nominal_value")
        .dbg_expect("Nominal value not found")
        .to_owned()
        .try_into()
        .dbg_expect("`nominal_value` not of the `NumericValue::Fixed` type");

    for issued_bond in issued_bonds {
        let buyer = issued_bond.id().account_id().clone();

        let quantity: u32 = issued_bond
            .value()
            .to_owned()
            .try_into()
            .dbg_expect("INTERNAL BUG: bond quantity is not of the `u32` type");
        let amount = Fixed::try_from(quantity as f64)
            .and_then(|qty| qty.checked_mul(nominal_value))
            .dbg_expect("Bond total price overflow");

        // FIXME: Should bonds be burnt or transferred back to the issuer?
        if let Err(err) = UnregisterExpr::new(issued_bond.id().clone()).execute() {
            error!(&format!(
                "{}: Failed to mature the bond (reason = {err:?})",
                issued_bond.id().account_id()
            ));
        } else {
            let bond_issuer_money = AssetId::new(bond_currency.clone(), issuer.clone());

            TransferExpr::new(bond_issuer_money, amount, buyer)
                .execute()
                .dbg_expect("Sending money failed. Country might have went bankrupt");
        }

        info!(&format!("{bond_id}: Bond matured"));
    }

    // TODO: Should all related triggers be unregistered at bond maturation?
    // Or should they be unregistered when asset definition is unregistered?
    // Both approaches can be automatized
}
