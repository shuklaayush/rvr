#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use alloc::{boxed::Box, format, vec::Vec};
use reth_ethereum_primitives::TransactionSigned;
use reth_stateless::UncompressedPublicKey;
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
use std::{boxed::Box, format, vec::Vec};

pub fn recover_signers<'a, I>(
    txs: I,
) -> Result<Vec<UncompressedPublicKey>, Box<dyn core::error::Error>>
where
    I: IntoIterator<Item = &'a TransactionSigned>,
{
    txs.into_iter()
        .enumerate()
        .map(|(i, tx)| {
            tx.signature()
                .recover_from_prehash(&tx.signature_hash())
                .map(|keys| {
                    UncompressedPublicKey(
                        TryInto::<[u8; 65]>::try_into(keys.to_encoded_point(false).as_bytes())
                            .unwrap(),
                    )
                })
                .map_err(|e| format!("failed to recover signature for tx #{i}: {e}").into())
        })
        .collect::<Result<Vec<UncompressedPublicKey>, _>>()
}
