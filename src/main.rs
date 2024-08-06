use std::{
    thread::sleep,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use eyre::Result;
use iroha_client::{
    client::Client,
    data_model::{
        asset::{AssetDefinition, AssetValueType},
        metadata::{Limits, Metadata},
        prelude::*,
        Registered,
    },
};
use iroha_config::{base::proxy::LoadFromDisk, client::ConfigurationProxy};

fn register_triggers(iroha: &Client) -> Result<()> {
    // TODO: Get from config in RC22
    let account_id: AccountId = "government@palau".parse().unwrap();

    println!("Building register_bond trigger...");
    let register_bond_wasm = WasmSmartContract::from_compiled(
        iroha_wasm_builder::Builder::new("smart_contracts/register_bond")
            // TODO: Available in RC22
            //.show_output()
            .build()?
            .optimize()?
            .into_bytes()?,
    );
    println!("Building buy_bonds trigger...");
    let buy_bonds_wasm = WasmSmartContract::from_compiled(
        iroha_wasm_builder::Builder::new("smart_contracts/buy_bonds")
            // TODO: Available in RC22
            //.show_output()
            .build()?
            .optimize()?
            .into_bytes()?,
    );

    let register_bond_trigger_id: TriggerId = "register_bond".parse().unwrap();
    let register_bond_trigger = Trigger::new(
        register_bond_trigger_id.clone(),
        Action::new(
            register_bond_wasm,
            Repeats::Indefinitely,
            account_id.clone(),
            // TODO: Can be simplified in RC22
            TriggeringFilterBox::from(BySome(DataEntityFilter::ByTrigger(BySome(
                TriggerFilter::new(
                    BySome(OriginFilter::new(register_bond_trigger_id)),
                    BySome(TriggerEventFilter::ByMetadataInserted),
                ),
            )))),
        ),
    );

    let buy_bonds_trigger_id: TriggerId = "buy_bonds_trigger".parse().unwrap();
    let buy_bonds_trigger = Trigger::new(
        buy_bonds_trigger_id.clone(),
        Action::new(
            buy_bonds_wasm,
            Repeats::Indefinitely,
            account_id.clone(),
            // TODO: Can be simplified in RC22
            TriggeringFilterBox::from(BySome(DataEntityFilter::ByTrigger(BySome(
                TriggerFilter::new(
                    BySome(OriginFilter::new(buy_bonds_trigger_id)),
                    BySome(TriggerEventFilter::ByMetadataInserted),
                ),
            )))),
        ),
    );

    println!("Registering register_bond trigger...");
    iroha.submit_blocking(RegisterExpr::new(register_bond_trigger))?;
    println!("Registering buy_bonds trigger...");
    iroha.submit_blocking(RegisterExpr::new(buy_bonds_trigger))?;

    Ok(())
}

fn register_bond(iroha: &Client, new_bond: <AssetDefinition as Registered>::With) -> Result<()> {
    let register_bond_trigger_id: TriggerId = "register_bond".parse().unwrap();

    let set_key = SetKeyValueExpr::new(
        register_bond_trigger_id,
        "bond".parse::<Name>()?,
        new_bond.clone(),
    );

    iroha.submit_blocking(set_key)?;

    Ok(())
}

fn create_new_bond() -> <AssetDefinition as Registered>::With {
    let curr_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let limits = Limits::new(1024, 1024);

    let mut bond_metadata = Metadata::new();
    bond_metadata
        .insert_with_limits("currency".parse().unwrap(), "USD".to_owned().into(), limits)
        .unwrap();
    bond_metadata
        .insert_with_limits(
            "nominal_value".parse().unwrap(),
            100_f64.try_into().unwrap(),
            limits,
        )
        .unwrap();
    bond_metadata
        .insert_with_limits(
            "coupon_rate".parse().unwrap(),
            0.1.try_into().unwrap(),
            limits,
        )
        .unwrap();

    bond_metadata
        .insert_with_limits(
            "maturation_date_ms".parse().unwrap(),
            ((curr_time + Duration::from_secs(10)).as_millis() as u64).into(),
            limits,
        )
        .unwrap();
    bond_metadata
        .insert_with_limits(
            "registration_time_ms".parse().unwrap(),
            (curr_time.as_millis() as u64).into(),
            limits,
        )
        .unwrap();

    AssetDefinition::new(
        "t-bond#palau".parse().unwrap(),
        AssetValueType::Quantity,
    )
    .with_metadata(bond_metadata)
}

fn main() -> Result<()> {
    // Prepare blockchain
    let iroha = Client::new(&ConfigurationProxy::from_path("configs/client.json").build()?)?;
    register_triggers(&iroha)?;

    //println!("Waiting for 5 seconds...");
    //sleep(Duration::from_secs(5));

    // Register new bond
    let new_bond = create_new_bond();
    register_bond(&iroha, new_bond)?;

    Ok(())
}
