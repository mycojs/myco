// Runtime snapshot is empty for now since we're not using snapshots yet
pub static RUNTIME_SNAPSHOT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/MYCO_SNAPSHOT.bin"));

#[repr(C, align(16))]
pub struct IcuData<T: ?Sized>(pub T);
pub static ICU_DATA: &IcuData<[u8]> = &IcuData(*include_bytes!("icudtl.dat"));
