use std::path::Path;

fn main() {
    if Path::new("blueprint.json").exists() {
        println!("cargo:rerun-if-changed=src/lib.rs");
        println!("cargo:rerun-if-changed=src/main.rs");
    }

    blueprint_metadata::generate_json();
}
