//! Smart contract for redeeming bonds
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use alloc::format;

use bonds_data_model::{BondDetails, FromJsonString, LogEntry};
use dlmalloc::GlobalDlmalloc;
use iroha_trigger::data_model::query::account::FindAccountById;
use iroha_trigger::log::info;
use iroha_trigger::{
    data_model::prelude::*,
    debug::{dbg_panic, DebugExpectExt as _},
    log::{error, trace},
    prelude::*,
};

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

struct RedeemBondsOrder {
    /// Who's buying back the bonds
    issuer: AccountId,
    /// Who's selling the bond
    seller: AccountId,
    /// Which bond to redeem
    bond: AssetDefinition,
    /// How many bonds to redeem
    quantity: Numeric,
}

impl RedeemBondsOrder {
    fn from_metadata(metadata: &JsonString, issuer: AccountId, seller: AccountId) -> Self {
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
            seller,
            bond,
            quantity,
        }
    }

    /// Checks:
    ///
    /// * The Account has this asset.
    /// * The AssetValue has a NumericValue type
    /// * The Account has enough asset quantity for transaction.
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

    fn find_redeem_bond_payment_idx(buyer: &AccountId) -> u32 {
        let redeem_bond_payment_idx_key: Name = "redeem_bond_payment_idx"
            .parse()
            .dbg_expect("INTERNAL BUG: Unable to parse redeem bond payment index key");

        let current_idx = FindAccountById::new(buyer.clone())
            .execute()
            .dbg_expect("INTERNAL BUG: Account not found")
            .into_inner()
            .metadata()
            .get(&redeem_bond_payment_idx_key)
            .map(|idx| {
                idx.try_into_any()
                    .dbg_expect("INTERNAL BUG: `redeem_bond_payment_idx` not of the `u32` type")
            })
            .unwrap_or(0_u32);

        let new_idx = current_idx + 1;

        SetKeyValue::account(buyer.clone(), redeem_bond_payment_idx_key, new_idx)
            .execute()
            .dbg_expect("Failed to set redeem bond payment index to seller's metadata");

        new_idx
    }

    fn execute(self) {
        let bond_details: BondDetails = self
            .bond
            .metadata()
            .get("details")
            .dbg_expect("INTERNAL BUG: bond `details` not found")
            .try_into()
            .dbg_expect("INTERNAL BUG: bond `details` is not of the `BondDetails` type");

        let bond_currency = AssetDefinitionId::from_json_string(&bond_details.currency).unwrap();

        let bonds_total_price = self
            .quantity
            .checked_mul(bond_details.nominal_value, NumericSpec::default())
            .dbg_expect("Bond total price overflow");

        let bond_seller_bonds = AssetId::new(self.bond.id().clone(), self.seller.clone());
        let bond_issuer_money = AssetId::new(bond_currency.clone(), self.issuer.clone());

        if !Self::check_account_asset_amount(&bond_issuer_money, bonds_total_price.into()) {
            return;
        }
        if !Self::check_account_asset_amount(&bond_seller_bonds, self.quantity) {
            return;
        }

        let issuer = self.issuer.clone();
        let seller = self.seller.clone();
        //todo: implement fee if needed
        // let fee_beneficiary = AccountId::from_json_string(&bond_details.fee_beneficiary).unwrap();

        info!(&format!(
            "Transferring {bonds_total_price} {bond_issuer_money} from {issuer} to {seller}"
        ));

        Transfer::asset_numeric(
            bond_issuer_money.clone(),
            bonds_total_price,
            self.seller.clone(),
        )
        .execute()
        .dbg_expect("Sending money failed");
        Burn::asset_numeric(self.quantity, bond_seller_bonds)
            .execute()
            .dbg_expect("Burning bonds failed");

        Self::write_log_entry(seller, &self.bond.id(), bonds_total_price, self.quantity);
    }

    /// Write a log entry into the buyer's metadata
    fn write_log_entry(
        seller: AccountId,
        bond_id: &AssetDefinitionId,
        amount: Numeric,
        quantity: Numeric,
    ) {
        let redeem_bond_payment_idx = Self::find_redeem_bond_payment_idx(&seller);

        let log_entry_id: Name = format!(
            "redeem_bond_payment_{}%%{}%%idx%%{}",
            bond_id.name(),
            bond_id.domain(),
            redeem_bond_payment_idx,
        )
        .parse()
        .dbg_expect("INTERNAL BUG: Unable to build redeem bond payment metadata id");

        let log_entry = LogEntry {
            bond: bond_id.clone(),
            amount,
            quantity,
        };

        SetKeyValue::account(seller, log_entry_id.clone(), log_entry)
            .execute()
            .dbg_expect("Failed to set transfer info to buyer's metadata");

        trace!(&format!(
            "{log_entry_id}: Coupon payment info set into buyer's metadata"
        ));
    }
}

#[iroha_trigger::main]
fn main(_id: TriggerId, issuer: AccountId, event: EventBox) {
    let redeem_bonds_key = "redeem_bonds".parse().unwrap();

    let EventBox::Data(DataEvent::Domain(DomainEvent::Account(AccountEvent::MetadataInserted(
        event,
    )))) = event
    else {
        dbg_panic(
            "INTERNAL BUG: Triggering event is not AccountEvent::MetadataInserted.
            To avoid this error, register the trigger using a more strict filter",
        );
    };
    if event.key() != &redeem_bonds_key {
        // TODO: Can we filter more precisely to avoid invoking trigger?
        trace!("It's not a redeem bonds event");

        return;
    }

    let seller = event.target().clone();
    RedeemBondsOrder::from_metadata(event.value(), issuer, seller.clone()).execute();
    RemoveKeyValue::account(seller, redeem_bonds_key)
        .execute()
        .dbg_unwrap();
}
