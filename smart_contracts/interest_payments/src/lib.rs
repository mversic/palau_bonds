//! Periodic time trigger for making interest payments
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use alloc::format;

use bonds_data_model::{BondDetails, LogEntry};
use dlmalloc::GlobalDlmalloc;
use iroha_trigger::{
    data_model::{events::EventBox, prelude::*},
    debug::dbg_panic,
    log::trace,
    smart_contract::{debug::DebugExpectExt as _, ExecuteOnHost as _, ExecuteQueryOnHost as _},
};

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

const ONE_YEAR_IN_SECONDS: u64 = 31_536_000;

/// Parse bond id from trigger name
fn parse_bond_id(id: TriggerId) -> AssetDefinitionId {
    const PREFIX: &str = "interest_payments_";

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

fn find_coupon_payment_idx(buyer: &AccountId) -> u32 {
    let coupon_payment_idx_key: Name = "coupon_payment_idx"
        .parse()
        .dbg_expect("INTERNAL BUG: Unable to parse coupon payment index key");
    let find_account_query = FindAccountById::new(buyer.clone());

    let account = find_account_query
        .execute()
        .dbg_expect("INTERNAL BUG: Account not found")
        .into_inner();

    let current_idx = account
        .metadata()
        .get(&coupon_payment_idx_key)
        .map(|idx| {
            idx.try_into_any()
                .dbg_expect("INTERNAL BUG: `coupon_payment_idx` not of the `u32` type")
        })
        .unwrap_or(0_u32);

    let new_idx = current_idx + 1;

    SetKeyValue::account(buyer.clone(), coupon_payment_idx_key, new_idx)
        .execute()
        .dbg_expect("Failed to set coupon payment index to buyer's metadata");

    new_idx
}

/// Write a log entry into the buyer's metadata
fn write_log_entry(buyer: AccountId, bond_id: &AssetDefinitionId, amount: Numeric) {
    let coupon_payment_idx = find_coupon_payment_idx(&buyer);

    let log_entry_id: Name = format!(
        "coupon_payment_{}%%{}%%idx%%{}",
        bond_id.domain(),
        bond_id.name(),
        coupon_payment_idx,
    )
    .parse()
    .dbg_expect("INTERNAL BUG: Unable to parse transfer metadata id");

    let log_entry = LogEntry {
        bond: bond_id.clone(),
        amount,
    };

    SetKeyValue::account(buyer, log_entry_id.clone(), log_entry)
        .execute()
        .dbg_expect("Failed to set transfer info to buyer's metadata");

    trace!(&format!(
        "{log_entry_id}: Coupon payment info set into buyer's metadata"
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
        .dbg_expect("INTERNAL BUG: `details` is not of the `BondDetails` type");

    let issued_bonds = FindAssetsByAssetDefinitionId::new(bond_id.clone())
        .execute()
        .dbg_expect(&format!("{bond_id}: Bond not found"));

    let current_coupon_payment_fraction = bond_details
        .coupon_rate
        // FIXME: This must be checked_mul
        .checked_add(bond_details.payment_frequency.as_secs().into())
        .unwrap()
        // FIXME: This must be checked_div
        .checked_sub(ONE_YEAR_IN_SECONDS.into())
        .expect("Coupon payment overflow");

    for (buyer, issued_bond) in issued_bonds.into_iter().filter_map(|issued_bond| {
        let issued_bond = issued_bond.dbg_expect("Query expired");
        let buyer = issued_bond.id().account().clone();

        if buyer == issuer {
            return None;
        }

        Some((buyer, issued_bond))
    }) {
        let issuer_money = AssetId::new(bond_details.currency.clone(), issuer.clone());

        let AssetValue::Numeric(quantity) = issued_bond.value() else {
            dbg_panic("INTERNAL BUG: bond quantity is not of the `Numeric` type")
        };
        assert_eq!(quantity.scale(), 0, "Bond quantity can't be a decimal");

        let amount = quantity
            // FIXME: This must be checked_mul
            .checked_add(bond_details.nominal_value)
            .and_then(|qty| {
                // FIXME: This must be checked_mul
                qty.checked_add(current_coupon_payment_fraction)
            })
            .dbg_expect("Bond total price overflow");

        trace!(&format!(
            "{bond_id}: Transferring {amount} {issuer_money} from {issuer} to {buyer}"
        ));

        Transfer::asset_numeric(issuer_money.clone(), amount, buyer.clone())
            .execute()
            .dbg_expect("Failed to pay bond interest");

        write_log_entry(buyer, &bond_id, amount);
    }
}
