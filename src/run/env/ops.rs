use deno_core::op;

use crate::AnyError;

#[op]
fn myco_op_argv() -> Result<Vec<String>, AnyError> {
    println!("myco_op_argv: {}", std::env::args().collect::<Vec<String>>().join(", "));
    Ok(std::env::args().collect())
}
