#![cfg_attr(not(feature = "std"), no_std)]

use codec::Codec;

sp_api::decl_runtime_apis! {
	pub trait TimestampApi<Time>
	where
		Time: Codec
	{
		fn set_time(time: Time);
	}
}
