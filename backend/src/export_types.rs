use std::path::PathBuf;

use manhunt_app_lib::mk_specta;
use specta_typescript::Typescript;

pub fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let path = args.get(1).expect("Usage: export-types path");
    let path = PathBuf::from(path)
        .canonicalize()
        .expect("Failed to canonicalize path");
    let specta = mk_specta();
    specta
        .export(Typescript::default(), &path)
        .expect("Failed to export types");
    println!(
        "Successfully exported type and commands to {}",
        path.to_str().unwrap()
    );
}
