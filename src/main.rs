use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eyre::Result;
use iroha_client::{
    client::Client,
    crypto::{Algorithm, KeyPair, PrivateKey},
    data_model::{
        asset::{AssetDefinition, AssetValueType},
        metadata::{Limits, Metadata},
        prelude::{TransactionBuilder, *},
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
            TriggeringFilterBox::from(BySome(DataEntityFilter::from(BySome(TriggerFilter::new(
                BySome(OriginFilter::new(register_bond_trigger_id)),
                BySome(TriggerEventFilter::ByMetadataInserted),
            ))))),
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
            TriggeringFilterBox::from(BySome(DataEntityFilter::from(BySome(AccountFilter::new(
                AcceptAll,
                BySome(AccountEventFilter::ByMetadataInserted),
            ))))),
        ),
    );

    println!("Registering register_bond trigger...");
    iroha.submit_blocking(RegisterExpr::new(register_bond_trigger))?;
    println!("Registering buy_bonds trigger...");
    iroha.submit_blocking(RegisterExpr::new(buy_bonds_trigger))?;

    Ok(())
}

fn register_bond(iroha: &Client, new_bond: <AssetDefinition as Registered>::With) -> Result<()> {
    let register_bond_trigger_id: TriggerId = "register_bond".parse()?;

    let set_key = SetKeyValueExpr::new(
        register_bond_trigger_id,
        "bond".parse::<Name>()?,
        new_bond.clone(),
    );

    println!("Registering new bond...");
    iroha.submit_blocking(set_key)?;

    Ok(())
}

fn create_new_bond() -> <AssetDefinition as Registered>::With {
    let curr_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let currency_id: AssetDefinitionId = "USD#palau".parse().unwrap();
    let limits = Limits::new(1024, 1024);
    let fee_recipient_account_id: AccountId = "government@palau".parse().unwrap();

    let mut bond_metadata = Metadata::new();
    bond_metadata
        .insert_with_limits("currency".parse().unwrap(), currency_id.into(), limits)
        .unwrap();
    bond_metadata
        .insert_with_limits(
            "quantity".parse().unwrap(),
            100_u32.into(),
            limits,
        )
        .unwrap();
    bond_metadata
        .insert_with_limits(
            "nominal_value".parse().unwrap(),
            100_000_f64.try_into().unwrap(), // 100_000$ to make the final value 0.19$
            limits,
        )
        .unwrap();
    bond_metadata
        .insert_with_limits(
            "coupon_rate".parse().unwrap(),
            0.1.try_into().unwrap(), //10%
            limits,
        )
        .unwrap();
    bond_metadata
        .insert_with_limits(
            "fixed_fee".parse().unwrap(),
            0.1_f64.try_into().unwrap(),
            limits,
        )
        .unwrap();
    bond_metadata
        .insert_with_limits(
            "fee_recipient_account_id".parse().unwrap(),
            fee_recipient_account_id.into(),
            limits,
        )
        .unwrap();

    bond_metadata
        .insert_with_limits(
            "maturation_date_ms".parse().unwrap(),
            ((curr_time + Duration::from_secs(120)).as_millis() as u64).into(),
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

    let payment_frequency_seconds = 60_u64;
    bond_metadata
        .insert_with_limits(
            "payment_frequency_seconds".parse().unwrap(),
            payment_frequency_seconds.try_into().unwrap(),
            limits,
        )
        .unwrap();

    AssetDefinition::new("t-bond#palau".parse().unwrap(), AssetValueType::Quantity)
        .with_metadata(bond_metadata)
}

fn buy_bonds(iroha: &Client) -> Result<()> {
    let buyer: AccountId = "citizen@palau".parse().unwrap();
    let bond_id: AssetDefinitionId = "t-bond#palau".parse()?;

    let limits = Limits::new(1024, 1024);
    let mut buy_order = Metadata::new();

    buy_order
        .insert_with_limits("bond".parse().unwrap(), bond_id.into(), limits)
        .unwrap();

    buy_order
        .insert_with_limits("quantity".parse().unwrap(), 1_u32.into(), limits)
        .unwrap();

    println!("Buying bond...");
    let keypair = KeyPair::new(
        "ed01207233BFC89DCBD68C19FDE6CE6158225298EC1131B6A130D1AEB454C1AB5183C0".parse()?,
        PrivateKey::from_hex(Algorithm::Ed25519, "9AC47ABF59B356E0BD7DCBBBB4DEC080E302156A48CA907E47CB6AEA1D32719E7233BFC89DCBD68C19FDE6CE6158225298EC1131B6A130D1AEB454C1AB5183C0".as_ref())?,
    )?;

    let tx = TransactionBuilder::new(buyer.clone())
        .with_instructions([SetKeyValueExpr::new(
            buyer,
            "buy_bonds".parse::<Name>()?,
            buy_order,
        )])
        .sign(keypair)?;

    iroha.submit_transaction_blocking(&tx)?;

    Ok(())
}

fn main() -> Result<()> {
    // Prepare blockchain
    let iroha = Client::new(&ConfigurationProxy::from_path("configs/client.json").build()?)?;
    register_triggers(&iroha)?;

    // Register new bond
    let new_bond = create_new_bond();
    register_bond(&iroha, new_bond)?;

    // Buy some bonds
    buy_bonds(&iroha)?;

    Ok(())
}
