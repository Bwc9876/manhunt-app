use std::borrow::Cow;

use manhunt_app_lib::mk_specta;
use specta_typescript::Typescript;

pub fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let path = args.get(1).expect("Usage: export-types path");
    let specta = mk_specta();
    let mut lang = Typescript::new();
    lang.header = Cow::Borrowed(
        "/* eslint @typescript-eslint/no-unused-vars: 0 */\n/* eslint @typescript-eslint/no-explicit-any: 0 */",
    );
    specta.export(lang, path).expect("Failed to export types");
    println!("Successfully exported types, events, and commands to {path}",);
}
