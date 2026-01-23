#![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
#![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
extern crate alloc;

// Use rvr-rt for RISC-V runtime support (entry, panic, alloc, critical-section)
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use rvr_rt::BumpAlloc;

pub mod libs;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use alloc::{sync::Arc, vec::Vec};
use core::hint::black_box;
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
use std::{sync::Arc, vec::Vec};

use alloy_primitives::FixedBytes;
use reth_chainspec::ChainSpec;
use reth_ethereum_primitives::Block as EthBlock;
use reth_evm_ethereum::EthEvmConfig;
use reth_stateless::{
    stateless_validation_with_trie, trie::StatelessSparseTrie, ExecutionWitness, Genesis,
    StatelessInput, UncompressedPublicKey,
};

use crate::libs::senders::recover_signers;

// 100 MB heap for RISC-V builds
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[global_allocator]
static ALLOC: BumpAlloc<{ 100 * 1024 * 1024 }> = BumpAlloc::new();

fn validate_block(
    block: EthBlock,
    witness: ExecutionWitness,
    chain_spec: Arc<ChainSpec>,
    public_keys: Vec<UncompressedPublicKey>,
    evm_config: EthEvmConfig,
) -> FixedBytes<32> {
    let (block_hash, _) = stateless_validation_with_trie::<StatelessSparseTrie, _, _>(
        block,
        public_keys,
        witness,
        chain_spec,
        evm_config,
    )
    .expect("Block validation failed");

    block_hash
}

pub fn run() {
    let stateless_input: StatelessInput =
        serde_json::from_str(include_str!("../fixtures/22974575.json"))
            .expect("Failed to read input");

    let public_keys = recover_signers(stateless_input.block.body.transactions.iter())
        .expect("Failed to recover signers");

    let genesis = Genesis {
        config: stateless_input.chain_config.clone(),
        ..Default::default()
    };
    let chain_spec: Arc<ChainSpec> = Arc::new(genesis.into());
    let evm_config = EthEvmConfig::new(chain_spec.clone());

    let parent_hash = stateless_input.block.parent_hash;

    let block_hash = validate_block(
        stateless_input.block,
        stateless_input.witness,
        chain_spec,
        public_keys,
        evm_config,
    );

    let public_inputs = (block_hash.0, parent_hash.0, true);
    black_box(public_inputs);
}

// Entry point for RISC-V builds (called by rvr-rt's _start)
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[no_mangle]
pub extern "C" fn main() -> i32 {
    run();
    0
}

// Entry point for host builds
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
fn main() {
    run();
}
