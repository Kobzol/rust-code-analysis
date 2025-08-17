use rustdoc_types::{GenericArg, GenericArgs, ItemEnum};
use scan_impls::download_top_n_crates;
use serde::de::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn run_cargo_doc(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut json_files = Vec::new();
    let mut found_krates = HashSet::new();
    for krate in std::fs::read_dir(dir)? {
        let krate = krate?;
        eprintln!("Documenting {:?}", krate.path());
        std::process::Command::new("cargo")
            .arg("+nightly")
            .arg("doc")
            .env("RUSTDOCFLAGS", "-Zunstable-options --output-format json --document-private-items --document-hidden-items --cap-lints=allow")
            .current_dir(krate.path())
            .spawn()?
            .wait()?;
        for file in krate.path().join("target").join("doc").read_dir()? {
            let file = file?;
            if file.path().extension().unwrap_or_default() == "json" {
                let name = file
                    .path()
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string();
                if !found_krates.contains(&name) {
                    found_krates.insert(name.to_string());
                    json_files.push(file.path());
                }
            }
        }
    }
    Ok(json_files)
}

fn main() -> anyhow::Result<()> {
    let dir = Path::new("crates");
    download_top_n_crates(dir, 100)?;
    let json_files = run_cargo_doc(dir)?;

    println!("Found {} crates", json_files.len());

    let mut found_structs = 0;
    let mut structs_implement_from = 0;
    for file in json_files {
        let data = std::fs::read_to_string(&file)?;

        let mut deserializer = serde_json::Deserializer::from_str(&data);
        deserializer.disable_recursion_limit();
        let deserializer = serde_stacker::Deserializer::new(&mut deserializer);
        let krate = rustdoc_types::Crate::deserialize(deserializer)?;

        for (_id, item) in &krate.index {
            if item.attrs.iter().any(|attr| attr.contains("derive")) {
                continue;
            }
            let ItemEnum::Struct(s) = &item.inner else {
                continue;
            };
            let field = match &s.kind {
                rustdoc_types::StructKind::Tuple(ids) if ids.len() == 1 && ids[0].is_some() => {
                    ids[0].unwrap()
                }
                //rustdoc_types::StructKind::Plain { fields, .. } if fields.len() == 1 => fields[0],
                _ => continue,
            };
            let Some(field) = krate.index.get(&field) else {
                continue;
            };
            let ItemEnum::StructField(field_ty) = &field.inner else {
                continue;
            };

            found_structs += 1;
            let mut implements_from_field = false;
            for imp in &s.impls {
                let Some(impl_block) = krate.index.get(&imp) else {
                    continue;
                };
                let ItemEnum::Impl(imp) = &impl_block.inner else {
                    continue;
                };
                let Some(trait_) = &imp.trait_ else {
                    continue;
                };
                if trait_.path.ends_with("From") {
                    let Some(args) = &trait_.args else {
                        continue;
                    };
                    let GenericArgs::AngleBracketed { args, .. } = &**args else {
                        continue;
                    };
                    if args.len() != 1 {
                        continue;
                    }
                    let GenericArg::Type(ty) = &args[0] else {
                        continue;
                    };
                    if ty == field_ty {
                        implements_from_field = true;
                        break;
                    }
                }
            }
            if implements_from_field {
                structs_implement_from += 1;
            }
        }
    }

    println!("Found tuple newtype structs: {found_structs}");
    println!("Implements From for the single field: {structs_implement_from}");

    Ok(())
}
