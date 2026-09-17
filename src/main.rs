//! Desktop entry point. The app itself lives in the library so that the same
//! code can be compiled as an Android `cdylib`; see `src/lib.rs`.

fn main() {
    bluerush::main();
}
