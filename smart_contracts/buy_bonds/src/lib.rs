//! Smart contract for buying bonds
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use core::num::NonZeroU64;

use alloc::{borrow::ToOwned as _, format};

use dlmalloc::GlobalDlmalloc;
use iroha_trigger::{
    data_model::prelude::*,
    debug::{dbg_panic, DebugExpectExt as _},
    log::{error, trace},
    prelude::*,
};

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

struct BuyBondsOrder {
    /// Who's buying the bond
    buyer: AccountId,
    /// Which bond to buy
    bond: AssetDefinition,
    /// How many bonds to buy
    quantity: NonZeroU64,
}

impl BuyBondsOrder {
    fn from_metadata(metadata: &Metadata) -> Self {
        let buyer = metadata
            .get("buyer")
            .dbg_expect("Buyer account id not found")
            .to_owned()
            .try_into()
            .dbg_expect("`buyer` not of the `AccountId` type");
        let bond_id: AssetDefinitionId = metadata
            .get("bond")
            .dbg_expect("Bond asset definition not found")
            .to_owned()
            .try_into()
            .dbg_expect("`bond` not of the `AssetDefinitionId` type");
        let quantity: u64 = metadata
            .get("quantity")
            .dbg_expect("Bond quantity not found")
            .to_owned()
            .try_into()
            .dbg_expect("`bond_quantity` not of the `u64` type");

        let bond = FindAssetDefinitionById::new(bond_id.clone())
            .execute()
            .dbg_expect(&format!("{bond_id}: asset definition not found"));

        Self {
            buyer,
            bond,
            quantity: NonZeroU64::new(quantity).dbg_expect("Bond quantity is zero"),
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
            trace!("Asset owner doesn't have enough funds");
            true
        }
    }

    fn execute(self, owner: AccountId) {
        // WARN: Do we expect that the same bond will be buyable in different currencies? If yes,
        // then this information should be provided by the buyer as part of the triggering event
        let bond_currency: AssetDefinitionId = self
            .bond
            .metadata()
            .get("currency")
            .dbg_expect("Currency not found")
            .to_owned()
            .try_into()
            .dbg_expect("`currency` not of the `AssetDefinitionId` type");

        // WARN: Are we going to support changing of the nominal value? I don't think that is easy
        // to support, instead we can just have the authority issue a new bond with a different price
        let bond_nominal_value: Fixed = self
            .bond
            .metadata()
            .get("nominal_value")
            .dbg_expect("Nominal value not found")
            .to_owned()
            .try_into()
            .dbg_expect("`nominal_value` not of the `NumericValue` type");

        let bonds_total_price = Fixed::try_from(self.quantity.get() as f64)
            .and_then(|qty| qty.checked_mul(bond_nominal_value))
            .dbg_expect("Bond total price overflow");
        let bond_buyer_money = AssetId::new(bond_currency, self.buyer.clone());
        let bond_issuer_bonds = AssetId::new(self.bond.id().clone(), owner.clone());

        if !Self::check_account_asset_amount(&bond_buyer_money, bonds_total_price.into()) {
            return;
        }
        if !Self::check_account_asset_amount(&bond_issuer_bonds, self.quantity.get().into()) {
            return;
        }

        TransferExpr::new(bond_buyer_money, bonds_total_price, owner)
            .execute()
            .dbg_expect("Sending money failed");
        TransferExpr::new(bond_issuer_bonds, self.quantity.get(), self.buyer)
            .execute()
            .dbg_expect("Sending bond failed");
    }
}

#[iroha_trigger::main]
fn main(id: TriggerId, owner: AccountId, event: Event) {
    let buy_bonds_key = "buy_bonds".parse().unwrap();

    let Event::Data(DataEvent::Trigger(TriggerEvent::MetadataInserted(event))) = event else {
        dbg_panic(
            "INTERNAL BUG: Triggering event is not TriggerEvent::MetadataInserted.
            To avoid this error, register the trigger using a more strict filter",
        );
    };
    if id != *event.target_id() {
        dbg_panic(
            "INTERNAL BUG: Triggered by metadata insert event of another trigger.
            To avoid this error, register the trigger using a more strict filter",
        );
    }
    if *event.key() != buy_bonds_key {
        dbg_panic(
            "INTERNAL BUG: Triggered by metadata insert event with wrong key.
            To avoid this error, register the trigger using a more strict filter",
        );
    }

    let Value::LimitedMetadata(metadata) = event.value() else {
        dbg_panic("Metadata value not of the correct type, expected: LimitedMetadata");
    };

    BuyBondsOrder::from_metadata(metadata).execute(owner);
}
