//! Chain state. Deliberately simpler than the Gray Paper's: services, their code and
//! their storage, hashed as one sorted encoding rather than a Merkle trie. Good enough
//! for every validator to prove it reached the same state; not enough for light clients.

use blake2::{digest::consts::U32, Blake2b, Digest};
use codec::Encode;
use std::collections::BTreeMap;

pub type Hash = [u8; 32];

pub fn hash(bytes: &[u8]) -> Hash {
	Blake2b::<U32>::digest(bytes).into()
}

#[derive(Clone, Default, Debug)]
pub struct Account {
	pub code: Vec<u8>,
	pub storage: BTreeMap<Vec<u8>, Vec<u8>>,
	pub balance: u64,
}

#[derive(Clone, Default, Debug)]
pub struct State {
	pub slot: u32,
	pub services: BTreeMap<u32, Account>,
}

impl State {
	pub fn root(&self) -> Hash {
		let mut buf = Vec::new();
		self.slot.encode_to(&mut buf);
		for (id, a) in &self.services {
			id.encode_to(&mut buf);
			hash(&a.code).encode_to(&mut buf);
			a.balance.encode_to(&mut buf);
			let items: Vec<(&Vec<u8>, &Vec<u8>)> = a.storage.iter().collect();
			items.encode_to(&mut buf);
		}
		hash(&buf)
	}
}
