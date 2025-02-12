//! Smart contract for buying bonds
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use alloc::format;

use bonds_data_model::{BondDetails, FromJsonString, LogEntry};
use dlmalloc::GlobalDlmalloc;
use iroha_trigger::{
    data_model::{events::EventBox, prelude::*},
    debug::{dbg_panic, DebugExpectExt as _},
    log::{error, trace},
    prelude::*,
};

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

struct BuyBondsOrder {
    /// Who's selling the bonds
    issuer: AccountId,
    /// Who's buying the bond
    buyer: AccountId,
    /// Which bond to buy
    bond: AssetDefinition,
    /// How many bonds to buy
    quantity: Numeric,
}

impl BuyBondsOrder {
    fn new(metadata: &JsonString, issuer: AccountId, buyer: AccountId) -> Self {
        let bond_key: Name = "bond".parse().unwrap();
        let quantity_key: Name = "quantity".parse().unwrap();

        let buy_bond_metadata = Metadata::from_json_string(metadata)
            .dbg_expect("INTERNAL BUG: Unable to parse metadata");

        let bond_id: AssetDefinitionId = buy_bond_metadata
            .get(&bond_key)
            .dbg_expect("Bond asset definition not found")
            .try_into_any()
            .dbg_expect("`bond` not of the `AssetDefinitionId` type");
        let quantity: Numeric = buy_bond_metadata
            .get(&quantity_key)
            .dbg_expect("Bond quantity not found")
            .try_into_any::<u32>()
            .dbg_expect("`bond_quantity` is not of the `Numeric` type")
            .into();
        assert_eq!(quantity.scale(), 0, "Bond quantity can't be a decimal");

        let bond = FindAssetDefinitionById::new(bond_id.clone())
            .execute()
            .dbg_expect(&format!("{bond_id}: asset definition not found"))
            .into_inner();

        Self {
            issuer,
            buyer,
            bond,
            quantity,
        }
    }

    fn check_account_asset_amount(asset_id: &AssetId, asset_amount: Numeric) -> bool {
        let Ok(asset) = FindAssetById::new(asset_id.clone()).execute() else {
            error!("Asset not found");
            return false;
        };

        let asset = asset.into_inner();
        let AssetValue::Numeric(quantity) = asset.value() else {
            error!("Asset not of the correct type");
            return false;
        };

        if *quantity < asset_amount {
            error!("Asset owner doesn't have enough funds");
            false
        } else {
            trace!("Asset owner has enough funds");
            true
        }
    }

    fn execute(self) {
        let bond_details: BondDetails = self
            .bond
            .metadata()
            .get("details")
            .dbg_expect("INTERNAL BUG: bond `details` not found")
            .try_into()
            .dbg_expect("INTERNAL BUG: bond `details` is not of the `BondDetails` type");

        let bonds_total_price = self
            .quantity
            .checked_mul(bond_details.nominal_value, NumericSpec::default())
            .dbg_expect("Bond total price overflow");

        let currency_id = AssetDefinitionId::from_json_string(&bond_details.currency).dbg_unwrap();
        let bond_buyer_money = AssetId::new(currency_id, self.buyer.clone());
        let bond_issuer_bonds = AssetId::new(self.bond.id().clone(), self.issuer.clone());

        if !Self::check_account_asset_amount(&bond_buyer_money, bonds_total_price.into()) {
            return;
        }
        if !Self::check_account_asset_amount(&bond_issuer_bonds, self.quantity.into()) {
            return;
        }

        let fee_beneficiary =
            AccountId::from_json_string(&bond_details.fee_beneficiary).dbg_unwrap();

        Transfer::asset_numeric(bond_buyer_money.clone(), bonds_total_price, self.issuer)
            .execute()
            .dbg_expect("Sending money failed");
        Transfer::asset_numeric(bond_buyer_money, bond_details.fee, fee_beneficiary)
            .execute()
            .dbg_expect("Sending fee failed");
        Transfer::asset_numeric(bond_issuer_bonds, self.quantity, self.buyer.clone())
            .execute()
            .dbg_expect("Sending bond failed");

        write_log_entry(
            self.buyer,
            &self.bond.id(),
            bonds_total_price,
            self.quantity,
        );
    }
}

fn find_buy_bond_payment_idx(buyer: &AccountId) -> u32 {
    let buy_bond_payment_idx_key: Name = "buy_bond_payment_idx"
        .parse()
        .dbg_expect("INTERNAL BUG: Unable to parse bond payment index key");

    let account = FindAccountById::new(buyer.clone())
        .execute()
        .dbg_expect("INTERNAL BUG: Account not found")
        .into_inner();

    let current_idx = account
        .metadata()
        .get(&buy_bond_payment_idx_key)
        .map(|idx| {
            idx.try_into_any()
                .dbg_expect("INTERNAL BUG: `buy_bond_payment_idx` not of the `u32` type")
        })
        .unwrap_or(0_u32);

    let new_idx = current_idx + 1;

    SetKeyValue::account(buyer.clone(), buy_bond_payment_idx_key, new_idx)
        .execute()
        .dbg_expect("Failed to set buy bond payment index to buyer's metadata");

    new_idx
}

/// Write a log entry into the buyer's metadata
fn write_log_entry(
    buyer: AccountId,
    bond_id: &AssetDefinitionId,
    amount: Numeric,
    quantity: Numeric,
) {
    let coupon_payment_idx = find_buy_bond_payment_idx(&buyer);

    let log_entry_id: Name = format!(
        "buy_bond_payment_{}%%{}%%idx%%{}",
        bond_id.name(),
        bond_id.domain(),
        coupon_payment_idx,
    )
    .parse()
    .dbg_expect("INTERNAL BUG: Unable to parse transfer metadata id");

    let log_entry = LogEntry {
        bond: bond_id.clone(),
        amount,
        quantity,
    };

    SetKeyValue::account(buyer, log_entry_id.clone(), log_entry)
        .execute()
        .dbg_expect("Failed to set transfer info to buyer's metadata");

    trace!(&format!(
        "{log_entry_id}: Coupon payment info set into buyer's metadata"
    ));
}

#[iroha_trigger::main]
fn main(_id: TriggerId, issuer: AccountId, event: EventBox) {
    let buy_bonds_key = "buy_bonds".parse().unwrap();

    let EventBox::Data(DataEvent::Domain(DomainEvent::Account(AccountEvent::MetadataInserted(
        event,
    )))) = event
    else {
        dbg_panic(
            "INTERNAL BUG: Triggering event is not AccountEvent::MetadataInserted.
            To avoid this error, register the trigger using a more strict filter",
        );
    };
    // TODO: Can we filter more precisely to avoid invoking trigger?
    if event.key() != &buy_bonds_key {
        trace!("Triggered by account metadata insert event with another key");

        return;
    }

    let buyer = event.target().clone();
    BuyBondsOrder::new(event.value(), issuer, buyer.clone()).execute();
    RemoveKeyValue::account(buyer, buy_bonds_key)
        .execute()
        .dbg_unwrap();
}
