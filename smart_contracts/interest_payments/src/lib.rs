//! Periodic time trigger for making interest payments
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use alloc::{borrow::ToOwned as _, format};
use dlmalloc::GlobalDlmalloc;
use iroha_trigger::data_model::query::account::model::FindAccountById;
use iroha_trigger::log::{info, trace};
use iroha_trigger::{data_model::prelude::*, debug::dbg_panic};

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

const LIMITS: MetadataLimits = MetadataLimits::new(256, 256);
const ONE_YEAR_IN_SECONDS: u64 = 31_536_000;

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

    // WARN: Coupon rate can be changed by the issuer after the bond is issued (not required, for now)
    let yearly_coupon_rate: Fixed = bond
        .metadata()
        .get(&"coupon_rate".parse::<Name>().unwrap())
        .dbg_expect("INTERNAL BUG: bond missing `coupon_rate`")
        .to_owned()
        .try_into()
        .dbg_expect("`coupon_rate` not of the `NumericValue::Fixed` type");

    let payment_frequency_seconds: u64 = bond
        .metadata()
        .get(&"payment_frequency_seconds".parse::<Name>().unwrap())
        .dbg_expect("INTERNAL BUG: bond missing `payment_frequency_seconds`")
        .to_owned()
        .try_into()
        .dbg_expect("`payment_frequency_seconds` not of the `u64` type");
    let payment_frequency = Fixed::try_from(payment_frequency_seconds as f64)
        .dbg_expect("Payment frequency overflow");

    let payment_fraction = payment_frequency
        .checked_div(Fixed::try_from(ONE_YEAR_IN_SECONDS as f64).unwrap())
        .dbg_expect("Payment fraction overflow");

    let current_coupon_payment_fraction = yearly_coupon_rate
        .checked_mul(payment_fraction)
        .expect("Coupon payment overflow");

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
        let buyer = issued_bond.id().account_id().clone();
        if buyer == issuer {
            trace!(&format!("{bond_id}: Buyer is the issuer, skipping coupon payment"));

            continue;
        }

        let issuer_money = AssetId::new(currency.clone(), issuer.clone());

        let quantity: u32 = issued_bond
            .value()
            .to_owned()
            .try_into()
            .dbg_expect("INTERNAL BUG: bond quantity is not of the `u32` type");
        let amount = Fixed::try_from(quantity as f64)
            .and_then(|qty| qty.checked_mul(nominal_value))
            .and_then(|qty| qty.checked_mul(current_coupon_payment_fraction))
            .dbg_expect("Bond total price overflow");

        trace!(&format!(
                "{bond_id}: Transferring {amount} {issuer_money} from {issuer} to {buyer}"
            ));

        TransferExpr::new(issuer_money.clone(), amount.clone(), buyer.clone())
            .execute()
            .dbg_expect("Failed to pay bond interest");

        let coupon_payment_idx = find_coupon_payment_idx(&buyer);
        trace!(&format!("{bond_id}: index of coupon payment: {coupon_payment_idx}"));

        let transfer_metadata_id: Name = format!(
            "coupon_payment_{}%%{}%%idx%%{}",
            bond_id.name().to_owned(),
            bond_id.domain_id().to_owned(),
            coupon_payment_idx.to_owned())
            .parse()
            .dbg_expect("INTERNAL BUG: Unable to parse transfer metadata id");

        let mut transfer_metadata = Metadata::new();
        transfer_metadata
            .insert_with_limits("amount".parse().unwrap(), amount.into(), LIMITS)
            .unwrap();
        transfer_metadata
            .insert_with_limits("currency".parse().unwrap(), issuer_money.into(), LIMITS)
            .unwrap();
        transfer_metadata
            .insert_with_limits("bond_asset_id".parse().unwrap(), bond_id.clone().into(), LIMITS)
            .unwrap();

        SetKeyValueExpr::new(buyer, transfer_metadata_id, transfer_metadata)
            .execute()
            .dbg_expect("Failed to set transfer info to buyer's metadata");

        info!(
            &format!(
                "{bond_id}: Successfully set coupon payment info to buyer's metadata"
            )
        );
    }

    fn find_coupon_payment_idx(buyer: &AccountId) -> u32 {
        let coupon_payment_idx_key: Name = "coupon_payment_idx"
            .parse()
            .dbg_expect("INTERNAL BUG: Unable to parse coupon payment index key");

        let current_idx = FindAccountById::new(buyer.clone())
            .execute()
            .dbg_expect("INTERNAL BUG: Account not found")
            .metadata()
            .get(&coupon_payment_idx_key)
            .map(|idx| {
                idx.to_owned()
                    .try_into()
                    .dbg_expect("INTERNAL BUG: `coupon_payment_idx` not of the `u32` type")
            })
            .unwrap_or(0_u32);

        let new_idx = current_idx + 1;

        SetKeyValueExpr::new(buyer.clone(), coupon_payment_idx_key, Value::Numeric(new_idx.into()))
            .execute()
            .dbg_expect("Failed to set coupon payment index to buyer's metadata");

        new_idx
    }
}
