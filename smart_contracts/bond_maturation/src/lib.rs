//! Scheduled time trigger for bond maturation
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use alloc::{borrow::ToOwned as _, format};

use dlmalloc::GlobalDlmalloc;
use iroha_trigger::{data_model::prelude::*, debug::dbg_panic, log::error};

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

#[iroha_trigger::main]
fn main(id: TriggerId, owner: AccountId, event: Event) {
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
    let accounts_with_bond = FindAccountsWithAsset::new(bond_id.clone())
        .execute()
        .dbg_expect(&format!("{bond_id}: Bond not found"));

    // WARN: Do we expect that the same bond will be buyable in different currencies? If yes,
    // then this information should be provided by the buyer as part of the triggering event
    let bond_currency: AssetDefinitionId = bond
        .metadata()
        .get("currency")
        .dbg_expect("Currency not found")
        .to_owned()
        .try_into()
        .dbg_expect("`currency` not of the `AssetDefinitionId` type");

    // WARN: Are we going to support changing of the nominal value? I don't think that is easy
    // to support, instead we can just have the authority issue a new bond with a different price
    let bond_nominal_value: Fixed = bond
        .metadata()
        .get("nominal_value")
        .dbg_expect("Nominal value not found")
        .to_owned()
        .try_into()
        .dbg_expect("`nominal_value` not of the `NumericValue::Fixed` type");

    for account in accounts_with_bond {
        let buyer_bonds = AssetId::new(bond_id.clone(), account.id().clone());

        // WARN: Should bonds be burnt or transferred back to the issuer?
        if let Err(err) = UnregisterExpr::new(buyer_bonds).execute() {
            error!(&format!(
                "{}: Failed to mature the bond (reason = {err:?})",
                account.id()
            ));
        } else {
            let bond_issuer_money = AssetId::new(bond_currency.clone(), owner.clone());

            TransferExpr::new(bond_issuer_money, bond_nominal_value, account.id().clone())
                .execute()
                .dbg_expect("Sending money failed. Country might have went bankrupt");
        }
    }

    // TODO: Should all related triggers be unregistered at bond maturation?
    // Or should they be unregistered when asset definition is unregistered?
    // Both approaches can be automatized
}
