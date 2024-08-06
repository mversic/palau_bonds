use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bonds_data_model::{BondDetails, RegisterBondArgs};
use eyre::Result;
use iroha::{
    client::Client,
    config::Config,
    data_model::{
        asset::AssetDefinition,
        metadata::Metadata,
        prelude::{TransactionBuilder, *},
        Registered,
    },
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

    println!("Registering register_bond trigger... {}", iroha.account == issuer);
    iroha.submit_blocking(Register::trigger(register_bond_trigger))?;
    println!("Registering buy_bonds trigger...");
    iroha.submit_blocking(Register::trigger(buy_bonds_trigger))?;

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

    let bond_details = BondDetails {
        currency: "USD#palau".parse().unwrap(),
        nominal_value: numeric!(100_000), // 100_000$ to make the final value 0.19$
        quantity: numeric!(100),
        coupon_rate: numeric!(0.1), //10%
        registration_time: curr_time,
        maturation_date: curr_time + Duration::from_secs(120),
        payment_frequency: Duration::from_secs(60),
        fee: numeric!(0.1),
        fee_beneficiary:
            "ed01207233BFC89DCBD68C19FDE6CE6158225298EC1131B6A130D1AEB454C1AB5183C0@palau"
                .parse()
                .unwrap(),
    };

    let mut bond_metadata = Metadata::default();
    bond_metadata.insert("details".parse().unwrap(), bond_details);
    AssetDefinition::numeric("t-bond#palau".parse().unwrap()).with_metadata(bond_metadata)
}

fn buy_bonds(iroha: &Client) -> Result<()> {
    let buyer: AccountId =
        "ed012004FF5B81046DDCCF19E2E451C45DFB6F53759D4EB30FA2EFA807284D1CC33016@palau"
            .parse()
            .unwrap();
    let bond_id: AssetDefinitionId = "t-bond#palau".parse()?;

    let mut buy_order = Metadata::default();

    buy_order
        .insert("bond".parse().unwrap(), JsonString::new(bond_id))
        .unwrap();

    buy_order
        .insert("quantity".parse().unwrap(), 1_u32)
        .unwrap();

    println!("Buying bond...");
    let private_key =
        "8026209AC47ABF59B356E0BD7DCBBBB4DEC080E302156A48CA907E47CB6AEA1D32719E".parse()?;

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
