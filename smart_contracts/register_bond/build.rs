//! Compile triggers for handling bond processing
use std::{io::Write as _, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    build_trigger("interest_payments")?;
    build_trigger("bond_maturation")?;

    Ok(())
}

fn build_trigger(trigger: &str) -> Result<(), Box<dyn std::error::Error>> {
    let trigger_dir = Path::new("..").join(trigger);
    println!("cargo::rerun-if-changed={}", trigger_dir.display());

    let out_dir = std::env::var("OUT_DIR").unwrap();
    eprintln!("{out_dir}");
    let wasm = iroha_wasm_builder::Builder::new(&trigger_dir)
        // TODO: Available in RC22
        //.show_output()
        .build()?
        .optimize()?
        .into_bytes()?;

    let mut file = std::fs::File::create(Path::new(&out_dir).join(format!("{trigger}.wasm")))?;
    file.write_all(&wasm)?;
    Ok(())
}
