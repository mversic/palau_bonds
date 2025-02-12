use std::time::{SystemTime, UNIX_EPOCH};

use bonds_data_model::{BondDetails, RegisterBondArgs};
use eyre::Result;
use iroha::{
    client::Client,
    config::Config,
    data_model::prelude::{TransactionBuilder, *},
};
use iroha_trigger::data_model::{
    asset::{AssetDefinition, AssetDefinitionId},
    metadata::Metadata,
    prelude::Numeric,
    Registered,
};

fn register_triggers(iroha: &Client) -> Result<()> {
    // TODO: Get from config in RC22
    let issuer: AccountId =
        "ed01207233BFC89DCBD68C19FDE6CE6158225298EC1131B6A130D1AEB454C1AB5183C0@palau"
            .parse()
            .unwrap();

    println!("Building register_bond trigger...");
    let register_bond_wasm = WasmSmartContract::from_compiled(
        iroha_wasm_builder::Builder::new("smart_contracts/register_bond")
            .show_output()
            .build()?
            .optimize()?
            .into_bytes()?,
    );

    println!("Building buy_bonds trigger...");
    let buy_bonds_wasm = WasmSmartContract::from_compiled(
        iroha_wasm_builder::Builder::new("smart_contracts/buy_bonds")
            .show_output()
            .build()?
            .optimize()?
            .into_bytes()?,
    );

    println!("Building redeem_bonds trigger...");
    let redeem_bonds_wasm = WasmSmartContract::from_compiled(
        iroha_wasm_builder::Builder::new("smart_contracts/redeem_bonds")
            .show_output()
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
            issuer.clone(),
            ExecuteTriggerEventFilter::new()
                .for_trigger(register_bond_trigger_id)
                .under_authority(issuer.clone()),
        ),
    );

    let buy_bonds_trigger_id: TriggerId = "buy_bonds_trigger".parse().unwrap();
    let buy_bonds_trigger = Trigger::new(
        buy_bonds_trigger_id.clone(),
        Action::new(
            buy_bonds_wasm,
            Repeats::Indefinitely,
            issuer.clone(),
            AccountEventFilter::new().for_events(AccountEventSet::MetadataInserted),
        ),
    );

    let redeem_bonds_trigger_id: TriggerId = "redeem_bonds_trigger".parse().unwrap();
    let redeem_bonds_trigger = Trigger::new(
        redeem_bonds_trigger_id.clone(),
        Action::new(
            redeem_bonds_wasm,
            Repeats::Indefinitely,
            issuer.clone(),
            AccountEventFilter::new().for_events(AccountEventSet::MetadataInserted),
        ),
    );

    println!("Registering register_bond trigger...");
    iroha.submit_blocking(Register::trigger(register_bond_trigger))?;
    println!("Registering buy_bonds trigger...");
    iroha.submit_blocking(Register::trigger(buy_bonds_trigger))?;
    println!("Registering redeem_bonds trigger...");
    iroha.submit_blocking(Register::trigger(redeem_bonds_trigger))?;

    Ok(())
}

fn register_bond(iroha: &Client, new_bond: <AssetDefinition as Registered>::With) -> Result<()> {
    let register_bond_trigger_id: TriggerId = "register_bond".parse()?;

    let args = RegisterBondArgs { bond: new_bond };
    let set_key = ExecuteTrigger::new(register_bond_trigger_id).with_args(&args);

    println!("Registering new bond...");
    iroha.submit_blocking(set_key)?;

    Ok(())
}

fn create_new_bond() -> <AssetDefinition as Registered>::With {
    let curr_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    let currency_id: AssetDefinitionId = "USD#palau".parse().unwrap();
    let fee_beneficiary: AccountId =
        "ed01207233BFC89DCBD68C19FDE6CE6158225298EC1131B6A130D1AEB454C1AB5183C0@palau"
            .parse()
            .unwrap();

    let bond_details = BondDetails {
        currency: JsonString::new(currency_id),
        nominal_value: numeric!(100_000), // 100_000$ to make the final value 0.19$
        quantity: numeric!(100),
        coupon_rate: numeric!(0.1), //10%
        registration_time_sec: Numeric::from(curr_time.as_secs()),
        maturation_date_sec: Numeric::from(curr_time.as_secs() + 60),
        payment_frequency_sec: Numeric::from(10_u32),
        fee: numeric!(0.1),
        fee_beneficiary: JsonString::new(fee_beneficiary),
    };

    let mut bond_metadata = Metadata::default();
    bond_metadata.insert("details".parse().unwrap(), bond_details);
    AssetDefinition::numeric("t-bond#palau".parse().unwrap()).with_metadata(bond_metadata)
}

fn buy_bonds(iroha: &Client) -> Result<()> {
    let buyer: AccountId =
        "ed0120c65bfe5bc9ab3c6bb04d6677314a95cfca46c55f371a20df577e5857f3eef518@palau".parse()?;
    let bond_id: AssetDefinitionId = "t-bond#palau".parse()?;

    let mut buy_order = Metadata::default();
    buy_order.insert("bond".parse()?, JsonString::new(bond_id));
    buy_order.insert("quantity".parse()?, 1_u32);

    println!("Buying bond...");
    let private_key =
        "80262081fc42dd4170f0e00202b468cd02151871ccb7d06c38e86256d80542254a5516".parse()?;

    let tx = TransactionBuilder::new(iroha.chain.clone(), buyer.clone())
        .with_instructions([SetKeyValue::account(
            buyer,
            "buy_bonds".parse::<Name>()?,
            JsonString::new(buy_order),
        )])
        .sign(&private_key);

    iroha.submit_transaction_blocking(&tx)?;

    Ok(())
}

fn main() -> Result<()> {
    // Prepare blockchain
    let iroha = Client::new(Config::load("configs/client.toml").unwrap());
    register_triggers(&iroha)?;

    // Register new bond
    let new_bond = create_new_bond();
    register_bond(&iroha, new_bond)?;

    // Buy some bonds
    buy_bonds(&iroha)?;

    Ok(())
}
