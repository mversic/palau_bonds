//! Scheduled time trigger for bond maturation
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use alloc::format;

use bonds_data_model::{BondDetails, RegisterBondArgs};
use dlmalloc::GlobalDlmalloc;
use iroha_trigger::{
    data_model::{events::EventBox, prelude::*},
    debug::{dbg_panic, DebugExpectExt as _},
    log::info,
    prelude::*,
};

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

struct RegisterBond {
    /// Authority issuing the bond
    issuer: AccountId,
    /// Bond to be registered
    bond: NewAssetDefinition,
}

impl RegisterBond {
    fn new(issuer: AccountId, args: RegisterBondArgs) -> Self {
        Self {
            issuer,
            bond: args.bond,
        }
    }

    fn register_interest_payments_trigger(&self) {
        const WASM: &[u8] =
            core::include_bytes!(concat!(core::env!("OUT_DIR"), "/interest_payments.wasm"));

        let bond_id = self.bond.id();
        let trigger_id: TriggerId =
            format!("interest_payments_{}%%{}", bond_id.domain(), bond_id.name(),)
                .parse()
                .dbg_unwrap();

        let bond_details: BondDetails = self
            .bond
            .metadata()
            .get("details")
            .dbg_expect("INTERNAL BUG: bond `details` not found")
            .try_into()
            .dbg_expect("INTERNAL BUG: bond `details` is not of the `BondDetails` type");

        let interest_payments_trigger = Trigger::new(
            trigger_id.clone(),
            Action::new(
                WasmSmartContract::from_compiled(WASM.to_vec()),
                Repeats::Indefinitely,
                self.issuer.clone(),
                // TODO: This is simplified in RC22
                TimeEventFilter::new(ExecutionTime::Schedule(
                    TimeSchedule::starting_at(bond_details.registration_time)
                        .with_period(bond_details.payment_frequency),
                )),
            ),
        );

        info!(&format!("{trigger_id}: Registering ..."));
        Register::trigger(interest_payments_trigger)
            .execute()
            .unwrap();
    }

    fn register_bond_maturation_trigger(&self) {
        const WASM: &[u8] =
            core::include_bytes!(concat!(core::env!("OUT_DIR"), "/bond_maturation.wasm"));

        let bond_id = self.bond.id();
        let trigger_id: TriggerId =
            format!("bond_maturation_{}%%{}", bond_id.domain(), bond_id.name())
                .parse()
                .dbg_unwrap();

        let bond_details: BondDetails = self
            .bond
            .metadata()
            .get("details")
            .dbg_expect("INTERNAL BUG: bond `details` not found")
            .try_into()
            .dbg_expect("INTERNAL BUG: bond `details` is not of the `BondDetails` type");

        let maturation_trigger = Trigger::new(
            trigger_id.clone(),
            Action::new(
                WasmSmartContract::from_compiled(WASM.to_vec()),
                Repeats::Exactly(1),
                self.issuer.clone(),
                TimeEventFilter::new(ExecutionTime::Schedule(TimeSchedule::starting_at(
                    bond_details.maturation_date,
                ))),
            ),
        );

        info!(&format!("{trigger_id}: Registering ..."));
        Register::trigger(maturation_trigger).execute().unwrap();
    }

    fn execute(self) {
        self.register_interest_payments_trigger();
        self.register_bond_maturation_trigger();

        let bond_asset_id = AssetId::new(self.bond.id().clone(), self.issuer.clone());
        let bond_details: BondDetails = self
            .bond
            .metadata()
            .get("details")
            .dbg_expect("INTERNAL BUG: bond `details` not found")
            .try_into()
            .dbg_expect("INTERNAL BUG: bond `details` is not of the `BondDetails` type");

        Register::asset_definition(self.bond).execute().unwrap();

        Mint::asset_numeric(bond_details.quantity, bond_asset_id)
            .execute()
            .unwrap();
    }
}

#[iroha_trigger::main]
fn main(id: TriggerId, issuer: AccountId, event: EventBox) {
    let EventBox::ExecuteTrigger(event) = event else {
        dbg_panic(
            "INTERNAL BUG: Triggering event is not ExecuteTriggerEvent.
            This trigger should be registered as a by-call trigger",
        );
    };
    if id != *event.trigger_id() {
        dbg_panic("INTERNAL BUG: Triggering event doesn't match the current trigger");
    }
    if issuer != *event.authority() {
        dbg_panic("Event authority and the authority that registered the trigger don't match");
    }

    let args: RegisterBondArgs = event
        .args()
        .dbg_expect("Missing event args")
        .try_into()
        .dbg_expect("Unable to parse `RegisterBondArgs`");

    RegisterBond::new(issuer, args).execute();
}
