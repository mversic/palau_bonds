//! Smart contract for buying bonds
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use core::num::NonZeroU32;

use alloc::{borrow::ToOwned as _, format};

use dlmalloc::GlobalDlmalloc;
use iroha_trigger::{
    data_model::prelude::*,
    debug::{dbg_panic, DebugExpectExt as _},
    log::{error, trace},
    prelude::*,
};
use iroha_trigger::data_model::query::account::model::FindAccountById;

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

const LIMITS: MetadataLimits = MetadataLimits::new(256, 256);

struct BuyBondsOrder {
    /// Who's selling the bonds
    issuer: AccountId,
    /// Who's buying the bond
    buyer: AccountId,
    /// Which bond to buy
    bond: AssetDefinition,
    /// How many bonds to buy
    quantity: NonZeroU32,
}

impl BuyBondsOrder {
    fn from_metadata(metadata: &Metadata, issuer: AccountId, buyer: AccountId) -> Self {
        let bond_id: AssetDefinitionId = metadata
            .get("bond")
            .dbg_expect("Bond asset definition not found")
            .to_owned()
            .try_into()
            .dbg_expect("`bond` not of the `AssetDefinitionId` type");
        let quantity: u32 = metadata
            .get("quantity")
            .dbg_expect("Bond quantity not found")
            .to_owned()
            .try_into()
            .dbg_expect("`bond_quantity` not of the `u32` type");

        let bond = FindAssetDefinitionById::new(bond_id.clone())
            .execute()
            .dbg_expect(&format!("{bond_id}: asset definition not found"));

        Self {
            issuer,
            buyer,
            bond,
            quantity: NonZeroU32::new(quantity).dbg_expect("Bond quantity is zero"),
        }
    }

    /// Checks:
    ///
    /// * The Account has this asset.
    /// * The AssetValue has a NumericValue type
    /// * The Account has enough asset quantity for transaction.
    fn check_account_asset_amount(asset_id: &AssetId, asset_amount: NumericValue) -> bool {
        let Ok(asset) = FindAssetById::new(asset_id.clone()).execute() else {
            error!("Asset not found");
            return false;
        };
        let Ok(asset): Result<NumericValue, _> = asset.value().to_owned().try_into() else {
            error!("Asset not of the correct type");
            return false;
        };

        if asset < asset_amount {
            error!("Asset owner doesn't have enough funds");
            false
        } else {
            trace!("Asset owner has enough funds");
            true
        }
    }

    fn find_buy_bond_payment_idx(buyer: &AccountId) -> u32 {
        let buy_bond_payment_idx_key: Name = "buy_bond_payment_idx"
            .parse()
            .dbg_expect("INTERNAL BUG: Unable to parse buy bond payment index key");

        let current_idx = FindAccountById::new(buyer.clone())
            .execute()
            .dbg_expect("INTERNAL BUG: Account not found")
            .metadata()
            .get(&buy_bond_payment_idx_key)
            .map(|idx| {
                idx.to_owned()
                    .try_into()
                    .dbg_expect("INTERNAL BUG: `buy_bond_payment_idx` not of the `u32` type")
            })
            .unwrap_or(0_u32);

        let new_idx = current_idx + 1;

        SetKeyValueExpr::new(buyer.clone(), buy_bond_payment_idx_key, Value::Numeric(new_idx.into()))
            .execute()
            .dbg_expect("Failed to set buy bond payment index to buyer's metadata");

        new_idx
    }

    fn execute(self) {
        let bond_currency: AssetDefinitionId = self
            .bond
            .metadata()
            .get("currency")
            .dbg_expect("Currency not found")
            .to_owned()
            .try_into()
            .dbg_expect("`currency` not of the `AssetDefinitionId` type");

        let bond_nominal_value: Fixed = self
            .bond
            .metadata()
            .get("nominal_value")
            .dbg_expect("Nominal value not found")
            .to_owned()
            .try_into()
            .dbg_expect("`nominal_value` not of the `NumericValue::Fixed` type");

        // note: fixed fee is an absolute value, i.e 0.1$. Clarify if it should be a percentage of the bond nominal value
        let bond_fee: Fixed = self
            .bond
            .metadata()
            .get("fixed_fee")
            .dbg_expect("Fixed fee not found")
            .to_owned()
            .try_into()
            .dbg_expect("`fixed_fee` not of the `NumericValue::Fixed` type");

        let bond_fee_recipient: AccountId = self
            .bond
            .metadata()
            .get("fee_recipient_account_id")
            .dbg_expect("Fee recipient account id not found")
            .to_owned()
            .try_into()
            .dbg_expect("`fee_recipient_account_id` not of the `AccountId` type");

        let bonds_total_price = Fixed::try_from(self.quantity.get() as f64)
            .and_then(|qty| qty.checked_mul(bond_nominal_value))
            .dbg_expect("Bond total price overflow");

        let bond_buyer_money = AssetId::new(bond_currency.clone(), self.buyer.clone());
        let bond_issuer_bonds = AssetId::new(self.bond.id().clone(), self.issuer.clone());

        if !Self::check_account_asset_amount(&bond_buyer_money, bonds_total_price.into()) {
            return;
        }
        if !Self::check_account_asset_amount(&bond_issuer_bonds, self.quantity.get().into()) {
            return;
        }

        let buy_bond_payment_idx = Self::find_buy_bond_payment_idx(&self.buyer);
        let transfer_metadata_id: Name = format!(
            "buy_bond_payment_{}%%{}%%idx%%{}",
            self.bond.id().name().to_owned(),
            self.bond.id().domain_id().to_owned(),
            buy_bond_payment_idx.to_owned())
            .parse()
            .dbg_expect("INTERNAL BUG: Unable to parse transfer metadata id");

        let mut transfer_metadata = Metadata::new();
        transfer_metadata
            .insert_with_limits("amount".parse().unwrap(), bonds_total_price.into(), LIMITS)
            .unwrap();
        transfer_metadata
            .insert_with_limits("currency".parse().unwrap(), bond_currency.into(), LIMITS)
            .unwrap();
        transfer_metadata
            .insert_with_limits("bond_asset_id".parse().unwrap(), self.bond.id().clone().into(), LIMITS)
            .unwrap();

        TransferExpr::new(bond_buyer_money.clone(), bonds_total_price, self.issuer)
            .execute()
            .dbg_expect("Sending money failed");
        TransferExpr::new(bond_buyer_money, bond_fee, bond_fee_recipient)
            .execute()
            .dbg_expect("Sending fee failed");
        TransferExpr::new(bond_issuer_bonds, self.quantity.get(), self.buyer.clone())
            .execute()
            .dbg_expect("Sending bond failed");
        SetKeyValueExpr::new(self.buyer, transfer_metadata_id, transfer_metadata)
            .execute()
            .dbg_expect("Failed to set buy bond info to buyer's metadata");
    }
}

#[iroha_trigger::main]
fn main(_id: TriggerId, issuer: AccountId, event: Event) {
    let buy_bonds_key = "buy_bonds".parse().unwrap();

    let Event::Data(DataEvent::Account(AccountEvent::MetadataInserted(event))) = event else {
        dbg_panic(
            "INTERNAL BUG: Triggering event is not AccountEvent::MetadataInserted.
            To avoid this error, register the trigger using a more strict filter",
        );
    };
    if event.key() != &buy_bonds_key {
        // TODO: Can we filter more precisely to avoid invoking trigger?
        trace!("Triggered by account metadata insert event with another key");
    }

    let Value::LimitedMetadata(metadata) = event.value() else {
        dbg_panic("Metadata value not of the correct type, expected: LimitedMetadata");
    };

    let buyer = event.target_id().clone();
    BuyBondsOrder::from_metadata(metadata, issuer, buyer.clone()).execute();
    RemoveKeyValueExpr::new(buyer, buy_bonds_key)
        .execute()
        .dbg_unwrap();
}
