// Runtime snapshot is empty for now since we're not using snapshots yet
pub static RUNTIME_SNAPSHOT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/MYCO_SNAPSHOT.bin"));

pub const MAIN_JS: &str = "\
const Myco = globalThis.Myco;

// Delete the global scope that we don't want access to
delete globalThis.Myco;

const {default: userModule} = await import('{{USER_MODULE}}');

// Call the user module and capture the result
const result = await userModule(Myco);

// Store the exit code in a global variable that Rust can access
globalThis.__MYCO_EXIT_CODE__ = typeof result === 'number' ? result : 0;
";

#[repr(C, align(16))]
pub struct IcuData<T: ?Sized>(pub T);
pub static ICU_DATA: &IcuData<[u8]> = &IcuData(*include_bytes!("icudtl.dat"));
