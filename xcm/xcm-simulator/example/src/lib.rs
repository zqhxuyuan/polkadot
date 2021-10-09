// Copyright 2021 Parity Technologies (UK) Ltd.
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

mod parachain;
mod relay_chain;

use polkadot_parachain::primitives::Id as ParaId;
use sp_runtime::traits::AccountIdConversion;
use xcm_simulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain};

pub const ALICE: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([0u8; 32]);
pub const BOB: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([1u8; 32]);
pub const INITIAL_BALANCE: u128 = 1_000_000_000;

decl_test_parachain! {
	pub struct ParaA {
		Runtime = parachain::Runtime,
		XcmpMessageHandler = parachain::MsgQueue,
		DmpMessageHandler = parachain::MsgQueue,
		new_ext = para_ext(1),
	}
}

decl_test_parachain! {
	pub struct ParaB {
		Runtime = parachain::Runtime,
		XcmpMessageHandler = parachain::MsgQueue,
		DmpMessageHandler = parachain::MsgQueue,
		new_ext = para_ext(2),
	}
}

decl_test_relay_chain! {
	pub struct Relay {
		Runtime = relay_chain::Runtime,
		XcmConfig = relay_chain::XcmConfig,
		new_ext = relay_ext(),
	}
}

decl_test_network! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(1, ParaA),
			(2, ParaB),
		],
	}
}

pub fn para_account_id(id: u32) -> relay_chain::AccountId {
	ParaId::from(id).into_account()
}

pub fn para_ext(para_id: u32) -> sp_io::TestExternalities {
	use parachain::{MsgQueue, Runtime, System};

	let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
			(ALICE, INITIAL_BALANCE)]
	}
		.assimilate_storage(&mut t)
		.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		MsgQueue::set_para_id(para_id.into());
	});
	ext
}

pub fn relay_ext() -> sp_io::TestExternalities {
	use relay_chain::{Runtime, System};

	let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
			(ALICE, INITIAL_BALANCE),
			(para_account_id(1), INITIAL_BALANCE)],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub type RelayChainPalletXcm = pallet_xcm::Pallet<relay_chain::Runtime>;
pub type ParachainPalletXcm = pallet_xcm::Pallet<parachain::Runtime>;
pub type RelayChainProxyPallet = pallet_proxy::Pallet<relay_chain::Runtime>;

#[cfg(test)]
mod tests {
	use super::*;

	use codec::Encode;
	use frame_support::assert_ok;
	use xcm::latest::prelude::*;
	use xcm_simulator::TestExt;
	use crate::relay_chain::ProxyType;
	use frame_system::ensure_signed;

	// Helper function for forming buy execution message
	fn buy_execution<C>(fees: impl Into<MultiAsset>) -> Instruction<C> {
		BuyExecution { fees: fees.into(), weight_limit: Unlimited }
	}

	#[test]
	fn dmp() {
		MockNet::reset();

		let remark =
			parachain::Call::System(frame_system::Call::<parachain::Runtime>::remark_with_event {
				remark: vec![1, 2, 3],
			});
		Relay::execute_with(|| {
			assert_ok!(RelayChainPalletXcm::send_xcm(
				Here,
				Parachain(1).into(),
				Xcm(vec![Transact {
					origin_type: OriginKind::SovereignAccount,
					require_weight_at_most: INITIAL_BALANCE as u64,
					call: remark.encode().into(),
				}]),
			));
		});

		ParaA::execute_with(|| {
			use parachain::{Event, System};
			assert!(System::events()
				.iter()
				.any(|r| matches!(r.event, Event::System(frame_system::Event::Remarked(_, _)))));
		});
	}

	#[test]
	fn simple_proxy_works() {
		MockNet::reset();

		Relay::execute_with(|| {
			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&ALICE), INITIAL_BALANCE);
			assert_eq!(relay_chain::Balances::free_balance(&ALICE), INITIAL_BALANCE);
			assert_eq!(relay_chain::Balances::free_balance(&BOB), 0);

			// this data should be consider with Proxy config on relay_chain runtime
			let Proxy_Fee: u128 = 1000;

			// make the call to be proxying
			let call = Box::new(relay_chain::Call::Balances(
				pallet_balances::Call::<relay_chain::Runtime>::transfer {
					dest: BOB, // transfer money to Bob, which means Bob can spend Alice's money
					value: 1000,
				},
			));

			// Alice proxy to Bob
			assert_ok!(RelayChainProxyPallet::add_proxy(
				relay_chain::Origin::signed(ALICE),
				BOB,
				ProxyType::Any,
				0
			));

			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&ALICE), INITIAL_BALANCE - Proxy_Fee);

			// Bob can do Alice's job
			relay_chain::Proxy::proxy(
				relay_chain::Origin::signed(BOB),
				ALICE,
				None,
				call.clone() // do the transfer
			);

			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&ALICE), INITIAL_BALANCE - Proxy_Fee * 2);
			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&BOB), Proxy_Fee);
		});
	}

	#[test]
	fn anonymous_proxy() {
		MockNet::reset();

		Relay::execute_with(|| {
			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&ALICE), INITIAL_BALANCE);
			assert_eq!(relay_chain::Balances::free_balance(&ALICE), INITIAL_BALANCE);
			assert_eq!(relay_chain::Balances::free_balance(&BOB), 0);

			// this data should be consider with Proxy config on relay_chain runtime
			let Proxy_Fee: u128 = 1000;

			// transfer amount
			let transfer_amount_to_anon: u128 = 2000;
			let transfer_amount: u128 = 1000;

			// make the call to be proxying
			let call = Box::new(relay_chain::Call::Balances(
				pallet_balances::Call::<relay_chain::Runtime>::transfer {
					dest: BOB, // transfer money to Bob, which means Bob can spend Alice's money
					value: transfer_amount,
				},
			));

			// Alice proxy to Bob
			// assert_ok!(RelayChainProxyPallet::add_proxy(
			// 	relay_chain::Origin::signed(ALICE),
			// 	BOB,
			// 	ProxyType::Any,
			// 	0
			// ));

			RelayChainProxyPallet::anonymous(
				relay_chain::Origin::signed(ALICE),
				ProxyType::Any,
				0,
				0
			);

			let anon = RelayChainProxyPallet::anonymous_account(&ALICE, &ProxyType::Any, 0, None);

			// the real is anon, and the delegate is ALICE. key=anon is in the storage
			let result = RelayChainProxyPallet::find_proxy(&anon, &ALICE,Some(ProxyType::Any)).unwrap();
			println!("proxydef:{:?}", result);
			assert_eq!(result.delegate, ALICE);
			// if in turn, the result is empty, because there are no real proxy: Alice in the storage
			let result = RelayChainProxyPallet::find_proxy( &ALICE, &anon,Some(ProxyType::Any));
			assert_eq!(result.is_ok(), false);

			let signed = relay_chain::Origin::signed(ALICE);
			let who = ensure_signed(signed).unwrap();
			println!("alice:{}", who);

			let signed = relay_chain::Origin::signed(anon.clone());
			let who = ensure_signed(signed).unwrap();
			println!("anon:{}", who);
			assert_eq!(anon.clone(), who);

			// alice create an anonymous account, cost Proxy_Fee
			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&ALICE), INITIAL_BALANCE - Proxy_Fee);

			// transfer alice to anonymous account, so that anonymous account can transfer money
			relay_chain::Balances::transfer(relay_chain::Origin::signed(ALICE), anon.clone(), transfer_amount_to_anon);

			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&ALICE),
					   INITIAL_BALANCE - Proxy_Fee - transfer_amount_to_anon);
			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&anon), transfer_amount_to_anon);

			// anon can do Alice's job
			relay_chain::Proxy::proxy(
				relay_chain::Origin::signed(ALICE),
				anon.clone(),
				None,
				call.clone() // do the transfer
			);

			// alice's balance no change
			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&ALICE), INITIAL_BALANCE - Proxy_Fee - transfer_amount_to_anon);
			// anonymous balance reduce
			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&anon), transfer_amount_to_anon - transfer_amount);
			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&BOB), transfer_amount);

			// we can also use anonymous account to transfer
			// although anon has no private key, but it still can be Origin::signed()
			relay_chain::Balances::transfer(relay_chain::Origin::signed(anon.clone()), BOB, 100);
			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&anon), transfer_amount_to_anon - transfer_amount - 100);
			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&BOB), transfer_amount + 100);
		});
	}

	#[test]
	fn ump() {
		MockNet::reset();

		let remark = relay_chain::Call::System(
			frame_system::Call::<relay_chain::Runtime>::remark_with_event { remark: vec![1, 2, 3] },
		);
		ParaA::execute_with(|| {
			assert_ok!(ParachainPalletXcm::send_xcm(
				Here,
				Parent.into(),
				Xcm(vec![Transact {
					origin_type: OriginKind::SovereignAccount,
					require_weight_at_most: INITIAL_BALANCE as u64,
					call: remark.encode().into(),
				}]),
			));
		});

		Relay::execute_with(|| {
			use relay_chain::{Event, System};
			assert!(System::events()
				.iter()
				.any(|r| matches!(r.event, Event::System(frame_system::Event::Remarked(_, _)))));
		});
	}

	#[test]
	fn xcmp() {
		MockNet::reset();

		let remark =
			parachain::Call::System(frame_system::Call::<parachain::Runtime>::remark_with_event {
				remark: vec![1, 2, 3],
			});
		ParaA::execute_with(|| {
			assert_ok!(ParachainPalletXcm::send_xcm(
				Here,
				MultiLocation::new(1, X1(Parachain(2))),
				Xcm(vec![Transact {
					origin_type: OriginKind::SovereignAccount,
					require_weight_at_most: INITIAL_BALANCE as u64,
					call: remark.encode().into(),
				}]),
			));
		});

		ParaB::execute_with(|| {
			use parachain::{Event, System};
			assert!(System::events()
				.iter()
				.any(|r| matches!(r.event, Event::System(frame_system::Event::Remarked(_, _)))));
		});
	}

	#[test]
	fn reserve_transfer() {
		MockNet::reset();

		let withdraw_amount = 123;

		Relay::execute_with(|| {
			// initial balance of Alice and para_1
			assert_eq!(pallet_balances::Pallet::<parachain::Runtime>::free_balance(&ALICE), INITIAL_BALANCE);
			assert_eq!(parachain::Balances::free_balance(&para_account_id(1)), INITIAL_BALANCE);

			// transfer
			assert_ok!(RelayChainPalletXcm::reserve_transfer_assets(
				// origin
				relay_chain::Origin::signed(ALICE),
				// dest
				Box::new(X1(Parachain(1)).into().into()),
				// benificiary
				Box::new(X1(AccountId32 { network: Any, id: ALICE.into() }).into().into()),
				// Box::new(X1(AccountId32 { network: Any, id: BOB.into() }).into().into()),
				// !!!assets!!!
				// 资产类型为中继链的native token，如果想要转账平行链的资产，需要引入Currency与Location的映射关系
				// MultiLocation + amount -> MultiAssets
				Box::new((Here, withdraw_amount).into()),
				0,
			));

			// result
			assert_eq!(pallet_balances::Pallet::<relay_chain::Runtime>::free_balance(&ALICE), INITIAL_BALANCE - withdraw_amount);
			assert_eq!(pallet_balances::Pallet::<parachain::Runtime>::free_balance(&ALICE), INITIAL_BALANCE - withdraw_amount);
			assert_eq!(parachain::Balances::free_balance(&para_account_id(1)), INITIAL_BALANCE + withdraw_amount);
		});

		ParaA::execute_with(|| {
			assert_eq!(parachain::Balances::free_balance(&para_account_id(1)), 0);

			// free execution, full amount received
			assert_eq!(pallet_balances::Pallet::<parachain::Runtime>::free_balance(&ALICE), INITIAL_BALANCE + withdraw_amount);
			// assert_eq!(pallet_balances::Pallet::<parachain::Runtime>::free_balance(&BOB), withdraw_amount);
		});
	}


	fn reserve_transfer_by_hand() {

	}

	/// Scenario:
	/// A parachain transfers funds on the relay chain to another parachain account.
	///
	/// Asserts that the parachain accounts are updated as expected.
	#[test]
	fn withdraw_and_deposit() {
		MockNet::reset();

		let send_amount = 10;

		ParaA::execute_with(|| {
			let message = Xcm(vec![
				// 和上面reserve_transfer中的assets类似，MultiAssets都是(Here,amount).into()
				WithdrawAsset((Here, send_amount).into()),
				buy_execution((Here, send_amount)),
				DepositAsset {
					assets: All.into(),
					max_assets: 1,
					beneficiary: Parachain(2).into(),
				},
			]);
			// Send withdraw and deposit
			assert_ok!(ParachainPalletXcm::send_xcm(Here, Parent.into(), message.clone()));
		});

		Relay::execute_with(|| {
			assert_eq!(
				relay_chain::Balances::free_balance(para_account_id(1)),
				INITIAL_BALANCE - send_amount
			);
			assert_eq!(relay_chain::Balances::free_balance(para_account_id(2)), send_amount);
		});
	}

	/// Scenario:
	/// A parachain wants to be notified that a transfer worked correctly.
	/// It sends a `QueryHolding` after the deposit to get notified on success.
	///
	/// Asserts that the balances are updated correctly and the expected XCM is sent.
	#[test]
	fn query_holding() {
		MockNet::reset();

		let send_amount = 10;
		let query_id_set = 1234;

		// Send a message which fully succeeds on the relay chain
		ParaA::execute_with(|| {
			let message = Xcm(vec![
				WithdrawAsset((Here, send_amount).into()),
				buy_execution((Here, send_amount)),
				DepositAsset {
					assets: All.into(),
					max_assets: 1,
					beneficiary: Parachain(2).into(),
				},
				QueryHolding {
					query_id: query_id_set,
					dest: Parachain(1).into(),
					assets: All.into(),
					max_response_weight: 1_000_000_000,
				},
			]);
			// Send withdraw and deposit with query holding
			assert_ok!(ParachainPalletXcm::send_xcm(Here, Parent.into(), message.clone(),));
		});

		// Check that transfer was executed
		Relay::execute_with(|| {
			// Withdraw executed
			assert_eq!(
				relay_chain::Balances::free_balance(para_account_id(1)),
				INITIAL_BALANCE - send_amount
			);
			// Deposit executed
			assert_eq!(relay_chain::Balances::free_balance(para_account_id(2)), send_amount);
		});

		// Check that QueryResponse message was received
		ParaA::execute_with(|| {
			assert_eq!(
				parachain::MsgQueue::received_dmp(),
				vec![Xcm(vec![QueryResponse {
					query_id: query_id_set,
					response: Response::Assets(MultiAssets::new()),
					max_weight: 1_000_000_000,
				}])],
			);
		});
	}
}
