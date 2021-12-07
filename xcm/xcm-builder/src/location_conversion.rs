// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use frame_support::traits::Get;
use parity_scale_codec::Encode;
use sp_io::hashing::blake2_256;
use sp_runtime::traits::AccountIdConversion;
use sp_std::{borrow::Borrow, marker::PhantomData};
use xcm::latest::{Junction::*, Junctions::*, MultiLocation, NetworkId, Parent};
use xcm_executor::traits::{Convert, InvertLocation};

pub struct Account32Hash<Network, AccountId>(PhantomData<(Network, AccountId)>);
impl<Network: Get<NetworkId>, AccountId: From<[u8; 32]> + Into<[u8; 32]> + Clone>
	Convert<MultiLocation, AccountId> for Account32Hash<Network, AccountId>
{
	fn convert_ref(location: impl Borrow<MultiLocation>) -> Result<AccountId, ()> {
		Ok(("multiloc", location.borrow()).using_encoded(blake2_256).into())
	}

	fn reverse_ref(_: impl Borrow<AccountId>) -> Result<MultiLocation, ()> {
		Err(())
	}
}

/// A [`MultiLocation`] consisting of a single `Parent` [`Junction`] will be converted to the
/// default value of `AccountId` (e.g. all zeros for `AccountId32`).
pub struct ParentIsDefault<AccountId>(PhantomData<AccountId>);
impl<AccountId: Default + Eq + Clone> Convert<MultiLocation, AccountId>
	for ParentIsDefault<AccountId>
{
	fn convert_ref(location: impl Borrow<MultiLocation>) -> Result<AccountId, ()> {
		if location.borrow().contains_parents_only(1) {
			Ok(AccountId::default())
		} else {
			Err(())
		}
	}

	fn reverse_ref(who: impl Borrow<AccountId>) -> Result<MultiLocation, ()> {
		if who.borrow() == &AccountId::default() {
			Ok(Parent.into())
		} else {
			Err(())
		}
	}
}

pub struct ChildParachainConvertsVia<ParaId, AccountId>(PhantomData<(ParaId, AccountId)>);
impl<ParaId: From<u32> + Into<u32> + AccountIdConversion<AccountId>, AccountId: Clone>
	Convert<MultiLocation, AccountId> for ChildParachainConvertsVia<ParaId, AccountId>
{
	fn convert_ref(location: impl Borrow<MultiLocation>) -> Result<AccountId, ()> {
		match location.borrow() {
			MultiLocation { parents: 0, interior: X1(Parachain(id)) } =>
				Ok(ParaId::from(*id).into_account()),
			_ => Err(()),
		}
	}

	fn reverse_ref(who: impl Borrow<AccountId>) -> Result<MultiLocation, ()> {
		if let Some(id) = ParaId::try_from_account(who.borrow()) {
			Ok(Parachain(id.into()).into())
		} else {
			Err(())
		}
	}
}

pub struct SiblingParachainConvertsVia<ParaId, AccountId>(PhantomData<(ParaId, AccountId)>);
impl<ParaId: From<u32> + Into<u32> + AccountIdConversion<AccountId>, AccountId: Clone>
	Convert<MultiLocation, AccountId> for SiblingParachainConvertsVia<ParaId, AccountId>
{
	fn convert_ref(location: impl Borrow<MultiLocation>) -> Result<AccountId, ()> {
		match location.borrow() {
			MultiLocation { parents: 1, interior: X1(Parachain(id)) } =>
				Ok(ParaId::from(*id).into_account()),
			_ => Err(()),
		}
	}

	fn reverse_ref(who: impl Borrow<AccountId>) -> Result<MultiLocation, ()> {
		if let Some(id) = ParaId::try_from_account(who.borrow()) {
			Ok(MultiLocation::new(1, X1(Parachain(id.into()))))
		} else {
			Err(())
		}
	}
}

/// Extracts the `AccountId32` from the passed `location` if the network matches.
pub struct AccountId32Aliases<Network, AccountId>(PhantomData<(Network, AccountId)>);
impl<Network: Get<NetworkId>, AccountId: From<[u8; 32]> + Into<[u8; 32]> + Clone>
	Convert<MultiLocation, AccountId> for AccountId32Aliases<Network, AccountId>
{
	fn convert(location: MultiLocation) -> Result<AccountId, MultiLocation> {
		let id = match location {
			MultiLocation {
				parents: 0,
				interior: X1(AccountId32 { id, network: NetworkId::Any }),
			} => id,
			MultiLocation { parents: 0, interior: X1(AccountId32 { id, network }) }
				if network == Network::get() =>
				id,
			_ => return Err(location),
		};
		Ok(id.into())
	}

	fn reverse(who: AccountId) -> Result<MultiLocation, AccountId> {
		Ok(AccountId32 { id: who.into(), network: Network::get() }.into())
	}
}

pub struct AccountKey20Aliases<Network, AccountId>(PhantomData<(Network, AccountId)>);
impl<Network: Get<NetworkId>, AccountId: From<[u8; 20]> + Into<[u8; 20]> + Clone>
	Convert<MultiLocation, AccountId> for AccountKey20Aliases<Network, AccountId>
{
	fn convert(location: MultiLocation) -> Result<AccountId, MultiLocation> {
		let key = match location {
			MultiLocation {
				parents: 0,
				interior: X1(AccountKey20 { key, network: NetworkId::Any }),
			} => key,
			MultiLocation { parents: 0, interior: X1(AccountKey20 { key, network }) }
				if network == Network::get() =>
				key,
			_ => return Err(location),
		};
		Ok(key.into())
	}

	fn reverse(who: AccountId) -> Result<MultiLocation, AccountId> {
		let j = AccountKey20 { key: who.into(), network: Network::get() };
		Ok(j.into())
	}
}

/// Simple location inverter; give it this location's ancestry and it'll figure out the inverted
/// location.
///
/// # Example
/// ## Network Topology
/// ```txt
///                    v Source
/// Relay -> Para 1 -> Account20
///       -> Para 2 -> Account32
///                    ^ Target
/// ```
/// ```rust
/// # use frame_support::parameter_types;
/// # use xcm::latest::{MultiLocation, Junction::*, Junctions::{self, *}, NetworkId::Any};
/// # use xcm_builder::LocationInverter;
/// # use xcm_executor::traits::InvertLocation;
/// # fn main() {
/// parameter_types!{
///     pub Ancestry: MultiLocation = X2(
///         Parachain(1),
///         AccountKey20 { network: Any, key: Default::default() },
///     ).into();
/// }
///
/// let input = MultiLocation::new(2, X2(Parachain(2), AccountId32 { network: Any, id: Default::default() }));
/// let inverted = LocationInverter::<Ancestry>::invert_location(&input);
/// assert_eq!(inverted, Ok(MultiLocation::new(
///     2,
///     X2(Parachain(1), AccountKey20 { network: Any, key: Default::default() }),
/// )));
/// # }
/// ```
pub struct LocationInverter<Ancestry>(PhantomData<Ancestry>);
impl<Ancestry: Get<MultiLocation>> InvertLocation for LocationInverter<Ancestry> {
	fn invert_location(location: &MultiLocation) -> Result<MultiLocation, ()> {
		let mut ancestry = Ancestry::get();
		let mut junctions = Here;
		for _ in 0..location.parent_count() {
			junctions = junctions
				.pushed_with(ancestry.take_first_interior().unwrap_or(OnlyChild))
				.map_err(|_| ())?;
		}
		let parents = location.interior().len() as u8;
		Ok(MultiLocation::new(parents, junctions))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use frame_support::parameter_types;
	use xcm::latest::{Junction, MultiAsset, NetworkId::Any};

	fn account20() -> Junction {
		AccountKey20 { network: Any, key: Default::default() }
	}

	fn account32() -> Junction {
		AccountId32 { network: Any, id: Default::default() }
	}

	// Network Topology
	//                                     v Source
	// Relay -> Para 1 -> SmartContract -> Account
	//       -> Para 2 -> Account
	//                    ^ Target
	//
	// Inputs and outputs written as file paths:
	//
	// input location (source to target): ../../../para_2/account32_default
	// ancestry (root to source): para_1/account20_default/account20_default
	// =>
	// output (target to source): ../../para_1/account20_default/account20_default
	#[test]
	fn inverter_works_in_tree() {
		parameter_types! {
			pub Ancestry: MultiLocation = X3(Parachain(1), account20(), account20()).into();
		}

		let input = MultiLocation::new(3, X2(Parachain(2), account32()));
		let inverted = LocationInverter::<Ancestry>::invert_location(&input).unwrap();
		assert_eq!(inverted, MultiLocation::new(2, X3(Parachain(1), account20(), account20())));
	}

	// Network Topology
	//                                     v Source
	// Relay -> Para 1 -> SmartContract -> Account
	//          ^ Target
	#[test]
	fn inverter_uses_ancestry_as_inverted_location() {
		parameter_types! {
			pub Ancestry: MultiLocation = X2(account20(), account20()).into();
		}

		let input = MultiLocation::grandparent();
		let inverted = LocationInverter::<Ancestry>::invert_location(&input).unwrap();
		assert_eq!(inverted, X2(account20(), account20()).into());
	}

	// Network Topology
	//                                        v Source
	// Relay -> Para 1 -> CollectivePallet -> Plurality
	//          ^ Target
	#[test]
	fn inverter_uses_only_child_on_missing_ancestry() {
		parameter_types! {
			pub Ancestry: MultiLocation = X1(PalletInstance(5)).into();
		}

		let input = MultiLocation::grandparent();
		let inverted = LocationInverter::<Ancestry>::invert_location(&input).unwrap();
		assert_eq!(inverted, X2(PalletInstance(5), OnlyChild).into());
	}

	#[test]
	fn inverter_errors_when_location_is_too_large() {
		parameter_types! {
			pub Ancestry: MultiLocation = Here.into();
		}

		let input = MultiLocation { parents: 99, interior: X1(Parachain(88)) };
		let inverted = LocationInverter::<Ancestry>::invert_location(&input);
		assert_eq!(inverted, Err(()));
	}

	#[test]
	fn test1() {
		// let ancestry: MultiLocation = (Parachain(1000), PalletInstance(42)).into();
		// 		let target = (Parent, PalletInstance(69)).into();
		// 		let expected = (Parent, PalletInstance(42)).into();
		// 		let inverted = ancestry.inverted(&target).unwrap();
		// 		assert_eq!(inverted, expected);
	}

	#[test]
	fn test_invert_take_first() {
		let mut ancestry: MultiLocation = (Parachain(1000), PalletInstance(42)).into();
		assert_eq!(ancestry.interior, X2(Parachain(1000), PalletInstance(42)));
		assert_eq!(2, ancestry.interior.len());

		let first = ancestry.take_first_interior();
		assert_eq!(first.unwrap(), Parachain(1000));
		assert_eq!(ancestry, (0, PalletInstance(42)).into());

		let first = ancestry.take_first_interior();
		assert_eq!(first.unwrap(), PalletInstance(42));
		assert_eq!(ancestry, (0, Here).into());

		let first = ancestry.take_first_interior();
		assert_eq!(first, None);
		assert_eq!(ancestry, (0, Here).into());

		let first = ancestry.take_first_interior();
		assert_eq!(first, None);
		assert_eq!(ancestry, (0, Here).into());

		let location: MultiLocation = (2, Here).into();
		assert_eq!(0, location.interior.len());
		let location: MultiLocation = (1, Here).into();
		assert_eq!(0, location.interior.len());
	}

	#[test]
	fn test_invert_para_pallet() {
		parameter_types! {
			pub Ancestry: MultiLocation = (Parachain(1000), PalletInstance(42)).into();
		}

		let location: MultiLocation = (0, Here).into();
		let invert = LocationInverter::<Ancestry>::invert_location(&location).unwrap();
		assert_eq!(0, invert.parents);
		assert_eq!(Here, invert.interior);

		let location: MultiLocation = (1, Here).into();
		let invert = LocationInverter::<Ancestry>::invert_location(&location).unwrap();
		assert_eq!(0, invert.parents);
		assert_eq!(X1(Parachain(1000)), invert.interior);

		let location: MultiLocation = (2, Here).into();
		let invert = LocationInverter::<Ancestry>::invert_location(&location).unwrap();
		assert_eq!(0, invert.parents);
		assert_eq!(X2(Parachain(1000), PalletInstance(42)), invert.interior);

		let location: MultiLocation = (3, Here).into();
		let invert = LocationInverter::<Ancestry>::invert_location(&location).unwrap();
		assert_eq!(0, invert.parents);
		assert_eq!(X3(Parachain(1000), PalletInstance(42), OnlyChild), invert.interior);
	}

	#[test]
	fn test_invert_para() {
		parameter_types! {
			pub Ancestry: MultiLocation = (Parachain(1000)).into();
		}

		let location: MultiLocation = (1, Here).into();
		let invert = LocationInverter::<Ancestry>::invert_location(&location).unwrap();
		assert_eq!(0, invert.parents);
		assert_eq!(X1(Parachain(1000)), invert.interior);

		let location: MultiLocation = (2, Here).into();
		let invert = LocationInverter::<Ancestry>::invert_location(&location).unwrap();
		assert_eq!(0, invert.parents);
		assert_eq!(X2(Parachain(1000), OnlyChild), invert.interior);

		let location: MultiLocation = (3, Here).into();
		let invert = LocationInverter::<Ancestry>::invert_location(&location).unwrap();
		assert_eq!(0, invert.parents);
		assert_eq!(X3(Parachain(1000), OnlyChild, OnlyChild), invert.interior);
	}

	#[test]
	fn test_invert_location_cases() {
		// Ancestry: root(Relay) -> source(Pallet42): (Parachain(1000), PalletInstance(42))
		// source->target: Parent, Pallet(69)
		// target->source: ?
		//
		//                    v Source
		// Relay -> Para 1 -> Pallet(42)
		//            |
		//            |-----> Pallet(69)
		//                    ^ Target
		parameter_types! {
			pub Ancestry: MultiLocation = (Parachain(1000), PalletInstance(42)).into();
		}
		let target = (Parent, PalletInstance(69)).into();
		let inverted = LocationInverter::<Ancestry>::invert_location(&target).unwrap();
		assert_eq!(inverted.parents, 1);
		assert_eq!(inverted.interior, X1(Parachain(1000)));

		// Ancestry: root(Relay) -> source(Pallet42): (Parachain(1000), PalletInstance(42))
		// source->target: 2, Pallet(69)
		// target->source: ?
		//
		//                    v Source
		// Relay -> Para 1 -> Pallet(42)
		//   |
		//   |-----> Pallet(69)
		//           ^ Target
		let target = (2, PalletInstance(69)).into();
		let inverted = LocationInverter::<Ancestry>::invert_location(&target).unwrap();
		assert_eq!(inverted.parents, 1);
		assert_eq!(inverted.interior, X2(Parachain(1000), PalletInstance(42)));

		//                    v Source
		// Relay -> Para 1 -> Pallet(42)
		//       -> Para 2 -> Pallet(43)
		//                    ^ Target
		let location: MultiLocation = (2, X2(Parachain(2000), PalletInstance(43))).into();
		let invert = LocationInverter::<Ancestry>::invert_location(&location).unwrap();
		assert_eq!(2, invert.parents);
		assert_eq!(X2(Parachain(1000), PalletInstance(42)), invert.interior);

		//                                  v Source
		// Relay -> Para 1 -> Pallet(42) -> Pallet(52)
		//       -> Para 2 -> Pallet(43)
		//                    ^ Target
		parameter_types! {
			pub Ancestry2: MultiLocation = (Parachain(1000), PalletInstance(42), PalletInstance(52)).into();
		}
		let location: MultiLocation = (3, X2(Parachain(2000), PalletInstance(43))).into();
		let invert = LocationInverter::<Ancestry2>::invert_location(&location).unwrap();
		assert_eq!(2, invert.parents);
		assert_eq!(X3(Parachain(1000), PalletInstance(42), PalletInstance(52)), invert.interior);

		//                                  v Source
		// Relay -> Para 1 -> Pallet(42) -> Pallet(52)
		//       -> Para 2 -> Pallet(43) -> Pallet(53)
		//                                  ^ Target
		let location: MultiLocation = (3, X3(Parachain(2000), PalletInstance(43), PalletInstance(53))).into();
		let invert = LocationInverter::<Ancestry2>::invert_location(&location).unwrap();
		assert_eq!(3, invert.parents);
		assert_eq!(X3(Parachain(1000), PalletInstance(42), PalletInstance(52)), invert.interior);


		//                                  v Source
		// Relay -> Para 1 -> Pallet(42) -> GeneralIndex(1)
		//                 -> Pallet(69) -> GeneralIndex(2)
		//                    ^ Target
		parameter_types! {
			pub Ancestry3: MultiLocation = (Parachain(1000), PalletInstance(42), GeneralIndex(1)).into();
		}
		let location: MultiLocation = (Parent, Parent, PalletInstance(69), GeneralIndex(2)).into();
		let invert = LocationInverter::<Ancestry3>::invert_location(&location).unwrap();
		assert_eq!(2, invert.parents);
		assert_eq!(X2(Parachain(1000), PalletInstance(42)), invert.interior);
	}

	#[test]
	fn test_parachain_reanchor_sibling() {
		use frame_support::parameter_types;

		// para.rs runtime config
		parameter_types! {
			pub Ancestry: MultiLocation = X1(Parachain(1)).into();
		}

		// if the original origin is in parachain, and the destination is an sibling parachain
		// then we could imaging this is an xcmp message which parachain send to parachain.
		let dest: MultiLocation = (Parent, Parachain(2)).into();

		// Parachain(1) invert (Parent,Parachain(2)) = (Parent, Parachain(1))
		let inv_dest = LocationInverter::<Ancestry>::invert_location(&dest).unwrap();
		assert_eq!(inv_dest, (1, Parachain(1)).into());

		// (Here, 100).reanchor((Parent,Parachain(1))) results ((Parent, Parachain(1)), 100)
		// this is for the case of A -> [A] -> B
		// (Here,100) means token A, then in para(2), (1, Para(1)) can express the meaning of token A asset
		let mut asset: MultiAsset = (Here, 100u128).into();
		asset.reanchor(&inv_dest);
		assert_eq!(asset, ((Parent, Parachain(1)), 100).into());
		assert_eq!(asset, ((1, Parachain(1)), 100).into());

		// (Parent, 100).reanchor((Parent,Parachain(1)))
		// this is for the case of A -> R -> B. i.e. Karura transfer KSM to Bifrost.
		// so here (Parent, 100) means 100 KSM in karura, and in bifrost,
		// it also use (Parent, 100) to express 100 KSM.
		let mut asset: MultiAsset = (Parent, 100u128).into();
		asset.reanchor(&inv_dest);
		assert_eq!(asset, (Parent, 100).into());

		// (Parachain(1), 100).reanchor((Parent,Parachain(1)))
		// it's meaningless, but here we use here just for the testcase
		let mut asset: MultiAsset = ((0, Parachain(1)), 100u128).into();
		asset.reanchor(&inv_dest);
		assert_eq!(asset, ((1, X2(Parachain(1), Parachain(1))), 100).into());

		// (0, Parachain(1)) is like Parachain(1), so it's also meaningless
		let mut asset: MultiAsset = (Parachain(1), 100u128).into();
		asset.reanchor(&inv_dest);
		assert_eq!(asset, ((1, X2(Parachain(1), Parachain(1))), 100).into());

		// (GeneralIndex(42), 100).reanchor((Parent,Parachain(1)))
		// the original is GeneralIndex belonging to origin parachain(1)
		// then in the Para(2) side, it needs first get into Para(1), then get into GeneralIndex
		let mut asset: MultiAsset = ((0, GeneralIndex(42)), 100u128).into();
		asset.reanchor(&inv_dest);
		assert_eq!(asset, ((1, X2(Parachain(1), GeneralIndex(42))), 100).into());
	}

	#[test]
	fn test_sibling_reanchor() {
		use frame_support::parameter_types;
		parameter_types! {
			pub Ancestry: MultiLocation = X1(Parachain(2001)).into();
		}
		let dest: MultiLocation = (Parent, Parachain(2000)).into();
		let inv_dest = LocationInverter::<Ancestry>::invert_location(&dest).unwrap();
		assert_eq!(inv_dest, (1, Parachain(2001)).into());

		// the GeneralIndex belong to Parachain(2001)
		let mut asset: MultiAsset = ((0, GeneralIndex(42)), 100u128).into();
		asset.reanchor(&inv_dest);
		assert_eq!(asset, ((1, X2(Parachain(2001), GeneralIndex(42))), 100).into());

		// the GeneralIndex belong to Parachain(2000)
		let mut asset: MultiAsset = ((Parent, X2(Parachain(2000), GeneralIndex(42))), 100u128).into();
		asset.reanchor(&inv_dest);
		assert_eq!(asset, ((1, X2(Parachain(2000), GeneralIndex(42))), 100).into());
	}

	#[test]
	fn test_sibling_reanchor_tokens() {
		use frame_support::parameter_types;
		parameter_types! {
			pub Ancestry: MultiLocation = X1(Parachain(2001)).into();
		}
		let dest: MultiLocation = (1, Parachain(2000)).into();
		let inv_dest = LocationInverter::<Ancestry>::invert_location(&dest).unwrap();
		assert_eq!(inv_dest, (1, Parachain(2001)).into());

		// the GeneralKey(BNC) belong to Parachain(2001)
		let mut asset: MultiAsset = ((0, GeneralKey("BNC".as_bytes().to_vec())), 100u128).into();
		asset.reanchor(&inv_dest);
		assert_eq!(asset, ((1, X2(Parachain(2001), GeneralKey("BNC".as_bytes().to_vec()))), 100).into());

		// the GeneralKey(KAR) and GeneralKey(KUSD) belong to Parachain(2000)
		let mut asset: MultiAsset = ((1, X2(Parachain(2000), GeneralKey("KAR".as_bytes().to_vec()))), 100u128).into();
		asset.reanchor(&inv_dest);
		assert_eq!(asset, ((1, X2(Parachain(2000), GeneralKey("KAR".as_bytes().to_vec()))), 100).into());

		let mut asset: MultiAsset = ((1, Parachain(2000), GeneralKey("KUSD".as_bytes().to_vec())), 100u128).into();
		asset.reanchor(&inv_dest);
		assert_eq!(asset, ((1, Parachain(2000), GeneralKey("KUSD".as_bytes().to_vec())), 100).into());
	}
}
