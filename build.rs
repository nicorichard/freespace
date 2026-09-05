// Embeds the vendored module catalog into the binary at compile time.
//
// Walks `catalog/*/module.toml` and generates a `CATALOG` table of
// `(dir_name, manifest_contents)` pairs into `$OUT_DIR/catalog.rs`, which
// `src/module/catalog.rs` includes. Generated rather than hand-written so the
// table can never drift from the directory contents.

use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let catalog_dir = Path::new(&manifest_dir).join("catalog");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let dest = Path::new(&out_dir).join("catalog.rs");

    println!("cargo:rerun-if-changed=catalog");
    println!("cargo:rerun-if-changed=build.rs");

    let mut entries: Vec<(String, String)> = Vec::new();

    if catalog_dir.is_dir() {
        let dirs = fs::read_dir(&catalog_dir)
            .unwrap_or_else(|e| panic!("could not read {}: {e}", catalog_dir.display()));

        for entry in dirs {
            let entry = entry.expect("catalog directory entry");
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest = path.join("module.toml");
            if !manifest.exists() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let abs = manifest
                .canonicalize()
                .unwrap_or(manifest)
                .to_string_lossy()
                .into_owned();
            println!("cargo:rerun-if-changed={abs}");
            entries.push((name, abs));
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::from(
        "/// Embedded module catalog: `(directory name, manifest contents)`.\n\
         pub static CATALOG: &[(&str, &str)] = &[\n",
    );
    for (name, abs) in &entries {
        out.push_str(&format!(
            "    ({:?}, include_str!({:?})),\n",
            name.as_str(),
            abs.as_str()
        ));
    }
    out.push_str("];\n");

    fs::write(&dest, out).unwrap_or_else(|e| panic!("could not write {}: {e}", dest.display()));
}
