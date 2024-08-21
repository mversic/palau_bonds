//! Executor for Palau T-bonds

#![no_std]

extern crate alloc;
#[cfg(not(test))]
extern crate panic_halt;

use iroha_executor::{default::default_permission_token_schema, prelude::*, smart_contract};
use dlmalloc::GlobalDlmalloc;

#[global_allocator]
static ALLOC: GlobalDlmalloc = GlobalDlmalloc;

/// Executor that replaces some of [`Validate`]'s methods with sensible defaults
///
/// # Warning
///
/// The defaults are not guaranteed to be stable.
#[derive(Clone, Constructor, Debug, ValidateEntrypoints, ExpressionEvaluator, Validate, Visit)]
pub struct Executor {
    verdict: Result,
    block_height: u64,
    host: smart_contract::Host,
}

/// Migrate previous executor to the current version.
/// Called by Iroha once just before upgrading executor.
#[entrypoint]
pub fn migrate(_block_height: u64) -> MigrationResult {
    let schema = default_permission_token_schema();
    let (token_ids, schema_str) = schema.serialize();

    iroha_executor::set_permission_token_schema(
        &iroha_executor::data_model::permission::PermissionTokenSchema::new(token_ids, schema_str),
    );

    Ok(())
}

