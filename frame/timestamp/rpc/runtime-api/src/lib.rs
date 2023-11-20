#![cfg_attr(not(feature = "std"), no_std)]

use codec::Codec;
use sp_runtime::traits::Block as BlockT;

sp_api::decl_runtime_apis! {
	pub trait TimestampApi<Time>
	where
		Time: Codec
	{
		fn get_set_time_extrinsic(time: Time) -> <Block as BlockT>::Extrinsic;
	}
}
