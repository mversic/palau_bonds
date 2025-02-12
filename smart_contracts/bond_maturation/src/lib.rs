//! Scheduled time trigger for bond maturation
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use alloc::format;

use bonds_data_model::{BondDetails, FromJsonString, LogEntry};
use dlmalloc::GlobalDlmalloc;
use iroha_trigger::{
    data_model::{
        asset::{AssetDefinitionId, AssetValue},
        events::EventBox,
        prelude::*,
    },
    debug::{dbg_panic, DebugExpectExt as _},
    log::{error, info, trace},
    smart_contract::ExecuteOnHost as _,
};

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

/// Parse bond id from trigger name
fn parse_bond_id(id: TriggerId) -> AssetDefinitionId {
    const PREFIX: &str = "bond_maturation_";

    id.name()
        .as_ref()
        .strip_prefix(PREFIX)
        .dbg_expect(&format!(
            "INTERNAL BUG: Trigger name must end with `{PREFIX}`"
        ))
        .replace("%%", "#")
        .parse()
        .dbg_expect(
            "INTERNAL BUG: Unable to parse bond id from trigger name prefix.
                Prefix trigger name with the id of the bond it's registered for",
        )
}

/// Write a log entry into the buyer's metadata
fn write_log_entry(
    buyer: AccountId,
    bond_id: &AssetDefinitionId,
    amount: Numeric,
    quantity: Numeric,
) {
    let log_entry_id: Name = format!("bond_maturation_{}%%{}", bond_id.name(), bond_id.domain())
        .parse()
        .dbg_expect("INTERNAL BUG: Unable to parse transfer metadata id");

    let log_entry = LogEntry {
        bond: bond_id.clone(),
        amount,
        quantity,
    };
    SetKeyValue::account(buyer, log_entry_id, log_entry)
        .execute()
        .dbg_expect("Failed to set transfer info to buyer's metadata");

    trace!(&format!(
        "{bond_id}: Successfully set maturity payment details into buyer's metadata"
    ));
}

#[iroha_trigger::main]
fn main(id: TriggerId, issuer: AccountId, event: EventBox) {
    if !matches!(event, EventBox::Time(_)) {
        dbg_panic(
            "INTERNAL BUG: Triggering event is not TimeEvent.
            To avoid this error, register the trigger using the correct filter",
        );
    }

    let bond_id: AssetDefinitionId = parse_bond_id(id);
    let bond = FindAssetDefinitionById::new(bond_id.clone())
        .execute()
        .dbg_expect(&format!("{bond_id}: Bond not found"))
        .into_inner();
    let bond_details: BondDetails = bond
        .metadata()
        .get("details")
        .dbg_expect("INTERNAL BUG: bond `details` not found")
        .try_into()
        .dbg_expect("INTERNAL BUG: bond `details` is not of the `BondDetails` type");
    let currency_id = AssetDefinitionId::from_json_string(&bond_details.currency)
        .dbg_expect("INTERNAL BUG: Unable to parse bond currency id");

    let issued_bonds = FindAssetsByAssetDefinitionId::new(bond_id.clone())
        .execute()
        .unwrap();

    for (buyer, issued_bond) in issued_bonds.into_iter().filter_map(|issued_bond| {
        let issued_bond = issued_bond.dbg_expect("Query expired");
        let buyer = issued_bond.id().account().clone();

        if buyer == issuer {
            return None;
        }

        Some((buyer, issued_bond))
    }) {
        // FIXME: Should bonds be burnt or transferred back to the issuer?
        if let Err(err) = Unregister::asset(issued_bond.id().clone()).execute() {
            error!(&format!(
                "{}: Bond maturation failed ({err:?})",
                issued_bond.id()
            ));
        } else {
            let bond_issuer_money = AssetId::new(currency_id.clone(), issuer.clone());

            let AssetValue::Numeric(quantity) = issued_bond.value() else {
                dbg_panic("INTERNAL BUG: bond quantity is not of the `Numeric` type")
            };
            assert_eq!(quantity.scale(), 0, "Bond quantity can't be a decimal");

            let amount = quantity
                .checked_mul(bond_details.nominal_value, NumericSpec::default())
                .dbg_expect("Bond total price overflow");

            trace!(&format!(
                "{bond_id}: Transferring {amount} {} from {issuer} to {buyer}",
                bond_details.currency
            ));

            Transfer::asset_numeric(bond_issuer_money, amount, buyer.clone())
                .execute()
                .dbg_expect("Sending money failed. Country might have went bankrupt");

            write_log_entry(buyer, &bond_id, amount, quantity.clone());
        }
    }

    info!(&format!("{bond_id}: Bond matured"));

    // FIXME: Should all related triggers be unregistered at bond maturation?
    // Or should they be unregistered when asset definition is unregistered?
    // Both approaches can be automatized
}
