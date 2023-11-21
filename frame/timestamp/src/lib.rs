#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

use frame_support::traits::{OnTimestampSet, Time, UnixTime};
use sp_runtime::traits::{AtLeast32Bit, SaturatedConversion, Scale, Zero};
use sp_std::{cmp, result};
use sp_timestamp::{InherentError, InherentType, INHERENT_IDENTIFIER};
pub use weights::WeightInfo;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{derive_impl, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	/// Default preludes for [`Config`].
	pub mod config_preludes {
		use super::*;

		/// Default prelude sensible to be used in a testing environment.
		pub struct TestDefaultConfig;

		#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig, no_aggregated_types)]
		impl frame_system::DefaultConfig for TestDefaultConfig {}

		#[frame_support::register_default_impl(TestDefaultConfig)]
		impl DefaultConfig for TestDefaultConfig {
			type Moment = u64;
			type OnTimestampSet = ();
			type MinimumPeriod = frame_support::traits::ConstU64<1>;
			type WeightInfo = ();
		}
	}

	/// The pallet configuration trait
	#[pallet::config(with_default)]
	pub trait Config: frame_system::Config {
		/// Type used for expressing a timestamp.
		#[pallet::no_default_bounds]
		type Moment: Parameter
			+ Default
			+ AtLeast32Bit
			+ Scale<BlockNumberFor<Self>, Output = Self::Moment>
			+ Copy
			+ MaxEncodedLen
			+ scale_info::StaticTypeInfo;

		/// Something which can be notified (e.g. another pallet) when the timestamp is set.
		///
		/// This can be set to `()` if it is not needed.
		type OnTimestampSet: OnTimestampSet<Self::Moment>;

		/// The minimum period between blocks.
		///
		/// Be aware that this is different to the *expected* period that the block production
		/// apparatus provides. Your chosen consensus system will generally work with this to
		/// determine a sensible block time. For example, in the Aura pallet it will be double this
		/// period on default settings.
		#[pallet::constant]
		type MinimumPeriod: Get<Self::Moment>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The current time for the current block.
	#[pallet::storage]
	#[pallet::getter(fn now)]
	pub type Now<T: Config> = StorageValue<_, T::Moment, ValueQuery>;

	/// Fake time for the current block.
	#[pallet::storage]
	#[pallet::getter(fn fake_now)]
	pub type FakeNow<T: Config> = StorageValue<_, T::Moment, ValueQuery>;

	/// Whether the timestamp has been updated in this block.
	///
	/// This value is updated to `true` upon successful submission of a timestamp by a node.
	/// It is then checked at the end of each block execution in the `on_finalize` hook.
	#[pallet::storage]
	pub(super) type DidUpdate<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// A dummy `on_initialize` to return the amount of weight that `on_finalize` requires to
		/// execute.
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// weight of `on_finalize`
			T::WeightInfo::on_finalize()
		}

		/// At the end of block execution, the `on_finalize` hook checks that the timestamp was
		/// updated. Upon success, it removes the boolean value from storage. If the value resolves
		/// to `false`, the pallet will panic.
		///
		/// ## Complexity
		/// - `O(1)`
		fn on_finalize(_n: BlockNumberFor<T>) {
			assert!(DidUpdate::<T>::take(), "Timestamp must be updated once in the block");
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the current time.
		///
		/// This call should be invoked exactly once per block. It will panic at the finalization
		/// phase, if this call hasn't been invoked by that time.
		///
		/// The timestamp should be greater than the previous one by the amount specified by
		/// [`Config::MinimumPeriod`].
		///
		/// The dispatch origin for this call must be _None_.
		///
		/// This dispatch class is _Mandatory_ to ensure it gets executed in the block. Be aware
		/// that changing the complexity of this call could result exhausting the resources in a
		/// block to execute any other calls.
		///
		/// ## Complexity
		/// - `O(1)` (Note that implementations of `OnTimestampSet` must also be `O(1)`)
		/// - 1 storage read and 1 storage mutation (codec `O(1)` because of `DidUpdate::take` in
		///   `on_finalize`)
		/// - 1 event handler `on_timestamp_set`. Must be `O(1)`.
		#[pallet::call_index(0)]
		#[pallet::weight((
			T::WeightInfo::set(),
			DispatchClass::Mandatory
		))]
		pub fn set(origin: OriginFor<T>, #[pallet::compact] now: T::Moment) -> DispatchResult {
			ensure_none(origin)?;
			let fake_time = Self::fake_now();
			if fake_time != T::Moment::zero() {
				Now::<T>::put(fake_time);
				DidUpdate::<T>::put(true);

				<T::OnTimestampSet as OnTimestampSet<_>>::on_timestamp_set(fake_time);

				return Ok(())
			}
			assert!(
				!DidUpdate::<T>::exists(),
				"Timestamp must be updated only once in the
				block"
			);
			let prev = Self::now();
			assert!(
				prev.is_zero() || now >= prev + T::MinimumPeriod::get(),
				"Timestamp must increment by at least <MinimumPeriod> between sequential blocks"
			);
			Now::<T>::put(now);
			DidUpdate::<T>::put(true);

			<T::OnTimestampSet as OnTimestampSet<_>>::on_timestamp_set(now);

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set())]
		pub fn set_time(
			_origin: OriginFor<T>,
			#[pallet::compact] time: T::Moment,
		) -> DispatchResult {
			FakeNow::<T>::put(time);
			Ok(())
		}
	}

	/// To check the inherent is valid, we simply take the max value between the current timestamp
	/// and the current timestamp plus the [`Config::MinimumPeriod`].
	/// We also check that the timestamp has not already been set in this block.
	///
	/// ## Errors:
	/// - [`InherentError::TooFarInFuture`]: If the timestamp is larger than the current timestamp +
	///   minimum drift period.
	/// - [`InherentError::TooEarly`]: If the timestamp is less than the current + minimum period.
	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = InherentError;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let inherent_data = data
				.get_data::<InherentType>(&INHERENT_IDENTIFIER)
				.expect("Timestamp inherent data not correctly encoded")
				.expect("Timestamp inherent data must be provided");
			let data = (*inherent_data).saturated_into::<T::Moment>();

			let next_time = cmp::max(data, Self::now() + T::MinimumPeriod::get());
			Some(Call::set { now: next_time })
		}

		fn check_inherent(
			call: &Self::Call,
			data: &InherentData,
		) -> result::Result<(), Self::Error> {
			const MAX_TIMESTAMP_DRIFT_MILLIS: sp_timestamp::Timestamp =
				sp_timestamp::Timestamp::new(30 * 1000);

			let t: u64 = match call {
				Call::set { ref now } => (*now).saturated_into::<u64>(),
				_ => return Ok(()),
			};

			let data = data
				.get_data::<InherentType>(&INHERENT_IDENTIFIER)
				.expect("Timestamp inherent data not correctly encoded")
				.expect("Timestamp inherent data must be provided");

			let minimum = (Self::now() + T::MinimumPeriod::get()).saturated_into::<u64>();
			if t > *(data + MAX_TIMESTAMP_DRIFT_MILLIS) {
				Err(InherentError::TooFarInFuture)
			} else if t < minimum {
				Err(InherentError::TooEarly)
			} else {
				Ok(())
			}
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::set { .. })
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Get the current time for the current block.
	///
	/// NOTE: if this function is called prior to setting the timestamp,
	/// it will return the timestamp of the previous block.
	pub fn get() -> T::Moment {
		let fake_time = Self::fake_now();
		if fake_time != T::Moment::zero() {
			return fake_time
		}
		Self::now()
	}

	/// Set the timestamp to something in particular. Only used for tests.
	#[cfg(any(feature = "runtime-benchmarks", feature = "std"))]
	pub fn set_timestamp(now: T::Moment) {
		Now::<T>::put(now);
		DidUpdate::<T>::put(true);
		<T::OnTimestampSet as OnTimestampSet<_>>::on_timestamp_set(now);
	}
}

impl<T: Config> Time for Pallet<T> {
	/// A type that represents a unit of time.
	type Moment = T::Moment;

	fn now() -> Self::Moment {
		let fake_time = Self::fake_now();
		if fake_time != T::Moment::zero() {
			return fake_time
		}
		Self::now()
	}
}

/// Before the timestamp inherent is applied, it returns the time of previous block.
///
/// On genesis the time returned is not valid.
impl<T: Config> UnixTime for Pallet<T> {
	fn now() -> core::time::Duration {
		let fake_time = Self::fake_now();
		if fake_time != T::Moment::zero() {
			return core::time::Duration::from_millis(fake_time.saturated_into::<u64>())
		}
		// now is duration since unix epoch in millisecond as documented in
		// `sp_timestamp::InherentDataProvider`.
		let now = Self::now();
		sp_std::if_std! {
			if now == T::Moment::zero() {
				log::error!(
					target: "runtime::timestamp",
					"`pallet_timestamp::UnixTime::now` is called at genesis, invalid value returned: 0",
				);
			}
		}
		core::time::Duration::from_millis(now.saturated_into::<u64>())
	}
}
