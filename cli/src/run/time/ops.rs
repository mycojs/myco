use deno_core::op;

use crate::AnyError;

#[op]
async fn myco_op_set_timeout(delay: u64) -> Result<(), AnyError> {
    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
    Ok(())
}
