//! Scheduled time trigger for bond maturation
#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use alloc::{borrow::ToOwned as _, format};
use core::time::Duration;

use dlmalloc::GlobalDlmalloc;
use iroha_trigger::{
    data_model::prelude::*,
    debug::{dbg_panic, DebugExpectExt as _},
    log::info,
    prelude::*,
};

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

struct RegisterBond {
    /// Who's buying the bond
    new_bond: NewAssetDefinition,
}

impl RegisterBond {
    fn from_metadata(metadata: &Value) -> Self {
        let new_bond: NewAssetDefinition = metadata
            .to_owned()
            .try_into()
            .dbg_expect("`bond` not of the `NewAssetDefinition` type");

        Self { new_bond }
    }

    // TODO:
    fn register_interest_payments_trigger(&self, owner: AccountId) {
        const WASM: &[u8] =
            core::include_bytes!(concat!(core::env!("OUT_DIR"), "/interest_payments.wasm"));

        let registration_time_ms: NumericValue = self
            .new_bond
            .metadata()
            .get(&"registration_time_ms".parse::<Name>().unwrap())
            .dbg_expect("INTERNAL BUG: bond missing `registration_time_ms`")
            .to_owned()
            .try_into()
            .dbg_expect("`registration_time_ms` not of the `NumericValue` type");
        let registration_time = Duration::from_millis(registration_time_ms.try_into().dbg_expect(
            "INTERNAL BUG: `registration_time_ms` not of the `NumericValue::U64` type",
        ));
        // FIXME: Temporarily undefined. What values do we support here? Any period or
        // specified enum variants like `Yearly`/`Quarterly`/`Monthly`/`Weekly`?
        let payment_period = Duration::from_millis(4000);

        let bond_id = self.new_bond.id();
        let interest_payments_trigger_id: TriggerId = format!(
            "{}_{}_interest_payments",
            bond_id.name(),
            bond_id.domain_id()
        )
        .parse()
        .unwrap();
        let interest_payments_trigger = Trigger::new(
            interest_payments_trigger_id.clone(),
            Action::new(
                WasmSmartContract::from_compiled(WASM.to_vec()),
                Repeats::Indefinitely,
                owner,
                // TODO: This is simplified in RC22
                TriggeringFilterBox::from(TimeEventFilter::new(ExecutionTime::Schedule(
                    TimeSchedule::starting_at(registration_time).with_period(payment_period),
                ))),
            ),
        );

        info!(&format!(
            "{interest_payments_trigger_id}: Registering interest payments trigger"
        ));
        RegisterExpr::new(interest_payments_trigger)
            .execute()
            .unwrap();
    }

    fn register_bond_maturation_trigger(&self, owner: AccountId) {
        const WASM: &[u8] =
            core::include_bytes!(concat!(core::env!("OUT_DIR"), "/bond_maturation.wasm"));

        let maturation_date = Duration::from_millis(
            NumericValue::try_from(
                self.new_bond
                    .metadata()
                    .get(&"maturation_date_ms".parse::<Name>().unwrap())
                    .dbg_expect("INTERNAL BUG: bond missing `maturation_date_ms`")
                    .to_owned(),
            )
            .dbg_expect("`maturation_date_ms` not of the `NumericValue` type")
            .try_into()
            .dbg_expect("INTERNAL BUG: `maturation_date_ms` not of the `NumericValue::U64` type"),
        );

        let bond_id = self.new_bond.id();
        let maturation_trigger_id: TriggerId =
            format!("{}_{}_bond_maturation", bond_id.name(), bond_id.domain_id())
                .parse()
                .unwrap();
        let maturation_trigger = Trigger::new(
            maturation_trigger_id.clone(),
            Action::new(
                WasmSmartContract::from_compiled(WASM.to_vec()),
                Repeats::Exactly(1),
                owner,
                // TODO: This is simplified in RC22
                TriggeringFilterBox::from(TimeEventFilter::new(ExecutionTime::Schedule(
                    TimeSchedule::starting_at(maturation_date),
                ))),
            ),
        );

        info!(&format!(
            "{maturation_trigger_id}: Registering interest payments trigger"
        ));
        RegisterExpr::new(maturation_trigger).execute().unwrap();
    }

    fn execute(self, owner: AccountId) {
        self.register_interest_payments_trigger(owner.clone());
        self.register_bond_maturation_trigger(owner);

        RegisterExpr::new(self.new_bond).execute().unwrap();
    }
}

#[iroha_trigger::main]
fn main(id: TriggerId, owner: AccountId, event: Event) {
    let register_bond = "bond".parse().unwrap();

    // FIXME: Replace with by call trigger with args after migrating to RC22
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
    if *event.key() != register_bond {
        dbg_panic(
            "INTERNAL BUG: Triggered by metadata insert event with wrong key.
            To avoid this error, register the trigger using a more strict filter",
        );
    }

    RegisterBond::from_metadata(event.value()).execute(owner);
}
