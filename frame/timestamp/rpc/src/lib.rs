use std::{convert::TryInto, sync::Arc};

use codec::Codec;
use futures::future::TryFutureExt;
use jsonrpsee::{
	core::{async_trait, RpcResult},
	proc_macros::rpc,
	types::error::{CallError, ErrorObject},
};
use sc_transaction_pool_api::TransactionPool;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_rpc::number::NumberOrHex;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, MaybeDisplay},
};
use std::marker::{Send, Sync};

pub use pallet_timestamp_rpc_runtime_api::TimestampApi as TimestampRuntimeApi;

/// RPC trait that provides methods for interacting with the dev timestamp functionalities.
#[rpc(server)]
#[async_trait]
pub trait TimestampApi<BlockHash, Time> {
	#[method(name = "timestamp_setTime")]
	async fn set_time(&self, time: Time) -> RpcResult<()>;
}

/// Error type of this RPC api.
pub enum Error {
	/// The transaction was not decodable.
	DecodeError,
	/// The call to runtime failed.
	RuntimeError,
}

impl From<Error> for i32 {
	fn from(e: Error) -> i32 {
		match e {
			Error::RuntimeError => 1,
			Error::DecodeError => 2,
		}
	}
}

/// Provides RPC methods to query a dispatchable's class, weight and fee.
pub struct TimestampRPC<C> {
	/// Shared reference to the client.
	client: Arc<C>,
}

impl<C> TimestampRPC<C> {
	/// Creates a new instance of the TransactionPayment Rpc helper.
	pub fn new(client: Arc<C>) -> Self {
		Self { client }
	}
}

#[async_trait]
impl<Client, Block, Time> TimestampApiServer<Block, Time> for TimestampRPC<Client>
where
	Block: BlockT,
	Client: Send + Sync + 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: TimestampRuntimeApi<Block, Time>,
	Time: Codec + MaybeDisplay + Copy + TryInto<u64> + Send + Sync + 'static,
{
	async fn set_time(&self, time: Time) -> RpcResult<()> {
		let best_block_hash = self.client.info().best_hash;
		self.client.runtime_api().set_time(best_block_hash, time).map_err(|e| {
			err(Error::RuntimeError.into(), "Unable to set time.", Some(&e.to_string().as_bytes()))
		})
	}
}

pub fn err<T: ToString>(code: i32, message: T, data: Option<&[u8]>) -> jsonrpsee::core::Error {
	jsonrpsee::core::Error::Call(jsonrpsee::types::error::CallError::Custom(
		jsonrpsee::types::error::ErrorObject::owned(
			code,
			message.to_string(),
			data.map(|bytes| {
				jsonrpsee::core::to_json_raw_value(&format!("0x{}", hex::encode(bytes)))
					.expect("fail to serialize data")
			}),
		),
	))
}

pub fn internal_err<T: ToString>(message: T) -> jsonrpsee::core::Error {
	err(jsonrpsee::types::error::INTERNAL_ERROR_CODE, message, None)
}

pub fn internal_err_with_data<T: ToString>(message: T, data: &[u8]) -> jsonrpsee::core::Error {
	err(jsonrpsee::types::error::INTERNAL_ERROR_CODE, message, Some(data))
}
