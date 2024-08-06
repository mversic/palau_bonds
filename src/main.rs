
fn main() {
    let wasm = iroha_wasm_builder::Builder::new("../smart_contracts/register_bond")
        // TODO: Available in RC22
        //.show_output()
        .build()?
        .optimize()?
        .into_bytes()?;
    let wasm = WasmSmartContract::from_compiled(wasm);

    let trigger = Trigger::new(
        multisig_register_trigger_id.clone(),
        Action::new(
            wasm,
            Repeats::Indefinitely,
            account_id.clone(),
            ExecuteTriggerEventFilter::new().for_trigger(multisig_register_trigger_id.clone()),
        ),
    );
}
