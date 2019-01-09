fn main() {
    {
        extern crate parity_wasm;

        use std::env;
        use std::io::prelude::*;
        use std::path::Path;
        use std::process::Command;

        fn has_symbol(
            symbol: &str,
            exports: &[parity_wasm::elements::ExportEntry],
            funcs: &[parity_wasm::elements::Func],
            types: &[parity_wasm::elements::Type],
        ) -> Result<(), String> {
            match exports.iter().find(|ref export| export.field() == symbol) {
                Some(export) => match export.internal() {
                    &parity_wasm::elements::Internal::Function(fidx) => {
                        let tidx = funcs[fidx as usize].type_ref();
                        let parity_wasm::elements::Type::Function(t) = &types[tidx as usize];
                        match t.return_type() {
                            Some(parity_wasm::elements::ValueType::I32) => {}
                            _ => return Err(format!("Invalid return type for `{}", symbol)),
                        }
                        let params = t.params();
                        if params.len() != 0 {
                            Err(format!("Invalid number of parameters for `{}`", symbol))
                        } else {
                            Ok(())
                        }
                    }
                    _ => {
                        return Err(format!(
                            "Module has a `{}` export that is not a function",
                            symbol
                        ));
                    }
                },
                None => Err(format!("Module is missing a `{}` export", symbol)),
            }
        }

        fn validate<U: Into<String>>(input: U) -> Result<parity_wasm::elements::Module, String> {
            let fname = input.into();

            let module = parity_wasm::deserialize_file(fname.clone()).map_err(|e| e.to_string())?;

            let functions = module
                .function_section()
                .ok_or("Module has no function section")?
                .entries();
            let types = module
                .type_section()
                .ok_or("Module has no function section")?
                .types();
            let exports = module
                .export_section()
                .ok_or("Module has no export section")?
                .entries();

            /* Look for `get_input_offs` */
            has_symbol("get_inputs_off", exports, functions, types)?;

            /* Look for `solve` */
            has_symbol("solve", exports, functions, types)?;

            Ok(module.clone())
        }

        fn add_global(
            symbol: &str,
            module: &parity_wasm::elements::Module,
            value: i32,
        ) -> Result<parity_wasm::elements::Module, String> {
            let nglobals = module.global_section().unwrap().entries().len();
            println!("Adding global at index {}", nglobals);
            let nm = parity_wasm::builder::from_module(module.clone())
                .global()
                .value_type()
                .i32()
                .init_expr(parity_wasm::elements::Instruction::I32Const(value))
                .build()
                .export()
                .field(symbol)
                .internal()
                .global(nglobals as u32)
                .build()
                .build();
            Ok(nm)
        }

        fn add_global_if_missing(
            symbol: &str,
            module: &parity_wasm::elements::Module,
            expected_type: parity_wasm::elements::ValueType,
            value: i32,
            _force: bool,
        ) -> Result<parity_wasm::elements::Module, String> {
            let global_section = module
                .global_section()
                .ok_or("Could not get globals section")?
                .entries()
                .clone();
            let exports = module
                .export_section()
                .ok_or("Could not get exports section")?
                .entries();

            let mut found = false;

            if let Some(export) = exports.iter().find(|ref export| export.field() == symbol) {
                found = true;

                // Export already exists, check its type and return it if said
                // type is correct.
                if let &parity_wasm::elements::Internal::Global(gidx) = export.internal() {
                    let global_type = global_section[gidx as usize].global_type();
                    if !global_type.is_mutable() && expected_type == global_type.content_type() {
                        return Ok(module.clone());
                    }
                }
            }

            /* overwrite only if asked with the -f switch */
            if !found {
                add_global(symbol, module, value)
            } else {
                Err(format!(
                    "Symbol {} is already present with a different type in module",
                    symbol
                ))
            }
        }

        /* Turn the output binary into a source file for zokrates_core */
        fn wasm2rs(fname: &str, modname: &str) {
            match validate(fname) {
                Ok(module) => {
                    let out_dir = env::var("OUT_DIR").unwrap();
                    let dest_path = Path::new(&out_dir).join(format!("{}.rs", modname));
                    println!("out= {}", dest_path.display());
                    let m0 = module.clone();
                    let m1 = add_global_if_missing(
                        "min_inputs",
                        &m0,
                        parity_wasm::elements::ValueType::I32,
                        1,
                        false,
                    )
                    .unwrap();
                    let m2 = add_global_if_missing(
                        "min_outputs",
                        &m1,
                        parity_wasm::elements::ValueType::I32,
                        2,
                        false,
                    )
                    .unwrap();
                    let m3 = add_global_if_missing(
                        "field_size",
                        &m2,
                        parity_wasm::elements::ValueType::I32,
                        32,
                        false,
                    )
                    .unwrap();
                    let buf = parity_wasm::serialize(m3).unwrap();
                    std::fs::File::create(dest_path)
                        .unwrap()
                        .write_all(
                            format!(
                                "
                                #[allow(dead_code)]
                                pub const {} : &'static [u8] = &{:?};
                                ",
                                modname.to_uppercase(),
                                buf
                            )
                            .as_bytes(),
                        )
                        .unwrap();
                }
                Err(e) => panic!(format!("Module validation error: {}", e.to_string())),
            }
        }

        /* Regenerate if files have changed */
        println!("cargo:rerun-if-changed=./plugins");

        /* Build the WASM helpers and turn them into files */
        let status = Command::new("cargo")
            .current_dir("../plugins/conditioneq_wasm")
            .args(&["build", "--target", "wasm32-unknown-unknown", "--release"])
            .status()
            .unwrap();
        if !status.success() {
            panic!("Error building WASM helpers");
        }

        /* Scan the plugins directory and compile them */
        if let Ok(entries) = std::fs::read_dir("../plugins") {
            for entry in entries {
                if let Ok(entry) = entry {
                    if let Ok(ftype) = entry.file_type() {
                        if ftype.is_dir() {
                            let fname = format!(
                                "{}/target/wasm32-unknown-unknown/release/{}.wasm",
                                entry.path().display(),
                                entry.file_name().to_str().unwrap()
                            );

                            wasm2rs(&fname, entry.file_name().to_str().unwrap());
                        }
                    }
                }
            }
        }
    }
}
